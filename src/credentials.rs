use fs2::FileExt;
use log::warn;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};
use thiserror::Error;
use uuid::Uuid;

use crate::{ActivityInsightsError, PS_DIR};

const CRED_FILE_NAME: &str = "credentials.yaml";
const UPDATE_FILE_NAME: &str = ".updated.credentials.yaml";
const LOCK_FILE_NAME: &str = "credentials.yaml.lock";

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
    latest_accepted_tos: Option<u8>,
    #[serde(skip)]
    lock_file: Option<File>,
    #[serde(skip)]
    location: PathBuf,
}

impl Credentials {
    pub fn fetch() -> Result<Self, ActivityInsightsError> {
        let creds_dir = dirs::home_dir()
            .map(|dir| dir.join(PS_DIR))
            .ok_or_else(|| {
                ActivityInsightsError::Other(String::from("Can't find the home directory"))
            })?;

        Self::fetch_from_dir(&creds_dir)
    }

    fn fetch_from_dir(dir: &Path) -> Result<Self, ActivityInsightsError> {
        let path = dir.join(CRED_FILE_NAME);
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
        self.location.join(CRED_FILE_NAME)
    }

    fn temp_update_file_path(&self) -> PathBuf {
        self.location.join(UPDATE_FILE_NAME)
    }

    fn lock_file_path(&self) -> PathBuf {
        self.location.join(LOCK_FILE_NAME)
    }

    pub fn api_token(&self) -> &Option<Uuid> {
        &self.api_token
    }

    pub fn has_accepted_latest(&self, latest_version: u8) -> bool {
        if let Some(val) = self.latest_accepted_tos {
            val >= latest_version
        } else {
            false
        }
    }

    pub fn has_exclusive_lock(&self) -> bool {
        self.lock_file.is_some()
    }

    /// Take out an exclusive lock on the credentials lock file. Use this before updating
    /// credentials on disk. Call realase_lock with the file passed back to release the lock
    pub fn get_exclusive_lock(&mut self) -> Result<&mut Self, ActivityInsightsError> {
        if !self.has_exclusive_lock() {
            let lock_file = self.lock_file_path();

            // We do this so it creates the file if it doesn't exist instead of erroring
            OpenOptions::new()
                .write(true)
                .create(true)
                .open(&lock_file)
                .map_err(|e| ActivityInsightsError::IO(lock_file.clone(), e))?;

            let file = File::open(&lock_file)
                .map_err(|e| ActivityInsightsError::IO(lock_file.clone(), e))?;
            file.try_lock_exclusive()
                .map_err(|e| ActivityInsightsError::IO(lock_file.clone(), e))?;

            self.lock_file = Some(file);
        }

        Ok(self)
    }

    pub fn release_exclusive_lock(&mut self) -> Result<(), ActivityInsightsError> {
        if let Some(ref lock_file) = self.lock_file {
            lock_file
                .unlock()
                .map_err(|e| ActivityInsightsError::IO(PathBuf::from("Lock file"), e))?;
            self.lock_file = None;
        }

        Ok(())
    }

    /// Update will only work if the Credentials struct has acquired an exlucsive lock on the file.
    /// To acquire a lock, use the get_exclusive_lock method. The api_token should be updated
    /// through the create_api_token method to ensure that an api token has not already been set.
    fn update(&self) -> Result<(), ActivityInsightsError> {
        if !self.has_exclusive_lock() {
            return Err(CredentialsError::NeedsExclusiveLock.into());
        };

        let temp_update_path = self.temp_update_file_path();

        fs::write(
            &temp_update_path,
            serde_yaml::to_vec(self).map_err(CredentialsError::from)?,
        )
        .map_err(|e| ActivityInsightsError::IO(temp_update_path.clone(), e))?;
        fs::rename(&temp_update_path, &self.creds_file_path())
            .map_err(|e| ActivityInsightsError::IO(temp_update_path.clone(), e))?;

        Ok(())
    }

    /// create_api_token only adds an api token if one is not already there. This prevents the user
    /// from overriding and api token that they have already successfully registered with. If an api
    /// token is already in the file but the user is not registered, try registering with the api
    /// token that is in the file.
    pub fn create_api_token(&mut self) -> Result<Uuid, ActivityInsightsError> {
        if self.api_token().is_some() {
            return Err(CredentialsError::HasApiToken.into());
        }

        self.get_exclusive_lock()?;

        let fresh_creds = match self.fetch_latest() {
            Ok(creds) => creds,
            Err(e) => {
                self.release_exclusive_lock()?;
                return Err(e);
            }
        };

        if fresh_creds.api_token().is_some() {
            self.release_exclusive_lock()?;
            return Err(CredentialsError::HasApiToken.into());
        }

        self.latest_accepted_tos = fresh_creds.latest_accepted_tos;
        let new_token = Uuid::new_v4();
        self.api_token = Some(new_token);

        if let Err(e) = self.update() {
            self.release_exclusive_lock()?;
            return Err(e);
        }

        self.release_exclusive_lock()?;

        Ok(new_token)
    }

    pub fn accept_tos(&mut self, tos_version: u8) -> Result<(), ActivityInsightsError> {
        self.get_exclusive_lock()?;

        let fresh_creds = match self.fetch_latest() {
            Ok(creds) => creds,
            Err(e) => {
                self.release_exclusive_lock()?;
                return Err(e);
            }
        };

        self.api_token = fresh_creds.api_token;
        self.latest_accepted_tos = Some(tos_version);

        if let Err(e) = self.update() {
            self.release_exclusive_lock()?;
            return Err(e);
        }

        self.release_exclusive_lock()?;
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

        creds_with_lock.get_exclusive_lock().unwrap();
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
        assert!(api_creds.lock_file.is_none());

        let mut creds = Credentials::fetch_from_dir(fake_dir.path()).unwrap();
        creds.get_exclusive_lock().unwrap();
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
}
