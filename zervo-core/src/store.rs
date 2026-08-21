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

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    /// A directory of this test's own. No `tempfile` dependency for something
    /// this small, and the counter keeps parallel tests apart.
    fn scratch(name: &str) -> PathBuf {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let unique = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "zervo-store-{}-{name}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn what_is_written_is_what_comes_back() {
        let path = scratch("round-trip").join("settings.json");
        assert!(save(Some(path.clone()), &vec![1_u32, 2, 3]));
        let read: Vec<u32> = load_or_default(Some(path.clone()));
        assert_eq!(read, vec![1, 2, 3]);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// The directory is made on the way, because the first save of a fresh
    /// install happens before anything else has been written there.
    #[test]
    fn the_directory_is_created_if_it_is_not_there() {
        let dir = scratch("mkdir");
        let path = dir.join("deeper").join("thing.json");
        assert!(save(Some(path.clone()), &"hello"));
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A missing file is an ordinary first run. A file full of nonsense is
    /// somebody's hand edit. Both mean "no usable preferences".
    #[test]
    fn anything_unreadable_falls_back_to_the_default() {
        let dir = scratch("garbage");
        std::fs::create_dir_all(&dir).unwrap();
        let missing: Vec<u32> = load_or_default(Some(dir.join("nothing.json")));
        assert!(missing.is_empty());

        let broken = dir.join("broken.json");
        std::fs::write(&broken, b"{ not json at all").unwrap();
        let read: Vec<u32> = load_or_default(Some(broken));
        assert!(read.is_empty());

        // No path at all — the platform has no config directory.
        let none: Vec<u32> = load_or_default(None);
        assert!(none.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The whole point: a save replaces the old file completely, and never
    /// leaves the temporary one lying beside it.
    #[test]
    fn a_save_replaces_the_old_file_and_tidies_up_after_itself() {
        let dir = scratch("replace");
        let path = dir.join("thing.json");
        assert!(save(Some(path.clone()), &"first"));
        assert!(save(Some(path.clone()), &"second"));
        let read: String = load_or_default(Some(path.clone()));
        assert_eq!(read, "second");

        let strays: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name != "thing.json")
            .collect();
        assert!(strays.is_empty(), "left behind: {strays:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// History and saved logins are nobody else's business, and a mode set at
    /// creation has no window where the file is briefly world-readable.
    #[cfg(unix)]
    #[test]
    fn files_are_readable_only_by_their_owner() {
        use std::os::unix::fs::PermissionsExt as _;

        let dir = scratch("mode");
        let path = dir.join("secret.json");
        assert!(save(Some(path.clone()), &"hush"));
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "mode was {:o}", mode & 0o777);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A save that cannot happen has to say so, because the caller may be
    /// holding the only record that there is still something to write.
    #[test]
    fn a_save_that_fails_reports_it() {
        // A path whose parent is a *file* cannot be created.
        let dir = scratch("blocked");
        std::fs::create_dir_all(&dir).unwrap();
        let blocker = dir.join("in-the-way");
        std::fs::write(&blocker, b"x").unwrap();
        assert!(!save(Some(blocker.join("thing.json")), &"nope"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
