pub const BAD_REGISTRATION_URL: &str =  "https://app.pluralsight.com/id?redirectTo=https://app.pluralsight.com/activity-insights-beta?error=unsuccessful-registration";
pub const CRED_FILE_NAME: &str = "credentials.yaml";
pub const LOCK_FILE_NAME: &str = "credentials.yaml.lock";
pub const CLI_VERSION_URL: &str = "https://app.pluralsight.com/wsd/api/ps-time/version";
pub const DASHBOARD_URL: &str = "https://app.pluralsight.com/activity-insights-beta/";
pub const LOG_FILE: &str = "activity-insights.logs";
pub const NOT_ACCEPTED_TOS_EXIT_CODE: i32 = 100;
pub const PS_DIR: &str = ".pluralsight";
pub const PULSE_API_URL: &str = "https://app.pluralsight.com/wsd/api/ps-time/pulse";
pub const REGISTRATION_URL: &str = "https://app.pluralsight.com/id?redirectTo=https://app.pluralsight.com/wsd/api/ps-time/register";
pub const TOS: &str = include_str!("../terms-of-service");
pub const TOS_VERSION: usize = include!("../terms-of-service-version");
pub const VERSION: usize = include!("../cli-version");

#[cfg(target_os = "linux")]
pub const BINARY_DISTRIBUTION: &str =
    "https://ps-cdn.s3-us-west-2.amazonaws.com/learner-workflow/ps-time/linux/activity-insights";
#[cfg(target_os = "macos")]
pub const BINARY_DISTRIBUTION: &str =
    "https://ps-cdn.s3-us-west-2.amazonaws.com/learner-workflow/ps-time/mac/activity-insights";
#[cfg(target_os = "windows")]
pub const BINARY_DISTRIBUTION: &str =
    "https://ps-cdn.s3-us-west-2.amazonaws.com/learner-workflow/ps-time/windows/activity-insights.exe";

#[cfg(unix)]
pub const EXECUTABLE: &str = "activity-insights";
#[cfg(not(unix))]
pub const EXECUTABLE: &str = "activity-insights.exe";
