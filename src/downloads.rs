//! File downloads.
//!
//! Servo upstream has no download subsystem; this build carries jdm's
//! prototype (servo/servo#40210) ported forward and completed, which offers
//! any response the renderer cannot display to the embedder.
//!
//! Transport is the engine itself: Servo performs the fetch — so cookies,
//! auth headers, redirects and cache all apply — and streams the response to
//! us through `WebViewDelegate::notify_response_chunk`. We only choose where
//! the bytes land.

// Several helpers here are only reachable with the `engine-downloads`
// feature; they stay compiled so the module keeps a single shape.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub type DownloadId = u64;

#[derive(Clone, PartialEq)]
pub enum DownloadState {
    Running,
    Complete,
    Cancelled,
    Failed(String),
}

pub struct Download {
    pub id: DownloadId,
    pub url: String,
    pub filename: String,
    pub path: PathBuf,
    pub received: u64,
    pub total: Option<u64>,
    pub state: DownloadState,
    cancel: Arc<AtomicBool>,
}

impl Download {
    /// Completion in 0..1 when the total size is known.
    pub fn fraction(&self) -> Option<f32> {
        let total = self.total?;
        (total > 0).then(|| (self.received as f32 / total as f32).clamp(0.0, 1.0))
    }
}

pub struct DownloadManager {
    pub items: Vec<Download>,
    next_id: DownloadId,
    /// Downloads the engine is streaming to us, keyed by its request id.
    #[cfg(feature = "engine-downloads")]
    engine: std::collections::HashMap<servo::RequestId, EngineDownload>,
}

/// A download whose bytes arrive from Servo itself.
#[cfg(feature = "engine-downloads")]
struct EngineDownload {
    id: DownloadId,
    file: std::fs::File,
    part_path: PathBuf,
    final_path: PathBuf,
    received: u64,
}

impl Default for DownloadManager {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            next_id: 0,
            #[cfg(feature = "engine-downloads")]
            engine: std::collections::HashMap::new(),
        }
    }
}

impl DownloadManager {
    pub fn active_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.state == DownloadState::Running)
            .count()
    }

    /// Accept a response Servo could not render, and start writing it to disk.
    /// Returns false if the destination could not be opened, in which case the
    /// caller should decline the offer.
    #[cfg(feature = "engine-downloads")]
    pub fn accept_from_engine(
        &mut self,
        request_id: servo::RequestId,
        url: &str,
        default_filename: &str,
    ) -> bool {
        let filename = if default_filename.trim().is_empty() {
            filename_from_url(url)
        } else {
            sanitize(default_filename)
        };
        let final_path = unique_path(&downloads_dir(), &filename);
        let part_path = final_path.with_extension(format!(
            "{}.part",
            final_path
                .extension()
                .map(|extension| extension.to_string_lossy())
                .unwrap_or_default()
        ));
        let Ok(file) = std::fs::File::create(&part_path) else {
            return false;
        };

        let id = self.next_id;
        self.next_id += 1;
        self.items.push(Download {
            id,
            url: url.to_owned(),
            filename: final_path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or(filename),
            path: final_path.clone(),
            received: 0,
            total: None,
            state: DownloadState::Running,
            cancel: Arc::new(AtomicBool::new(false)),
        });
        self.engine.insert(
            request_id,
            EngineDownload {
                id,
                file,
                part_path,
                final_path,
                received: 0,
            },
        );
        true
    }

    /// A chunk of an engine-driven download.
    #[cfg(feature = "engine-downloads")]
    pub fn engine_chunk(&mut self, request_id: servo::RequestId, chunk: &[u8]) {
        let Some(entry) = self.engine.get_mut(&request_id) else {
            return;
        };
        use std::io::Write as _;
        if entry.file.write_all(chunk).is_err() {
            return;
        }
        entry.received += chunk.len() as u64;
        let (id, received) = (entry.id, entry.received);
        if let Some(item) = self.items.iter_mut().find(|item| item.id == id) {
            item.received = received;
        }
    }

    /// The engine finished (or failed) an engine-driven download.
    #[cfg(feature = "engine-downloads")]
    pub fn engine_finished(&mut self, request_id: servo::RequestId, ok: bool) {
        let Some(entry) = self.engine.remove(&request_id) else {
            return;
        };
        drop(entry.file);
        let state = if ok && std::fs::rename(&entry.part_path, &entry.final_path).is_ok() {
            quarantine(&entry.final_path);
            DownloadState::Complete
        } else {
            let _ = std::fs::remove_file(&entry.part_path);
            DownloadState::Failed("transfer failed".to_owned())
        };
        if let Some(item) = self.items.iter_mut().find(|item| item.id == entry.id) {
            item.state = state;
        }
    }

    pub fn cancel(&mut self, id: DownloadId) {
        if let Some(item) = self.items.iter_mut().find(|item| item.id == id) {
            item.cancel.store(true, Ordering::Relaxed);
            if item.state == DownloadState::Running {
                item.state = DownloadState::Cancelled;
            }
        }
    }

    pub fn remove(&mut self, id: DownloadId) {
        self.cancel(id);
        self.items.retain(|item| item.id != id);
    }

    pub fn clear_finished(&mut self) {
        self.items
            .retain(|item| item.state == DownloadState::Running);
    }
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        // Read the two digits out of `bytes`, not by slicing the `&str`.
        // Slicing at `index + 1..index + 3` panics the moment those offsets
        // land inside a multi-byte character — `%aé` was enough. Nothing
        // reaches this with non-ASCII today, because the one caller feeds it
        // output from `url::Url` which has already percent-encoded everything
        // else, but a function that panics on its own input type is a trap
        // waiting for the second caller.
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let digits = [bytes[index + 1], bytes[index + 2]];
            if digits.iter().all(u8::is_ascii_hexdigit) {
                let text = std::str::from_utf8(&digits).unwrap_or("");
                if let Ok(byte) = u8::from_str_radix(text, 16) {
                    out.push(byte);
                    index += 3;
                    continue;
                }
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn filename_from_url(url: &str) -> String {
    let name = url::Url::parse(url)
        .ok()
        .and_then(|parsed| {
            parsed
                .path_segments()
                .and_then(|segments| segments.last().map(|segment| segment.to_owned()))
        })
        .filter(|segment| !segment.is_empty())
        .unwrap_or_else(|| "download".to_owned());
    sanitize(&percent_decode(&name))
}

/// The name a server-chosen filename will actually land under, for showing
/// somebody before they agree to it.
pub fn sanitize_public(name: &str) -> String {
    sanitize(name)
}

/// Names that are safe to create in a directory, from names a server chose.
///
/// The server picks this, so every rule here is about a name written to be
/// read as something it is not.
fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|character| {
            // Bidirectional overrides let a name render as its own reverse:
            // `photo\u{202e}gpj.exe` shows up in a file listing looking like
            // `photoexe.jpg`. Other browsers strip these for exactly this
            // reason. The rest of the C0/C1 range has no business in a filename
            // either — a newline in one makes a mess of every listing that
            // prints it.
            !matches!(character,
                '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{200e}' | '\u{200f}')
                && !character.is_control()
        })
        .map(|character| match character {
            '/' | '\\' | ':' => '_',
            other => other,
        })
        .collect();
    // Every leading dot goes, so a download can never arrive already hidden.
    // It does mean a file genuinely called `.bashrc` lands as `bashrc`, which
    // is the right trade for a directory nobody reads config out of. Keeping
    // one dot instead was tried and is worse: `../../x` sanitises to
    // `.._.._etc_x`, and keeping its first dot produces a hidden file out of
    // the very input the rule exists to defang.
    let cleaned = cleaned.trim().trim_start_matches('.');
    // Trailing dots and spaces are dropped by the Windows filesystem itself,
    // so `evil.exe.` and `evil.exe` are the same file there while looking
    // different in a listing.
    let cleaned = cleaned.trim_end_matches(['.', ' ']);
    if cleaned.is_empty() {
        return "download".to_owned();
    }
    // Reserved device names on Windows, with or without an extension: opening
    // `dir/NUL` opens the null device rather than creating a file.
    let stem = cleaned.split('.').next().unwrap_or("");
    const DEVICES: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if DEVICES
        .iter()
        .any(|device| stem.eq_ignore_ascii_case(device))
    {
        return format!("_{cleaned}");
    }
    cleaned.to_owned()
}

pub fn downloads_dir() -> PathBuf {
    dirs::download_dir().unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Downloads")
    })
}

/// Never overwrite: `file.zip` → `file (1).zip`.
fn unique_path(dir: &Path, filename: &str) -> PathBuf {
    let candidate = dir.join(filename);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(filename);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let extension = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    for index in 1..1000 {
        let candidate = dir.join(format!("{stem} ({index}){extension}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    // A thousand files under one name does not deserve a pretty answer, but it
    // must not be the *colliding* one — which is what this used to return,
    // quietly replacing a file somebody already had, three lines under a
    // doc comment promising never to. The clock gives a name nothing holds.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or_default();
    dir.join(format!("{stem} ({stamp}){extension}"))
}

/// Mark a finished download as having come from the internet.
///
/// macOS decides whether to warn before opening a file from this attribute, so
/// one written without it opens with no prompt at all — including a file whose
/// name and contents a page chose. Set after the rename, on the final path,
/// because that is the file somebody will actually open.
///
/// Written with `xattr` rather than a binding, for the same reason
/// `passwords.rs` shells out to `security`: the command is stable, this is not
/// a hot path, and it keeps a dependency out of the build.
#[cfg_attr(not(feature = "engine-downloads"), allow(dead_code))]
fn quarantine(path: &Path) {
    #[cfg(target_os = "macos")]
    {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_secs())
            .unwrap_or_default();
        // flags;hex timestamp;agent;UUID — 0001 is "downloaded", the flag that
        // makes Gatekeeper ask before the first open.
        let value = format!("0001;{stamp:x};Zervo;");
        let _ = std::process::Command::new("xattr")
            .args(["-w", "com.apple.quarantine", &value])
            .arg(path)
            .status();
    }
    #[cfg(not(target_os = "macos"))]
    let _ = path;
}

/// Reveal a finished file in the system file manager.
pub fn reveal(path: &Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn();
    // Explorer has a real reveal; elsewhere there is none, so open the
    // containing folder, which every desktop can do.
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer")
        .arg(format!("/select,{}", path.display()))
        .spawn();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = std::process::Command::new("xdg-open")
        .arg(path.parent().unwrap_or(path))
        .spawn();
}

/// Open a finished file with its default application.
pub fn open_file(path: &Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer").arg(path).spawn();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}

/// Human-readable byte count.
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole point of `sanitize`: nothing a server sends may address a
    /// directory other than the one the download is going into.
    #[test]
    fn a_name_cannot_climb_out_of_the_downloads_folder() {
        assert_eq!(sanitize("../../etc/passwd"), "_.._etc_passwd");
        assert_eq!(sanitize("/etc/passwd"), "_etc_passwd");
        assert_eq!(sanitize("C:\\Windows\\evil.exe"), "C__Windows_evil.exe");
        // Nothing but separators and dots leaves nothing to keep.
        assert_eq!(sanitize(".."), "download");
        assert_eq!(sanitize("   "), "download");
        assert_eq!(sanitize(""), "download");
    }

    /// A right-to-left override makes a name render as its own reverse, so
    /// `photo<RLO>gpj.exe` sits in a listing looking like `photoexe.jpg`. The
    /// extension the user reads must be the extension the file has.
    #[test]
    fn a_name_cannot_lie_about_its_extension() {
        let disguised = "photo\u{202e}gpj.exe";
        let cleaned = sanitize(disguised);
        assert!(
            !cleaned.contains('\u{202e}'),
            "the override survived: {cleaned:?}"
        );
        assert_eq!(cleaned, "photogpj.exe");
    }

    /// Control characters, newlines included, make a mess of every listing
    /// that prints the name and hide whatever follows them.
    #[test]
    fn control_characters_do_not_survive() {
        assert_eq!(sanitize("re\nport.pdf"), "report.pdf");
        assert_eq!(sanitize("re\u{0}port.pdf"), "report.pdf");
    }

    /// Opening `NUL` on Windows opens the null device rather than creating a
    /// file, whatever extension it is wearing.
    #[test]
    fn windows_device_names_are_pushed_out_of_the_way() {
        assert_eq!(sanitize("NUL"), "_NUL");
        assert_eq!(sanitize("nul.txt"), "_nul.txt");
        assert_eq!(sanitize("COM1.zip"), "_COM1.zip");
        // Trailing dots and spaces are dropped by the filesystem itself, so
        // `evil.exe.` and `evil.exe` are one file wearing two names.
        assert_eq!(sanitize("evil.exe."), "evil.exe");
        assert_eq!(sanitize("evil.exe "), "evil.exe");
        // A name that merely starts the same way is not a device.
        assert_eq!(sanitize("console.log"), "console.log");
    }

    /// A download never arrives already hidden — including the sanitised form
    /// of a traversal attempt, which starts with the dots its separators left
    /// behind.
    #[test]
    fn nothing_lands_as_a_hidden_file() {
        assert_eq!(sanitize(".bashrc"), "bashrc");
        assert_eq!(sanitize("....hidden"), "hidden");
        assert!(!sanitize("../../etc/passwd").starts_with('.'));
    }

    /// `percent_decode` used to slice the `&str` at byte offsets, which panics
    /// the moment they land inside a multi-byte character.
    #[test]
    fn percent_decoding_survives_non_ascii() {
        assert_eq!(percent_decode("a%20b.zip"), "a b.zip");
        // The one that panicked: `%` followed by a byte of a two-byte char.
        assert_eq!(percent_decode("%aé"), "%aé");
        assert_eq!(percent_decode("%zz"), "%zz");
        assert_eq!(percent_decode("%2"), "%2");
        assert_eq!(percent_decode("%"), "%");
    }

    /// Decoding happens before sanitising, so an encoded separator cannot
    /// smuggle a path through.
    #[test]
    fn an_encoded_separator_is_still_a_separator() {
        assert_eq!(
            filename_from_url("https://x/%2Fetc%2Fpasswd"),
            "_etc_passwd"
        );
        assert_eq!(filename_from_url("https://x/a%20b.zip"), "a b.zip");
        assert_eq!(filename_from_url("https://x/"), "download");
        assert_eq!(filename_from_url("not a url"), "download");
    }

    #[test]
    fn bytes_are_reported_in_units_a_person_reads() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        // The largest number there is must not come out as "inf" or panic.
        let huge = format_bytes(u64::MAX);
        assert!(
            !huge.contains("inf") && !huge.contains("NaN"),
            "u64::MAX formatted as {huge:?}"
        );
    }
}
