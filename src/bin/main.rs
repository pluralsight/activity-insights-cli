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
    sync::mpsc,
    thread,
    time::Duration,
};

use activity_insights_cli::{
    build_pulses, constants, maybe_update, open_browser, register, send_pulses, Credentials,
};

fn main() {
    create_logger();
    info!("Starting cli...");

    match env::args().nth(1) {
        Some(v) if v.as_str() == "accept_tos" => accept_tos_command(),
        Some(v) if v.as_str() == "version" => println!("{}", constants::VERSION),
        _ => {
            check_tos();
            match env::args().nth(1) {
                Some(v) if v.as_str() == "register" => register_command(),
                Some(v) if v.as_str() == "dashboard" => dashboard_command(),
                _ => pulse_command(),
            }
        }
    };

    if let Err(e) = maybe_update() {
        error!("Error updating: {}", e)
    }
}

/*
 * Create_logger will exit if it can't create the logger
 * If the process exits while creating the logger, the exit code will be in the range 10-19
 */
fn create_logger() {
    let mut log_dir = dirs::home_dir().unwrap_or_else(|| {
        eprintln!("Error finding home dir");
        exit(10);
    });
    log_dir.push(constants::PS_DIR);
    log_dir.push(constants::LOG_FILE);

    let rotation_policy = CompoundPolicy::new(
        Box::new(SizeTrigger::new(10_000)),
        Box::new(DeleteRoller::new()),
    );

    let logger = RollingFileAppender::builder()
        .build(log_dir, Box::new(rotation_policy))
        .unwrap_or_else(|e| {
            eprintln!("Can't create the log file: {}", e);
            exit(11);
        });

    let config = Config::builder()
        .appender(Appender::builder().build("logger", Box::new(logger)))
        .build(Root::builder().appender("logger").build(LevelFilter::Info))
        .unwrap_or_else(|e| {
            eprintln!("Can't create the logger config: {}", e);
            exit(12);
        });

    log4rs::init_config(config).unwrap_or_else(|e| {
        eprintln!("Failed to initialize logger: {}", e);
        exit(13);
    });
}

fn check_tos() {
    let creds = Credentials::fetch().unwrap_or_else(|e| {
        error!("Unable to get creds file: {}", e);
        exit(101)
    });

    if !creds.has_accepted_latest(constants::TOS_VERSION) {
        println!("{}", constants::TOS);
        exit(constants::NOT_ACCEPTED_TOS_EXIT_CODE)
    }
}

fn register_command() {
    info!("Starting register command");
    if let Err(e) = register() {
        error!("Error on registration: {}", e);
        if let Err(e) = open_browser(constants::BAD_REGISTRATION_URL) {
            error!(
                "Error trying to let the user know a registration went bad: {}",
                e
            );
            exit(31);
        }
    }
}

fn dashboard_command() {
    info!("Starting dashboard command");
    if let Err(e) = open_browser(constants::DASHBOARD_URL) {
        error!("Error trying to show the user their dashboard: {}", e);
        exit(40);
    } else {
        info!("Dashboard successfully opened");
    }
}

fn pulse_command() {
    info!("Starting pulse command");

    let input = match read_from_stdin_with_timeout(Duration::from_millis(10_000)) {
        Ok(input) => input,
        Err(e) => {
            error!("Timedout reading from stdin: {}", e);
            exit(21);
        }
    };

    let pulses = build_pulses(&input).unwrap_or_else(|e| {
        error!("Error building pulses from content: {}\n{}", input, e);
        exit(22);
    });

    match send_pulses(&pulses) {
        Ok(StatusCode::NO_CONTENT) => {
            info!("Pulses successfully sent");
        }
        Ok(code) => info!("Unexpected status code for pulses: {:?}\n{}", pulses, code),
        Err(e) => {
            error!("Error sending pulses:{:?}\n{}", pulses, e);
            exit(23);
        }
    }
}

fn accept_tos_command() {
    let mut creds = Credentials::fetch().unwrap_or_else(|e| {
        error!("Unable to get creds file: {}", e);
        exit(101)
    });

    creds
        .accept_tos(constants::TOS_VERSION)
        .unwrap_or_else(|e| {
            error!("Error accepting TOS {}: {}", constants::TOS_VERSION, e);
            exit(102)
        });
}

/*
 * Read from stdin with timeout so the process doesn't hang forever. This could happen if an editor
 * starts the process but forgets to pipe stdin
 */
fn read_from_stdin_with_timeout(duration: Duration) -> Result<String, mpsc::RecvTimeoutError> {
    let (send, recv) = mpsc::channel();

    thread::spawn(move || {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer).unwrap_or_else(|e| {
            error!("Error reading from stdin: {}", e);
            exit(20);
        });

        if let Err(e) = send.send(buffer) {
            error!("Error sending value across the channel: {}", e)
        }
    });

    recv.recv_timeout(duration)
}

fn exit(code: i32) -> ! {
    error!("Exiting with code: {}", code);
    process::exit(code);
}
