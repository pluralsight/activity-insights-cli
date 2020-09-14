use log::{info, warn};
use reqwest::{
    blocking::{self, Client},
    StatusCode,
};
use serde::{Deserialize, Serialize};
use std::{
    convert::TryFrom,
    fs,
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
    process::{Child, Command},
};
use tempfile::NamedTempFile;
use thiserror::Error;

pub mod constants;
mod credentials;
mod pulses;

pub use credentials::{Credentials, CredentialsError};
use pulses::{Pulse, PulseFromEditor};

#[derive(Debug, Error)]
pub enum ActivityInsightsError {
    #[error("HTTP Error for request to url: {0}\n{1}")]
    HTTP(String, reqwest::Error),

    #[error("Bad response from request to url: {0}\n{:1}")]
    BadResponse(String, StatusCode),

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
    if pulses.is_empty() {
        return Ok(StatusCode::from_u16(204).unwrap());
    };

    let client = Client::new();
    let creds = Credentials::fetch()?;
    match creds.api_token() {
        Some(token) => {
            let res = client
                .post(constants::PULSE_API_URL)
                .bearer_auth(token)
                .json(&PulseRequest::new(pulses))
                .send()
                .map_err(|e| {
                    ActivityInsightsError::HTTP(constants::PULSE_API_URL.to_string(), e)
                })?;
            Ok(res.status())
        }
        None => Err(ActivityInsightsError::Other(String::from(
            "No api token was found in the config file. Can't send the request without one.",
        ))),
    }
}

// Don't send the pulses for the tests
#[cfg(test)]
pub fn send_pulses(pulses: &Vec<Pulse>) -> Result<StatusCode, ActivityInsightsError> {
    // loggging out unused variables here to avoid unused warning
    log::info!(
        "{:?}, {} {:?}",
        Client::new(),
        constants::PULSE_API_URL,
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
        None => creds.create_api_token()?,
    };

    open_browser(&format!(
        "{}?apiToken={}",
        constants::REGISTRATION_URL,
        api_token
    ))
    .map_err(|e| ActivityInsightsError::IO(PathBuf::from("Opening browser..."), e))?;
    Ok(())
}

pub fn maybe_update() -> Result<(), ActivityInsightsError> {
    let latest = get_latest_version()?;
    if latest > constants::VERSION {
        let update_location = dirs::home_dir()
            .map(|dir| dir.join(constants::PS_DIR))
            .ok_or_else(|| {
                ActivityInsightsError::Other(String::from("Error getting the home directory"))
            })?;

        update_cli(&update_location, latest)?;
    }
    Ok(())
}

pub fn get_latest_version() -> Result<usize, ActivityInsightsError> {
    let resp = blocking::get(constants::CLI_VERSION_URL)
        .map_err(|e| ActivityInsightsError::HTTP(constants::CLI_VERSION_URL.to_string(), e))?;
    let resp: VersionResponse = serde_json::from_reader(resp)?;

    Ok(resp.version)
}

pub fn update_cli(path: &Path, version: usize) -> Result<(), ActivityInsightsError> {
    info!("Updating cli to version {}...", version);

    let download = {
        let download_url = get_download_url(version);

        let response = blocking::get(&download_url)
            .map_err(|e| ActivityInsightsError::HTTP(download_url.to_string(), e))?;

        match response.status() {
            StatusCode::OK => match response.bytes() {
                Ok(bytes) => bytes,
                Err(e) => return Err(ActivityInsightsError::HTTP(download_url, e)),
            },
            other => return Err(ActivityInsightsError::BadResponse(download_url, other)),
        }
    };

    let ephemeral_update_file = NamedTempFile::new_in(path)
        .map_err(|e| ActivityInsightsError::IO(PathBuf::from("temp-file"), e))?;

    let mut writer = BufWriter::new(&ephemeral_update_file);
    if let Err(e) = writer.write(&download) {
        return Err(ActivityInsightsError::IO(
            ephemeral_update_file.path().to_path_buf(),
            e,
        ));
    }

    let permanent_executable_path = path.join(constants::EXECUTABLE);
    if let Err(e) = fs::rename(&ephemeral_update_file, &permanent_executable_path) {
        return Err(ActivityInsightsError::IO(
            permanent_executable_path.clone(),
            e,
        ));
    }

    #[cfg(unix)]
    if let Err(e) = give_executable_permissions(&permanent_executable_path) {
        // If we don't remove the binary, then the next time an editor goes to run the binary it
        // will get a permissions error. Removing the binary will cause the editor to try and
        // reinstall the binary and hopefully it goes better on the next attempt.
        if let Err(e) = fs::remove_file(&permanent_executable_path) {
            return Err(ActivityInsightsError::IO(
                permanent_executable_path.to_path_buf(),
                e,
            ));
        }
        return Err(ActivityInsightsError::IO(permanent_executable_path, e));
    }

    Ok(())
}

#[cfg(unix)]
fn give_executable_permissions(path: &Path) -> Result<(), io::Error> {
    use std::os::unix::fs::PermissionsExt;

    let new_permissions = fs::Permissions::from_mode(0o777);
    fs::set_permissions(path, new_permissions).unwrap();
    Ok(())
}

#[cfg(target_os = "linux")]
fn get_download_url(version: usize) -> String {
    format!(
        "{}linux/activity-insights-{}",
        constants::BASE_BINARY_DISTRIBUTION,
        version
    )
}

#[cfg(target_os = "macos")]
fn get_download_url(version: usize) -> String {
    format!(
        "{}mac/activity-insights-{}",
        constants::BASE_BINARY_DISTRIBUTION,
        version
    )
}

#[cfg(target_os = "windows")]
fn get_download_url(version: usize) -> String {
    format!(
        "{}windows/activity-insights-{}.exe",
        constants::BASE_BINARY_DISTRIBUTION,
        version
    )
}

#[cfg(test)]
#[ctor::ctor]
fn init() {
    let _ = env_logger::builder()
        .is_test(true)
        .filter_level(log::LevelFilter::Debug)
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile;

    const FAKE_VERSION: usize = 1;

    #[test]
    fn updating() {
        let fake_dir = tempfile::tempdir().unwrap();
        update_cli(fake_dir.path(), FAKE_VERSION).unwrap();

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
        assert_eq!(filename, String::from("activity-insights.exe"));

        #[cfg(unix)]
        {
            use std::fs::Permissions;
            use std::os::unix::fs::PermissionsExt;

            let file = fs::File::open(new_binary).unwrap();
            let permissions = file.metadata().unwrap().permissions();

            // The first few bits represent data about the file, which is why its 0o100777 and not
            // 0o777
            let expected_permissions = Permissions::from_mode(0o100777);
            assert_eq!(permissions, expected_permissions);
        }
    }

    #[test]
    fn get_latest() {
        let very_old_version = 0;
        let latest = get_latest_version().unwrap();
        assert!(latest > very_old_version)
    }
}
