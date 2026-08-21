//! Reading and writing Zervo's own small JSON files.
//!
//! Four things keep state on disk — the settings, the library, the password
//! index and the wallpaper manifest — and all four did the same thing:
//! serialise, then `fs::write` straight over the destination.
//!
//! `fs::write` truncates the file before it writes, so an interruption anywhere
//! in the middle leaves half of one behind. The next launch reads that, fails to
//! parse it, and falls back to the defaults — silently, because three of the
//! four discarded the error too. Losing a settings file that way is annoying;
//! losing the history or the password index is worse.
//!
//! So a save is written to a sibling temporary file and renamed over the
//! destination. `rename` is atomic, so a reader sees either the old file or the
//! new one and never a partial one. It is the same `.part`-then-rename that
//! `downloads.rs` already does for downloaded files; this is that idiom given a
//! name so the rest of the tree can share it.
//!
//! Files are created readable only by their owner. None of this is a secret in
//! the keychain sense — the passwords themselves are not here (see
//! `passwords.rs`) — but browsing history is nobody else's business, and a mode
//! set at creation has no window where the file is briefly world-readable.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Read `path` as JSON, falling back to the default.
///
/// A missing file is the ordinary first-run case and says nothing. A file that
/// is there but will not parse is worth a line in the log, because it means
/// somebody is about to lose their preferences and would otherwise have no idea
/// why.
pub fn load_or_default<T: DeserializeOwned + Default>(path: Option<PathBuf>) -> T {
    let Some(path) = path else {
        return T::default();
    };
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return T::default();
    };
    match serde_json::from_str(&contents) {
        Ok(value) => value,
        Err(error) => {
            log::warn!(
                "{} did not parse ({error}); starting from the defaults",
                path.display()
            );
            T::default()
        },
    }
}

/// Serialise `value` over `path`, atomically. Returns whether it landed.
///
/// The return value matters to callers holding a dirty flag: clearing one on a
/// write that did not happen is how a day of history goes missing.
#[must_use]
pub fn save<T: Serialize>(path: Option<PathBuf>, value: &T) -> bool {
    let Some(path) = path else {
        return false;
    };
    let json = match serde_json::to_vec_pretty(value) {
        Ok(json) => json,
        Err(error) => {
            log::warn!("could not serialise {}: {error}", path.display());
            return false;
        },
    };
    match write_private(&path, &json) {
        Ok(()) => true,
        Err(error) => {
            log::warn!("could not write {}: {error}", path.display());
            false
        },
    }
}

/// Write `bytes` over `path` atomically, owner-readable only.
///
/// Public because the password export wants the same guarantees but its own
/// error message: it is the one write here a person asked for by name, so a
/// failure belongs on screen rather than in the log.
pub fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }

    // A sibling of the destination, so the rename below never crosses a
    // filesystem — `rename` fails across mounts, and a temporary directory is
    // not always on the same one. The process id keeps two Zervos apart.
    let temporary = path.with_extension(format!("tmp{}", std::process::id()));

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        // Created private rather than created and then tightened: there is no
        // moment in between for anything else to open it.
        options.mode(0o600);
    }

    let written = (|| -> std::io::Result<()> {
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        // Flushed before the rename. Without this the rename can land while the
        // contents are still only in the page cache, and a crash in that window
        // leaves an empty file where the old good one used to be — the exact
        // failure the rename is here to prevent.
        file.sync_all()
    })();

    let outcome = written.and_then(|()| std::fs::rename(&temporary, path));
    if outcome.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    outcome
}
