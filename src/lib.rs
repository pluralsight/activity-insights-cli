use log::{info, warn};
use reqwest::{
    blocking::{self, Client},
    StatusCode,
};
use serde::{Deserialize, Serialize};
use std::{
    convert::TryFrom,
    fs::{self, File},
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
    process::{Child, Command},
};
use thiserror::Error;
use uuid::Uuid;

mod credentials;
mod pulses;

pub use credentials::{Credentials, CredentialsError};
use pulses::{Pulse, PulseFromEditor};

#[cfg(target_os = "linux")]
const BINARY_DISTRIBUTION: &str =
    "https://ps-cdn.s3-us-west-2.amazonaws.com/learner-workflow/ps-time/linux/ps-time";
#[cfg(target_os = "macos")]
const BINARY_DISTRIBUTION: &str =
    "https://ps-cdn.s3-us-west-2.amazonaws.com/learner-workflow/ps-time/mac/ps-time";
#[cfg(target_os = "windows")]
const BINARY_DISTRIBUTION: &str =
    "https://ps-cdn.s3-us-west-2.amazonaws.com/learner-workflow/ps-time/windows/ps-time.exe";

const CLI_VERSION_URL: &str = "https://app.pluralsight.com/wsd/api/ps-time/version";

#[cfg(unix)]
const EXECUTABLE: &str = "activity-insights";
#[cfg(not(unix))]
const EXECUTABLE: &str = "activity-insights.exe";

const PULSE_API_URL: &str = "https://app.pluralsight.com/wsd/api/ps-time/pulse";
const REGISTRATION_URL: &str = "https://app.pluralsight.com/id?redirectTo=https://app.pluralsight.com/wsd/api/ps-time/register";
const UPDATED_EXECUTABLE: &str = ".updated-activity-insights";
pub const PS_DIR: &str = ".pluralsight";
const VERSION: usize = 1;

#[derive(Debug, Error)]
pub enum ActivityInsightsError {
    #[error("HTTP Error for request to url: {0}\n{1}")]
    HTTP(String, reqwest::Error),

    #[error("IO Error for location: {0}\n{1}")]
    IO(PathBuf, io::Error),

    #[error("{0}")]
    Credentials(#[from] CredentialsError),

    #[error("{0}")]
    Deserialization(#[from] serde_json::Error),

    #[error("{0}")]
    Other(String),
}

#[derive(Debug, Serialize)]
struct PulseRequest<'a> {
    pulses: &'a [Pulse],
}

impl<'a> PulseRequest<'a> {
    fn new(pulses: &'a [Pulse]) -> Self {
        PulseRequest { pulses }
    }
}

#[derive(Debug, Copy, Clone, Deserialize, Serialize)]
struct VersionResponse {
    version: usize,
}

pub fn build_pulses(content: &str) -> Result<Vec<Pulse>, serde_json::error::Error> {
    let editor_pulses: Vec<PulseFromEditor> = serde_json::from_str(content)?;
    let pulses = editor_pulses
        .into_iter()
        .filter_map(|event| match Pulse::try_from(event) {
            Ok(p) => Some(p),
            Err(e) => {
                warn!("Couldn't convert event to a pulse: {}", e);
                None
            }
        })
        .collect();
    Ok(pulses)
}

#[cfg(not(test))]
pub fn send_pulses(pulses: &[Pulse]) -> Result<StatusCode, ActivityInsightsError> {
    let client = Client::new();
    let creds = Credentials::fetch()?;
    match creds.api_token() {
        Some(token) => {
            let res = client
                .post(PULSE_API_URL)
                .bearer_auth(token)
                .json(&PulseRequest::new(pulses))
                .send()
                .map_err(|e| ActivityInsightsError::HTTP(PULSE_API_URL.to_string(), e))?;
            Ok(res.status())
        }
        None => Err(ActivityInsightsError::Other(String::from(
            "No api token was found in the config file. Can't send the request without one.",
        ))),
    }
}

// Don't actually send the pulses for the tests
#[cfg(test)]
pub fn send_pulses(pulses: &Vec<Pulse>) -> Result<StatusCode, ActivityInsightsError> {
    // loggging out unused variables here to avoid unused warning
    log::info!(
        "{:?}, {} {:?}",
        Client::new(),
        PULSE_API_URL,
        PulseRequest::new(pulses)
    );
    Ok(StatusCode::default())
}

#[cfg(target_os = "macos")]
pub fn open_browser(url: &str) -> Result<Child, io::Error> {
    Command::new("open").args(&[url]).spawn()
}

#[cfg(target_os = "linux")]
pub fn open_browser(url: &str) -> Result<Child, io::Error> {
    Command::new("xdg-open").args(&[url]).spawn()
}

#[cfg(target_os = "windows")]
pub fn open_browser(url: &str) -> Result<Child, io::Error> {
    Command::new("cmd").args(&["/C", "start", url]).spawn()
}

pub fn register() -> Result<(), ActivityInsightsError> {
    let mut creds = Credentials::fetch()?;
    let api_token = match creds.api_token() {
        Some(api_token) => *api_token,
        None => {
            let api_token = creds.new_api_token();
            creds.update_api_token()?;
            api_token
        }
    };

    open_browser(&format!("{}?apiToken={}", REGISTRATION_URL, api_token))
        .map_err(|e| ActivityInsightsError::IO(PathBuf::from("Opening browser..."), e))?;
    Ok(())
}

pub fn maybe_update() -> Result<(), ActivityInsightsError> {
    match check_for_updates(VERSION) {
        Ok(true) => {
            let update_location =
                dirs::home_dir()
                    .map(|dir| dir.join(PS_DIR))
                    .ok_or_else(|| {
                        ActivityInsightsError::Other(String::from(
                            "Error getting the home directory",
                        ))
                    })?;

            update_cli(&update_location)
        }
        Ok(false) => Ok(()),
        Err(e) => Err(e),
    }
}

pub fn check_for_updates(current_version: usize) -> Result<bool, ActivityInsightsError> {
    let resp = blocking::get(CLI_VERSION_URL)
        .map_err(|e| ActivityInsightsError::HTTP(CLI_VERSION_URL.to_string(), e))?;
    let resp: VersionResponse = serde_json::from_reader(resp)?;

    if resp.version > current_version {
        info!("Updating cli to version {}...", resp.version);
        Ok(true)
    } else {
        Ok(false)
    }
}

pub fn update_cli(path: &Path) -> Result<(), ActivityInsightsError> {
    let download = blocking::get(BINARY_DISTRIBUTION)
        .and_then(|req| req.bytes())
        .map_err(|e| ActivityInsightsError::HTTP(BINARY_DISTRIBUTION.to_string(), e))?;

    let new_binary = path.join(format!("{}-{}", UPDATED_EXECUTABLE, Uuid::new_v4()));
    let old_binary = path.join(EXECUTABLE);

    let file = create_executable_file(&new_binary)
        .map_err(|e| ActivityInsightsError::IO(new_binary.clone(), e))?;
    let mut writer = BufWriter::new(file);
    writer
        .write(&download)
        .map_err(|e| ActivityInsightsError::IO(new_binary.clone(), e))?;
    drop(writer);

    fs::rename(&new_binary, &old_binary)
        .map_err(|e| ActivityInsightsError::IO(old_binary.clone(), e))?;

    Ok(())
}

#[cfg(not(unix))]
fn create_executable_file(path: &Path) -> Result<File, io::Error> {
    File::create(&path)
}

#[cfg(unix)]
fn create_executable_file(path: &Path) -> Result<File, io::Error> {
    use std::fs::OpenOptions;
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o777)
        .open(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile;

    #[test]
    fn updating() {
        let fake_dir = tempfile::tempdir().unwrap();
        update_cli(fake_dir.path()).unwrap();

        let entries: Vec<_> = fs::read_dir(fake_dir.path())
            .unwrap()
            .map(|x| x.unwrap())
            .collect();
        assert_eq!(1, entries.len());

        let new_binary = entries[0].path();
        let filename = new_binary.file_name().unwrap().to_str().unwrap();

        #[cfg(unix)]
        assert_eq!(filename, String::from("activity-insights"));

        #[cfg(not(unix))]
        assert_eq!(filename, String::from("activity-insights"));
    }

    #[test]
    fn check_update_required() {
        let very_old_version = 0;
        let should_update = check_for_updates(very_old_version).unwrap();
        assert_eq!(should_update, true)
    }

    #[test]
    fn check_update_not_required() {
        let very_new_version = 10_000_000;
        let should_update = check_for_updates(very_new_version).unwrap();
        assert_eq!(should_update, false)
    }
}
