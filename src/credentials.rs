use fs2::FileExt;
use log::warn;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    path::PathBuf,
};
use thiserror::Error;
use uuid::Uuid;

use crate::{ActivityInsightsError, PS_DIR};

#[cfg(not(test))]
const CRED_FILE_NAME: &str = "credentials.yaml";
#[cfg(not(test))]
const UPDATE_FILE_NAME: &str = ".updated.credentials.yaml";
#[cfg(not(test))]
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
}

impl Credentials {
    pub fn fetch() -> Result<Self, ActivityInsightsError> {
        let creds_file = dirs::home_dir()
            .map(|dir| dir.join(PS_DIR).join(CRED_FILE_NAME))
            .ok_or_else(|| {
                ActivityInsightsError::Other(String::from("Can't find the home directory"))
            })?;

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&creds_file)
            .map_err(|e| ActivityInsightsError::IO(creds_file, e))?;
        let creds: Credentials = match serde_yaml::from_reader(file) {
            Ok(creds) => creds,
            Err(e) => {
                warn!("Error deserializing yaml: {}", e);
                Default::default()
            }
        };
        Ok(creds)
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
            let lock_file = dirs::home_dir()
                .map(|dir| dir.join(PS_DIR).join(LOCK_FILE_NAME))
                .ok_or_else(|| {
                    ActivityInsightsError::Other(String::from("Error getting the home directory"))
                })?;

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

        let home_dir = dirs::home_dir().ok_or_else(|| {
            ActivityInsightsError::Other(String::from("Error finding the home directory"))
        })?;
        let updated_creds_file = home_dir.join(PS_DIR).join(UPDATE_FILE_NAME);
        let actual_creds_file = home_dir.join(PS_DIR).join(CRED_FILE_NAME);

        fs::write(
            &updated_creds_file,
            serde_yaml::to_vec(self).map_err(CredentialsError::from)?,
        )
        .map_err(|e| ActivityInsightsError::IO(updated_creds_file.clone(), e))?;
        fs::rename(&updated_creds_file, &actual_creds_file)
            .map_err(|e| ActivityInsightsError::IO(updated_creds_file.clone(), e))?;

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

        let fresh_creds = Credentials::fetch()?;
        if fresh_creds.api_token().is_some() {
            return Err(CredentialsError::HasApiToken.into());
        }

        self.latest_accepted_tos = fresh_creds.latest_accepted_tos;
        let new_token = Uuid::new_v4();
        self.api_token = Some(new_token);

        self.update()?;

        self.release_exclusive_lock()?;

        Ok(new_token)
    }

    pub fn accept_tos(&mut self, tos_version: u8) -> Result<(), ActivityInsightsError> {
        self.get_exclusive_lock()?;

        let fresh_creds = Credentials::fetch()?;
        self.api_token = fresh_creds.api_token;
        self.latest_accepted_tos = Some(tos_version);

        self.update()?;

        self.release_exclusive_lock()?;
        Ok(())
    }
}

#[cfg(test)]
const CRED_FILE_NAME: &str = "test-creds.yaml";
#[cfg(test)]
const UPDATE_FILE_NAME: &str = ".updated.test-creds.yaml";
#[cfg(test)]
const LOCK_FILE_NAME: &str = "test-creds.yaml.lock";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_api_token() {
        let home = dirs::home_dir().expect("Couldn't get home dir in test");
        let fake_creds_file = home.join(PS_DIR).join(CRED_FILE_NAME);
        #[allow(unused_must_use)]
        {
            // removing here in case it didn't get removed from the last test
            fs::remove_file(&fake_creds_file);
        }

        let mut creds = Credentials::fetch().unwrap();
        let api_token = creds.create_api_token().unwrap();

        let updated_creds = Credentials::fetch().unwrap();

        fs::remove_file(&fake_creds_file).unwrap();
        assert_eq!(Some(api_token), updated_creds.api_token);
    }
}
