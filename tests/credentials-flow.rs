use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::{env, fs::File, process::Command};

use activity_insights_cli::{constants, Credentials};

const TOS: &str = include_str!("../terms-of-service");
const TOS_VERSION: usize = include!("../terms-of-service-version");

#[test]
fn credentials_flow() {
    let fake_home_dir = tempfile::tempdir().unwrap();
    env::set_var("HOME", fake_home_dir.path());

    // Get denied by TOS
    let mut cmd = Command::cargo_bin("activity-insights").unwrap();
    cmd.arg("dashboard");

    cmd.assert()
        .failure()
        .code(100)
        .stdout(predicate::str::starts_with(TOS));

    // Accept TOS
    let mut cmd = Command::cargo_bin("activity-insights").unwrap();
    cmd.arg("accept_tos");
    cmd.assert().success();

    let creds_path = fake_home_dir
        .path()
        .join(constants::PS_DIR)
        .join(constants::CRED_FILE_NAME);

    let file = File::open(creds_path).unwrap();
    let creds: Credentials = serde_yaml::from_reader(file).unwrap();
    assert!(creds.has_accepted_latest(TOS_VERSION))
}
