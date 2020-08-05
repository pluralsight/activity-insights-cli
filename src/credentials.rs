use dirs;
use fs2::FileExt;
use log::warn;
use serde::{Deserialize, Serialize};
use serde_yaml;
use std::{
    fs::{self, File, OpenOptions},
    path::PathBuf,
};
use thiserror::Error;
use uuid::Uuid;

use crate::{ActivityInsightsError, PS_DIR};

const CRED_FILE_NAME: &'static str = "credentials.yaml";
const UPDATE_FILE_NAME: &'static str = ".updated.credentials.yaml";
const LOCK_FILE_NAME: &'static str = "credentials.yaml.lock";

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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Credentials {
    api_token: Option<Uuid>,
    #[serde(skip)]
    has_exclusive_lock: bool,
}

impl Credentials {
    pub fn fetch() -> Result<Self, ActivityInsightsError> {
        let creds_file = dirs::home_dir()
            .map(|dir| dir.join(PS_DIR).join(CRED_FILE_NAME))
            .ok_or(ActivityInsightsError::Other(String::from(
                "Can't find the home directory",
            )))?;

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
                Credentials {
                    api_token: None,
                    has_exclusive_lock: false,
                }
            }
        };
        Ok(creds)
    }

    pub fn api_token(&self) -> &Option<Uuid> {
        &self.api_token
    }

    pub fn has_exclusive_lock(&self) -> bool {
        self.has_exclusive_lock
    }

    /*
     * Take out an exclusive lock on the credentials lock file. Use this before updating
     * credentials on disk. Call realase_lock with the file passed back to release the lock
     */
    pub fn get_exclusive_lock(&mut self) -> Result<File, ActivityInsightsError> {
        if !self.has_exclusive_lock() {
            let lock_file = dirs::home_dir()
                .map(|dir| dir.join(PS_DIR).join(LOCK_FILE_NAME))
                .ok_or(ActivityInsightsError::Other(String::from(
                    "Error getting the home directory",
                )))?;

            // first create the file if it doesn't exist
            OpenOptions::new()
                .write(true)
                .create(true)
                .open(&lock_file)
                .map_err(|e| ActivityInsightsError::IO(lock_file.clone(), e))?;

            let file = File::open(&lock_file)
                .map_err(|e| ActivityInsightsError::IO(lock_file.clone(), e))?;
            file.try_lock_exclusive()
                .map_err(|e| ActivityInsightsError::IO(lock_file.clone(), e))?;

            self.has_exclusive_lock = true;
            Ok(file)
        } else {
            Err(CredentialsError::HasExclusiveLock)?
        }
    }

    pub fn release_exclusive_lock(&mut self, lock: File) -> Result<(), ActivityInsightsError> {
        lock.unlock()
            .map_err(|e| ActivityInsightsError::IO(PathBuf::from("Lock file"), e))?;
        self.has_exclusive_lock = false;
        Ok(())
    }

    /*
     * new_api_token will create a new api token and add it to the struct but it will not udpate
     * the credentials file on disk. To update the credentials file, call update_api_token
     */
    pub fn new_api_token(&mut self) -> Uuid {
        let uuid = Uuid::new_v4();
        self.api_token = Some(uuid);
        uuid
    }

    /*
     * Update will only work if the Credentials struct has acquired an exlucsive lock on the file.
     * To acquire a lock, use the get_exclusive_lock method. The api_token should be updated
     * through the update_api_token method to ensure that an api token has not already been set.
     */
    fn update(&self) -> Result<(), ActivityInsightsError> {
        if !self.has_exclusive_lock() {
            return Err(CredentialsError::NeedsExclusiveLock)?;
        };

        let home_dir = dirs::home_dir().ok_or(ActivityInsightsError::Other(String::from(
            "Error finding the home directory",
        )))?;
        let updated_creds_file = home_dir.join(PS_DIR).join(UPDATE_FILE_NAME);
        let actual_creds_file = home_dir.join(PS_DIR).join(CRED_FILE_NAME);

        fs::write(
            &updated_creds_file,
            serde_yaml::to_vec(self).map_err(|e| CredentialsError::from(e))?,
        )
        .map_err(|e| ActivityInsightsError::IO(updated_creds_file.clone(), e))?;
        fs::rename(&updated_creds_file, &actual_creds_file)
            .map_err(|e| ActivityInsightsError::IO(updated_creds_file.clone(), e))?;

        Ok(())
    }

    /*
     * udpate_api_token only adds an api token if one is not already there. This prevents the user
     * from overriding and api token that they have already successfully registered with. If an api
     * token is already in the file but the user is not registered, try registering with the api
     * token that is in the file.
     */
    pub fn update_api_token(&mut self) -> Result<(), ActivityInsightsError> {
        if let None = self.api_token() {
            return Err(CredentialsError::ApiTokenRequired)?;
        }
        if self.has_exclusive_lock() {
            return Err(CredentialsError::HasExclusiveLock)?;
        }

        let lock_file = self.get_exclusive_lock()?;

        // Check to see if an api token has already been set
        let fresh_creds = Credentials::fetch()?;
        if let Some(_) = fresh_creds.api_token() {
            return Err(CredentialsError::HasApiToken)?;
        }

        self.update()?;

        self.release_exclusive_lock(lock_file)?;

        Ok(())
    }
}
