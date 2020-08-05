use dirs;
use log::{error, info, LevelFilter};
use log4rs::{
    append::rolling_file::{
        policy::compound::{
            roll::delete::DeleteRoller, trigger::size::SizeTrigger, CompoundPolicy,
        },
        RollingFileAppender,
    },
    config::{Appender, Config, Root},
};
use reqwest::StatusCode;
use std::{
    env,
    io::{self, Read},
    process,
};

use activity_insights_cli::{
    build_pulses, check_for_updates, open_browser, register, send_pulses, PS_DIR,
};

const BAD_REGISTRATION_URL: &'static str =  "https://app.pluralsight.com/id?redirectTo=https://app.pluralsight.com/activity-insights-beta?error=unsuccessful-registration";
const DASHBOARD_URL: &'static str = "https://app.pluralsight.com/activity-insights-beta/";
const LOG_FILE: &'static str = "ps-activity-insights.logs";

fn main() {
    create_logger();
    info!("Starting cli...");

    match env::args().skip(1).next() {
        Some(v) if v.as_str() == "register" => register_command(),
        Some(v) if v.as_str() == "dashboard" => dashboard_command(),
        _ => pulse_command(),
    };

    if let Err(e) = check_for_updates() {
        error!("Error updating cli: {}", e);
    }
}

/*
 * Create_logger will exit if it can't create the logger
 * If the process exits while creating the logger, the exit code will be in the range 10-19
 */
fn create_logger() {
    let mut log_dir = dirs::home_dir().unwrap_or_else(|| {
        eprintln!("Error finding home dir");
        process::exit(10);
    });
    log_dir.push(PS_DIR);
    log_dir.push(LOG_FILE);

    let rotation_policy = CompoundPolicy::new(
        Box::new(SizeTrigger::new(10_000)),
        Box::new(DeleteRoller::new()),
    );

    let logger = RollingFileAppender::builder()
        .build(log_dir, Box::new(rotation_policy))
        .unwrap_or_else(|e| {
            eprintln!("Can't create the log file: {}", e);
            process::exit(11);
        });

    let config = Config::builder()
        .appender(Appender::builder().build("logger", Box::new(logger)))
        .build(Root::builder().appender("logger").build(LevelFilter::Info))
        .unwrap_or_else(|e| {
            eprintln!("Can't create the logger config: {}", e);
            process::exit(12);
        });

    log4rs::init_config(config).unwrap_or_else(|e| {
        eprintln!("Failed to initialize logger: {}", e);
        process::exit(13);
    });
}

fn register_command() {
    if let Err(e) = register() {
        error!("Error on registration: {}", e);
        if let Err(e) = open_browser(BAD_REGISTRATION_URL) {
            error!(
                "Error trying to let the user know a registration went bad: {}",
                e
            );
        }
    }
}

fn dashboard_command() {
    if let Err(e) = open_browser(DASHBOARD_URL) {
        error!("Error trying to show the user their dashboard: {}", e);
    }
}

fn pulse_command() {
    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer).unwrap_or_else(|e| {
        error!("Error reading from stdin: {}", e);
        process::exit(1);
    });

    let pulses = build_pulses(&buffer).unwrap_or_else(|e| {
        error!("Error building pulses from content: {}\n{}", buffer, e);
        process::exit(2);
    });

    match send_pulses(&pulses) {
        Ok(StatusCode::NO_CONTENT) => {
            info!("Pulses successfully sent");
        }
        Ok(code) => info!("Unexpected status code for pulses: {:?}\n{}", pulses, code),
        Err(e) => {
            error!("Error sending pulses:{:?}\n{}", pulses, e);
        }
    }
}
