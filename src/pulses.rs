// Include a phf set of common package names to match against
// static PACKAGES: phf::Set<&'static str> = ...;
include!("./codegen/packages-set.rs");

use chrono::{TimeZone, Utc};
use polyglot_tokenizer::{Token, Tokenizer};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::{convert::TryFrom, path::PathBuf};
use thiserror::Error;

/// event_date is milliseconds seconds since the Unix epoch
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PulseFromEditor {
    file_path: PathBuf,
    event_type: String,
    event_date: i64,
    editor: String,
}

/// date is a string representing a date formatted according to: https://tools.ietf.org/html/rfc3339
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Pulse {
    #[serde(rename(serialize = "type"))]
    pulse_type: String,
    date: String,
    #[serde(rename(serialize = "programmingLanguage"))]
    programming_language: String,
    editor: String,
    tags: HashSet<&'static str>,
}

#[derive(Debug, Error)]
pub enum ConversionError {
    #[error("File: {0} threw io error: {1}")]
    IOError(PathBuf, std::io::Error),
}

/// TryFrom will fail in the event of an io error but not if the programming language can't be
/// detected. If the programming language can't be detected, then "Other" will be the  value of
/// programming language
impl TryFrom<PulseFromEditor> for Pulse {
    type Error = ConversionError;

    fn try_from(editor_pulse: PulseFromEditor) -> Result<Self, Self::Error> {
        let (seconds, nanosecs) = breakdown_milliseconds(editor_pulse.event_date);
        let timestamp = Utc.timestamp(seconds, nanosecs);

        let language = hyperpolyglot::detect(&editor_pulse.file_path)
            .map_err(|e| ConversionError::IOError(editor_pulse.clone().file_path, e))?
            .map(|detection| detection.language())
            .unwrap_or("Other");

        let content = fs::read_to_string(&editor_pulse.file_path).unwrap_or_default();
        let tags: HashSet<&'static str> = Tokenizer::new(&content)
            .tokens()
            .filter_map(|token| match token {
                Token::String(_, value, _) | Token::Ident(value) => {
                    PACKAGES.get_key(value).copied()
                }
                _ => None,
            })
            .collect();

        Ok(Pulse {
            pulse_type: editor_pulse.event_type,
            date: timestamp.to_rfc3339(),
            editor: editor_pulse.editor,
            programming_language: String::from(language),
            tags,
        })
    }
}

/// Takes a unix timestamp in ms and breaks it down into the number of seconds and nano seconds.
/// This is the way chrono expects the time when generating a Utc timestamp and it comes out of the
/// editors in ms
fn breakdown_milliseconds(ms: i64) -> (i64, u32) {
    let seconds = ms.div_euclid(1000);
    let nanoseconds = (ms % 1000) * 1_000_000;
    (seconds, nanoseconds as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn from_payload_to_pulse() {
        let mut fake_file = NamedTempFile::new().unwrap();
        let fake_content = r#"
            use reqwest;
            const blah = require("express");
        "#;
        write!(fake_file, "{}", fake_content).unwrap();

        let raw_pulse = r#"{"filePath": "{filepath}","eventType": "typing","eventDate": 1595868513238,"editor": "emacs :rip:"}"#;
        println!("PRE-FILLED PULSE: '{}'", raw_pulse);
        let raw_pulse = raw_pulse.replace("{filepath}", fake_file.path().to_str().unwrap());

        #[cfg(windows)]
        let raw_pulse = raw_pulse.replace(r#"\"#, r#"\\"#); // Otherwise windows backslashes in file paths are interpreted as escape sequences

        println!("PULSE: '{}'", raw_pulse);

        let editor_pulse: PulseFromEditor =
            serde_json::from_str(&raw_pulse).expect("Failed deserializing editor pulse");
        let pulse = Pulse::try_from(editor_pulse).expect("Error converting to pulse");
        let tags: HashSet<&'static str> = vec!["express", "reqwest"].into_iter().collect();

        let expected = Pulse {
            pulse_type: String::from("typing"),
            date: String::from("2020-07-27T16:48:33.238+00:00"),
            programming_language: String::from("Other"),
            editor: String::from("emacs :rip:"),
            tags,
        };
        assert_eq!(pulse, expected);
    }

    #[test]
    fn breakdown_milliseconds_smoke_test() {
        assert_eq!(breakdown_milliseconds(10_500), (10, 500_000_000))
    }
}
