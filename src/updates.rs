//! Snapshot of the macOS updater for the Settings and Updates pages.

use serde::Deserialize;

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Status {
    #[default]
    Unavailable,
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

#[derive(Default, Deserialize)]
pub(crate) struct Snapshot {
    pub status: Status,
    pub can_check: bool,
    pub automatic: bool,
    pub version: String,
    pub notes: String,
    pub bytes: u64,
}

impl Snapshot {
    pub fn enabled(&self) -> bool {
        !matches!(self.status, Status::Unavailable)
    }

    pub fn label(&self) -> &'static str {
        crate::l10n::text(match self.status {
            Status::Unavailable => "updates-unavailable",
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
