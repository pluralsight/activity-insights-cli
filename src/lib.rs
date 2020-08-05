use log::warn;
use reqwest::{
    blocking::{self, Client},
    StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json;
use std::{
    convert::TryFrom,
    fs::{self, File},
    io::{self, BufWriter, Write},
    path::Path,
    process::{Child, Command},
};
use thiserror::Error;
use uuid::Uuid;

mod credentials;
mod pulses;

use credentials::{Credentials, CredentialsError};
use pulses::{Pulse, PulseFromEditor};

#[cfg(target_os = "linux")]
const BINARY_DISTRIBUTION: &'static str =
    "https://ps-cdn.s3-us-west-2.amazonaws.com/learner-workflow/ps-time/linux/ps-time";
#[cfg(target_os = "macos")]
const BINARY_DISTRIBUTION: &'static str =
    "https://ps-cdn.s3-us-west-2.amazonaws.com/learner-workflow/ps-time/mac/ps-time";
#[cfg(target_os = "windows")]
const BINARY_DISTRIBUTION: &'static str =
    "https://ps-cdn.s3-us-west-2.amazonaws.com/learner-workflow/ps-time/windows/ps-time.exe";

const CLI_VERSION_URL: &'static str = "https://app.pluralsight.com/wsd/api/ps-time/version";

#[cfg(unix)]
const EXECUTABLE: &'static str = "activity-insights";
#[cfg(not(unix))]
const EXECUTABLE: &'static str = "activity-insights.exe";

#[allow(dead_code)]
const PULSE_API_URL: &'static str = "https://app.pluralsight.com/wsd/api/ps-time/pulse";
const REGISTRATION_URL: &'static str = "https://app.pluralsight.com/id?redirectTo=https://app.pluralsight.com/wsd/api/ps-time/register";
const UPDATED_EXECUTABLE: &'static str = ".updated-activity-insights";
const VERSION: usize = 1;
pub const PS_DIR: &'static str = ".pluralsight";

#[derive(Debug, Error)]
pub enum RequestError {
    #[error("{0}")]
    HTTP(#[from] reqwest::Error),

    #[error("{0}")]
    CredentialsError(#[from] CredentialsError),

    #[error("No api token was found in the credentials file. It is required to make a request.")]
    ApiTokenError,
}

#[derive(Debug, Error)]
pub enum RegistrationError {
    #[error("{0}")]
    CredentialsError(#[from] CredentialsError),

    #[error("{0}")]
    IoError(#[from] io::Error),
}

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("Error getting the home directory")]
    NoHomeDir,

    #[error("{0}")]
    RequestError(#[from] reqwest::Error),

    #[error("{0}")]
    IOError(#[from] io::Error),

    #[error("{0}")]
    DeserializationError(#[from] serde_json::Error),
}

#[derive(Debug, Serialize)]
struct PulseRequest<'a> {
    pulses: &'a Vec<Pulse>,
}

impl<'a> PulseRequest<'a> {
    #[allow(dead_code)]
    fn new(pulses: &'a Vec<Pulse>) -> Self {
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
pub fn send_pulses(pulses: &Vec<Pulse>) -> Result<StatusCode, RequestError> {
    let client = Client::new();
    let creds = Credentials::fetch()?;
    match creds.api_token() {
        Some(token) => {
            let res = client
                .post(PULSE_API_URL)
                .bearer_auth(token)
                .json(&PulseRequest::new(pulses))
                .send()?;
            Ok(res.status())
        }
        None => Err(RequestError::ApiTokenError),
    }
}

// Don't actually send the pulses for the tests
#[cfg(test)]
pub fn send_pulses(_pulses: &Vec<Pulse>) -> Result<StatusCode, RequestError> {
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

pub fn register() -> Result<(), RegistrationError> {
    let mut creds = Credentials::fetch()?;
    let api_token = match creds.api_token() {
        Some(api_token) => *api_token,
        None => creds.new_api_token(),
    };

    creds.update_api_token()?;
    open_browser(&format!("{}?apiToken={}", REGISTRATION_URL, api_token))?;
    Ok(())
}

pub fn check_for_updates() -> Result<(), UpdateError> {
    let resp = blocking::get(CLI_VERSION_URL)?;
    let resp: VersionResponse = serde_json::from_reader(resp)?;

    if resp.version > VERSION {
        log::info!("Updating cli to version {}...", resp.version);
        update_cli()
    } else {
        Ok(())
    }
}

fn update_cli() -> Result<(), UpdateError> {
    let download = blocking::get(BINARY_DISTRIBUTION)?.bytes()?;

    let pluralsight_dir = dirs::home_dir().ok_or(UpdateError::NoHomeDir)?.join(PS_DIR);

    let new_binary = pluralsight_dir.join(format!("{}-{}", UPDATED_EXECUTABLE, Uuid::new_v4()));
    let old_binary = pluralsight_dir.join(EXECUTABLE);

    let file = create_executable_file(&new_binary)?;
    let mut writer = BufWriter::new(file);
    writer.write(&download)?;
    drop(writer);

    fs::rename(new_binary, old_binary)?;

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
