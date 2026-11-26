//! Snapshot of the macOS updater for the Settings and Updates pages.

use serde::Deserialize;

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Status {
    #[default]
    Unavailable,
    InstallRequired,
    Idle,
    Checking,
    Current,
    Available,
    Downloading,
    Staging,
    Ready,
    Installing,
    Manual,
    NoCompatible,
    Failed,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Preference {
    AutomaticChecks(bool),
    AutomaticInstall(bool),
    CheckInterval(u32),
    SystemNotifications(bool),
    RemindTomorrow,
}

pub(crate) const CHECK_INTERVALS: [(u32, &str); 4] = [
    (3_600, "updates-hourly"),
    (21_600, "updates-six-hours"),
    (86_400, "updates-daily"),
    (604_800, "updates-weekly"),
];

pub(crate) fn check_interval_label(seconds: u32) -> &'static str {
    CHECK_INTERVALS
        .iter()
        .find(|(interval, _)| *interval == seconds)
        .map_or("updates-custom-interval", |(_, label)| label)
}

#[derive(Default, Deserialize)]
pub(crate) struct Snapshot {
    pub status: Status,
    pub can_check: bool,
    pub automatic_checks: bool,
    pub automatic_install: bool,
    pub check_interval: u32,
    pub system_notifications: bool,
    pub notifications_blocked: bool,
    pub reminder_scheduled: bool,
    pub notice_version: String,
    pub open_updates: bool,
    pub version: String,
    pub notes: String,
    pub bytes: u64,
}

impl Snapshot {
    pub fn enabled(&self) -> bool {
        !matches!(self.status, Status::Unavailable | Status::InstallRequired)
    }

    pub fn label(&self) -> &'static str {
        crate::l10n::text(match self.status {
            Status::Unavailable => "updates-unavailable",
            Status::InstallRequired => "updates-install-required",
            Status::Idle => "updates-idle",
            Status::Checking => "updates-checking",
            Status::Current => "updates-current",
            Status::Available => "updates-available",
            Status::Downloading => "updates-downloading",
            Status::Staging => "updates-staging",
            Status::Ready => "updates-ready",
            Status::Installing => "updates-installing",
            Status::Manual => "updates-manual",
            Status::NoCompatible => "updates-no-compatible",
            Status::Failed => "updates-failed",
        })
    }
}

pub(crate) fn installed_version() -> String {
    env!("CARGO_PKG_VERSION").replace('+', ".")
}

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
pub(crate) use macos::Updater;
