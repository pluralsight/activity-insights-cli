use fs2::FileExt;
use log::warn;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;
use thiserror::Error;
use uuid::Uuid;

use crate::{constants, ActivityInsightsError};

#[derive(Error, Debug)]
pub enum CredentialsError {
    #[error("Performing an update requires an exclusive lock on the credentials file")]
    NeedsExclusiveLock,

    #[error("You already have an exclusive lock on this file")]
    HasExclusiveLock,

    #[error("An api token was already set")]
    HasApiToken,

    #[error("An api token is required to update the api token in the lock file")]
    ApiTokenRequired,

    #[error("Error deserializing the credentials file: {0}")]
    DeserializationError(#[from] serde_yaml::Error),
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct Credentials {
    api_token: Option<Uuid>,
    latest_accepted_tos: Option<usize>,
    #[serde(skip)]
    location: PathBuf,
}

impl Credentials {
    pub fn fetch() -> Result<Self, ActivityInsightsError> {
        let creds_dir = dirs::home_dir()
            .map(|dir| dir.join(constants::PS_DIR))
            .ok_or_else(|| {
                ActivityInsightsError::Other(String::from("Can't find the home directory"))
            })?;

        Self::fetch_from_dir(&creds_dir)
    }

    fn fetch_from_dir(dir: &Path) -> Result<Self, ActivityInsightsError> {
        let path = dir.join(constants::CRED_FILE_NAME);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&path)
            .map_err(|e| ActivityInsightsError::IO(path.to_path_buf(), e))?;

        let creds: Credentials = match serde_yaml::from_reader(file) {
            Ok(creds) => Credentials {
                location: dir.to_path_buf(),
                ..creds
            },
            Err(e) => {
                warn!("Error deserializing yaml: {}", e);
                Credentials {
                    location: dir.to_path_buf(),
                    ..Default::default()
                }
            }
        };
        Ok(creds)
    }

    fn fetch_latest(&self) -> Result<Credentials, ActivityInsightsError> {
        Self::fetch_from_dir(&self.location)
    }

    fn creds_file_path(&self) -> PathBuf {
        self.location.join(constants::CRED_FILE_NAME)
    }

    fn ephemeral_update_file(&self) -> Result<NamedTempFile, ActivityInsightsError> {
        NamedTempFile::new_in(&self.location)
            .map_err(|e| ActivityInsightsError::IO(self.location.to_path_buf(), e))
    }

    fn lock_file_path(&self) -> PathBuf {
        self.location.join(constants::LOCK_FILE_NAME)
    }

    pub fn api_token(&self) -> &Option<Uuid> {
        &self.api_token
    }

    pub fn has_accepted_latest(&self, latest_version: usize) -> bool {
        if let Some(val) = self.latest_accepted_tos {
            val >= latest_version
        } else {
            false
        }
    }

    pub fn lock(&mut self) -> Result<CredentialsGuard, ActivityInsightsError> {
        CredentialsGuard::new(&self.lock_file_path())
    }

    /// create_api_token only adds an api token if one is not already there. This prevents the user
    /// from overriding and api token that they have already successfully registered with. If an api
    /// token is already in the file but the user is not registered, try registering with the api
    /// token that is in the file.
    pub fn create_api_token(&mut self) -> Result<Uuid, ActivityInsightsError> {
        if self.api_token().is_some() {
            return Err(CredentialsError::HasApiToken.into());
        }

        let lock = self.lock()?;
        let mut fresh_creds = self.fetch_latest()?;

        if fresh_creds.api_token().is_some() {
            return Err(CredentialsError::HasApiToken.into());
        }

        let new_token = Uuid::new_v4();
        fresh_creds.api_token = Some(new_token);

        if let Err(e) = lock.update(&fresh_creds) {
            return Err(e);
        }

        Ok(new_token)
    }

    pub fn accept_tos(&mut self, tos_version: usize) -> Result<(), ActivityInsightsError> {
        let lock = self.lock()?;

        let mut fresh_creds = self.fetch_latest()?;
        fresh_creds.latest_accepted_tos = Some(tos_version);

        lock.update(&fresh_creds)?;

        Ok(())
    }
}

/// Responsible for controlling the lock on the Credentials file and updating the credentials to disk. Lock is released when it goes out of
/// scope
#[derive(Debug)]
pub struct CredentialsGuard {
    lock_file: File,
}

impl CredentialsGuard {
    pub fn new(path: &Path) -> Result<Self, ActivityInsightsError> {
        let lock_file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(&path)
            .map_err(|e| ActivityInsightsError::IO(path.to_path_buf(), e))?;

        lock_file
            .try_lock_exclusive()
            .map_err(|e| ActivityInsightsError::IO(path.to_path_buf(), e))?;
        Ok(CredentialsGuard { lock_file })
    }

    fn update(&self, creds: &Credentials) -> Result<(), ActivityInsightsError> {
        let ephemeral_update_file = creds.ephemeral_update_file()?;
        let credentials_file = creds.creds_file_path();
        fs::write(
            &ephemeral_update_file,
            serde_yaml::to_vec(creds).map_err(CredentialsError::from)?,
        )
        .map_err(|e| ActivityInsightsError::IO(ephemeral_update_file.path().to_path_buf(), e))?;
        fs::rename(ephemeral_update_file.path(), &credentials_file).map_err(|e| {
            ActivityInsightsError::IO(ephemeral_update_file.path().to_path_buf(), e)
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn create_api_token() {
        let fake_dir = tempdir().unwrap();

        let mut creds = Credentials::fetch_from_dir(fake_dir.path()).unwrap();
        let api_token = creds.create_api_token().unwrap();

        let updated_creds = Credentials::fetch_from_dir(fake_dir.path()).unwrap();

        assert_eq!(updated_creds.api_token, Some(api_token));
    }

    #[test]
    fn api_token_lock_failure() {
        let fake_dir = tempdir().unwrap();

        let mut creds_with_lock = Credentials::fetch_from_dir(fake_dir.path()).unwrap();
        let mut creds_without_lock = Credentials::fetch_from_dir(fake_dir.path()).unwrap();

        let _lock = creds_with_lock.lock().unwrap();
        match creds_without_lock.create_api_token() {
            Err(ActivityInsightsError::IO(_, e)) => {
                assert_eq!(e.kind(), std::io::ErrorKind::WouldBlock)
            }
            _ => assert!(false),
        }
    }

    #[test]
    fn api_token_releases_lock() {
        let fake_dir = tempdir().unwrap();

        let mut api_creds = Credentials::fetch_from_dir(fake_dir.path()).unwrap();
        api_creds.create_api_token().unwrap();

        let mut creds = Credentials::fetch_from_dir(fake_dir.path()).unwrap();
        creds.lock().unwrap();
    }

    #[test]
    fn accept_tos() {
        let fake_dir = tempdir().unwrap();

        let mut creds = Credentials::fetch_from_dir(fake_dir.path()).unwrap();
        creds.accept_tos(100).unwrap();

        let updated_creds = Credentials::fetch_from_dir(fake_dir.path()).unwrap();
        assert_eq!(updated_creds.latest_accepted_tos, Some(100));
    }

    #[test]
    fn chaos_test() {
        let fake_dir = tempdir().unwrap();
        let fake_path = fake_dir.path();

        let mut creds = Credentials::fetch_from_dir(fake_path).unwrap();
        let api_token = creds.create_api_token().unwrap();

        for i in 0..=100 {
            let mut creds = Credentials::fetch_from_dir(fake_path).unwrap();
            #[allow(unused_must_use)]
            {
                creds.create_api_token();
            }
            creds.accept_tos(i).unwrap();
        }

        let updated_creds = Credentials::fetch_from_dir(fake_path).unwrap();
        let actual = (updated_creds.api_token, updated_creds.latest_accepted_tos);
        let expected = (Some(api_token), Some(100));
        assert_eq!(actual, expected);
    }

    #[test]
    fn lock_releases_on_drop() {
        let fake_dir = tempdir().unwrap();

        let mut creds1 = Credentials::fetch_from_dir(fake_dir.path()).unwrap();
        let lock = creds1.lock().unwrap();
        drop(lock);

        let mut creds2 = Credentials::fetch_from_dir(fake_dir.path()).unwrap();
        creds2.lock().unwrap();
    }
}
