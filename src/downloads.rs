//! File downloads.
//!
//! The engine keeps the response from the original navigation. Heion asks
//! the user before accepting it and writes the resulting stream off the UI thread.

// Several helpers here are only reachable with the `engine-downloads`
// feature; they stay compiled so the module keeps a single shape.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
#[cfg(feature = "engine-downloads")]
use std::sync::Arc;
#[cfg(feature = "engine-downloads")]
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
    pub can_retry: bool,
    #[cfg(feature = "engine-downloads")]
    pub source: servo::WebViewId,
    #[cfg(feature = "engine-downloads")]
    transfer: Option<Transfer>,
}

/// One download as a list row wants it.
///
/// The three places that draw a download — the hover card, the
/// `zervo://downloads` page and the new tab page's widget — each need the same
/// handful of facts and none of the rest of a `Download`. Naming them is also
/// what stops the copy taken for a row being a bare seven-tuple.
#[derive(Clone)]
pub struct RowView {
    pub id: DownloadId,
    pub filename: String,
    pub url: String,
    /// Completion in 0..1, when the total size is known.
    pub fraction: Option<f32>,
    pub received: u64,
    pub total: Option<u64>,
    pub state: DownloadState,
    pub can_retry: bool,
}

impl Download {
    /// The facts a row needs, copied out so the list can be walked while the
    /// manager is borrowed elsewhere.
    pub fn row_view(&self) -> RowView {
        RowView {
            id: self.id,
            filename: self.filename.clone(),
            url: self.url.clone(),
            fraction: self.fraction(),
            received: self.received,
            total: self.total,
            state: self.state.clone(),
            can_retry: self.can_retry && self.state != DownloadState::Running,
        }
    }

    /// Completion in 0..1 when the total size is known.
    pub fn fraction(&self) -> Option<f32> {
        let total = self.total?;
        (total > 0).then(|| (self.received as f32 / total as f32).clamp(0.0, 1.0))
    }
}

#[derive(Default)]
pub struct DownloadManager {
    pub items: Vec<Download>,
    next_id: DownloadId,
}

#[cfg(feature = "engine-downloads")]
struct Transfer {
    cancelled: Arc<AtomicBool>,
    cancellation: servo::DownloadCancellation,
}

#[cfg(feature = "engine-downloads")]
#[derive(Debug)]
pub(crate) enum Event {
    Progress(DownloadId, u64),
    Finished(DownloadId, u64, TransferResult),
}

#[cfg(feature = "engine-downloads")]
#[derive(Debug)]
pub(crate) enum TransferResult {
    Complete(PathBuf),
    Cancelled,
    Failed,
}

impl DownloadManager {
    pub fn active_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.state == DownloadState::Running)
            .count()
    }

    /// Open a private partial file before taking ownership of the response.
    #[cfg(feature = "engine-downloads")]
    pub fn accept_from_engine(
        &mut self,
        response: &mut servo::UnsupportedResponse,
        filename: &str,
        proxy: winit::event_loop::EventLoopProxy<crate::WakerEvent>,
    ) {
        let id = self.next_id;
        self.next_id += 1;
        let directory = downloads_dir();
        let mut item = Download {
            id,
            url: response.url.to_string(),
            filename: filename.to_owned(),
            path: directory.join(filename),
            received: 0,
            total: response.content_length,
            state: download_failed(),
            can_retry: response.can_retry,
            source: response.webview_id,
            transfer: None,
        };

        if let Ok(file) = tempfile::Builder::new()
            .prefix(".heion-")
            .suffix(".part")
            .tempfile_in(&directory)
            && let Some(stream) = response.accept()
        {
            let cancelled = Arc::new(AtomicBool::new(false));
            let transfer = Transfer {
                cancelled: cancelled.clone(),
                cancellation: stream.cancellation(),
            };
            let filename = filename.to_owned();
            let length = item.total;
            let started = std::thread::Builder::new()
                .name("heion-download".into())
                .spawn(move || {
                    let mut received = 0;
                    let result = write_download(
                        stream,
                        file,
                        &directory,
                        &filename,
                        length,
                        &cancelled,
                        &mut received,
                        |received| {
                            proxy
                                .send_event(crate::WakerEvent::Download(Event::Progress(
                                    id, received,
                                )))
                                .is_ok()
                        },
                    );
                    let _ = proxy.send_event(crate::WakerEvent::Download(Event::Finished(
                        id, received, result,
                    )));
                });
            if started.is_ok() {
                item.state = DownloadState::Running;
                item.transfer = Some(transfer);
            }
        } else {
            response.decline();
        }

        self.items.push(item);
    }

    #[cfg(feature = "engine-downloads")]
    pub(crate) fn apply_event(&mut self, event: Event) {
        let id = match &event {
            Event::Progress(id, _) | Event::Finished(id, _, _) => *id,
        };
        let Some(item) = self.items.iter_mut().find(|item| item.id == id) else {
            return;
        };
        match event {
            Event::Progress(_, received) => item.received = received,
            Event::Finished(_, received, result) => {
                item.received = received;
                item.transfer = None;
                item.state = match result {
                    TransferResult::Complete(path) => {
                        if let Some(name) = path.file_name() {
                            item.filename = name.to_string_lossy().into_owned();
                        }
                        item.path = path;
                        DownloadState::Complete
                    },
                    TransferResult::Cancelled => DownloadState::Cancelled,
                    TransferResult::Failed => download_failed(),
                };
            },
        }
    }

    pub fn cancel(&mut self, id: DownloadId) {
        if let Some(item) = self.items.iter_mut().find(|item| item.id == id)
            && item.state == DownloadState::Running
        {
            #[cfg(feature = "engine-downloads")]
            if let Some(transfer) = &item.transfer {
                transfer.cancelled.store(true, Ordering::Relaxed);
                transfer.cancellation.cancel();
            }
            item.state = DownloadState::Cancelled;
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

#[cfg(feature = "engine-downloads")]
fn download_failed() -> DownloadState {
    DownloadState::Failed(crate::l10n::text("downloads-transfer-failed").to_owned())
}

#[cfg(feature = "engine-downloads")]
fn write_download(
    mut stream: servo::DownloadStream,
    mut file: tempfile::NamedTempFile,
    directory: &Path,
    filename: &str,
    length: Option<u64>,
    cancelled: &AtomicBool,
    received: &mut u64,
    progress: impl Fn(u64) -> bool,
) -> TransferResult {
    use std::io::Write as _;
    use std::time::{Duration, Instant};

    let mut last_progress = Instant::now();
    loop {
        let chunk = stream.read_chunk();
        if cancelled.load(Ordering::Relaxed) {
            return TransferResult::Cancelled;
        }
        let chunk = match chunk {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(error) => {
                log::debug!("Download did not finish: {error:?}");
                return TransferResult::Failed;
            },
        };
        if let Err(error) = file.write_all(&chunk) {
            log::warn!("Cannot write download: {error}");
            return TransferResult::Failed;
        }
        *received += chunk.len() as u64;
        if last_progress.elapsed() >= Duration::from_millis(100) {
            if !progress(*received) {
                return TransferResult::Cancelled;
            }
            last_progress = Instant::now();
        }
    }

    if length.is_some_and(|length| length != *received) || file.as_file().sync_all().is_err() {
        return TransferResult::Failed;
    }
    if cancelled.load(Ordering::Relaxed) {
        return TransferResult::Cancelled;
    }
    quarantine(file.path());
    match persist_download(file, directory, filename) {
        Ok(path) => TransferResult::Complete(path),
        Err(error) => {
            log::warn!("Cannot save download: {error}");
            TransferResult::Failed
        },
    }
}

#[cfg(feature = "engine-downloads")]
fn persist_download(
    mut file: tempfile::NamedTempFile,
    directory: &Path,
    filename: &str,
) -> std::io::Result<PathBuf> {
    loop {
        let path = unique_path(directory, filename);
        match file.persist_noclobber(&path) {
            Ok(_) => return Ok(path),
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                file = error.file
            },
            Err(error) => return Err(error.error),
        }
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
                // `next_back`, not `last`: path segments iterate both ways, and
                // `last` walks the whole path to reach the end of it.
                .and_then(|mut segments| segments.next_back().map(std::borrow::ToOwned::to_owned))
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
    if matches!(candidate.symlink_metadata(), Err(error) if error.kind() == std::io::ErrorKind::NotFound)
    {
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
        if matches!(candidate.symlink_metadata(), Err(error) if error.kind() == std::io::ErrorKind::NotFound)
        {
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
#[cfg_attr(not(feature = "engine-downloads"), expect(dead_code))]
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
pub fn format_progress(received: u64, total: u64) -> String {
    crate::l10n::message(
        "downloads-transfer-progress",
        [
            ("received", format_bytes(received).into()),
            ("total", format_bytes(total).into()),
        ],
    )
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = [
        "downloads-size-bytes",
        "downloads-size-kilobytes",
        "downloads-size-megabytes",
        "downloads-size-gigabytes",
        "downloads-size-terabytes",
    ];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    let amount = if unit == 0 {
        bytes.to_string()
    } else {
        format!("{value:.1}")
    };
    crate::l10n::message(UNITS[unit], [("amount", amount.into())])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "engine-downloads")]
    #[test]
    fn concurrent_downloads_do_not_overwrite_each_other() {
        use std::io::Write as _;
        use std::sync::Barrier;

        let dir = tempfile::tempdir().unwrap();
        let gate = Barrier::new(2);
        let download = |contents: &[u8]| {
            let mut file = tempfile::NamedTempFile::new_in(dir.path()).unwrap();
            file.write_all(contents).unwrap();
            gate.wait();
            persist_download(file, dir.path(), "same.txt").unwrap()
        };
        std::thread::scope(|scope| {
            let first = scope.spawn(|| download(b"first"));
            let second = scope.spawn(|| download(b"second"));
            let first = first.join().unwrap();
            let second = second.join().unwrap();
            assert_ne!(first, second);
            assert_eq!(std::fs::read(first).unwrap(), b"first");
            assert_eq!(std::fs::read(second).unwrap(), b"second");
        });
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }

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
        let visible_bytes = |bytes| format_bytes(bytes).replace(['\u{2068}', '\u{2069}'], "");
        assert_eq!(visible_bytes(0), "0 B");
        assert_eq!(visible_bytes(1023), "1023 B");
        assert_eq!(visible_bytes(1024), "1.0 KB");
        assert_eq!(visible_bytes(1536), "1.5 KB");
        // The largest number there is must not come out as "inf" or panic.
        let huge = format_bytes(u64::MAX);
        assert!(
            !huge.contains("inf") && !huge.contains("NaN"),
            "u64::MAX formatted as {huge:?}"
        );
    }
}
