use log::warn;
use reqwest::{blocking::Client, StatusCode};
use serde::Serialize;
use serde_json;
use std::{
    convert::TryFrom,
    io,
    process::{Child, Command},
};
use thiserror::Error;

mod credentials;
mod pulses;

use credentials::{Credentials, CredentialsError};
use pulses::{Pulse, PulseFromEditor};

pub const PS_DIR: &'static str = ".pluralsight";

pub fn build_pulses(content: &str) -> Result<Vec<Pulse>, serde_json::error::Error> {
    let editor_pulses: Vec<PulseFromEditor> = serde_json::from_str(content)?;
    let pulses = editor_pulses
        .into_iter()
        .filter_map(|event| match Pulse::try_from(event) {
            Ok(p) => Some(p),
            Err(e) => {
                warn!("Couldn't convert event to a pulse: {:?}\n{}", content, e);
                None
            }
        })
        .collect();
    Ok(pulses)
}

#[derive(Debug, Serialize)]
struct PulseRequest<'a> {
    pulses: &'a Vec<Pulse>,
}

impl<'a> PulseRequest<'a> {
    fn new(pulses: &'a Vec<Pulse>) -> Self {
        PulseRequest { pulses }
    }
}

#[derive(Debug, Error)]
pub enum RequestError {
    #[error("{0}")]
    HTTP(#[from] reqwest::Error),

    #[error("{0}")]
    CredentialsError(#[from] CredentialsError),

    #[error("No api token was found in the credentials file. It is required to make a request.")]
    ApiTokenError,
}

const PULSE_API_URL: &'static str = "https://app.pluralsight.com/wsd/api/ps-time/pulse";

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

#[derive(Debug, Error)]
pub enum RegistrationError {
    #[error("{0}")]
    CredentialsError(#[from] CredentialsError),

    #[error("{0}")]
    IoError(#[from] io::Error),
}

const REGISTRATION_URL: &'static str = "https://app.pluralsight.com/id?redirectTo=https://app.pluralsight.com/wsd/api/ps-time/register";

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
