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
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&input[index + 1..index + 3], 16) {
                out.push(byte);
                index += 3;
                continue;
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

fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '\0' => '_',
            other => other,
        })
        .collect();
    let cleaned = cleaned.trim().trim_start_matches('.').to_owned();
    if cleaned.is_empty() {
        "download".to_owned()
    } else {
        cleaned
    }
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
    candidate
}

/// Reveal a finished file in the system file manager.
pub fn reveal(path: &Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn();
    // No portable "reveal", so open the containing folder. Good enough, and
    // every desktop has it.
    #[cfg(not(target_os = "macos"))]
    let _ = std::process::Command::new("xdg-open")
        .arg(path.parent().unwrap_or(path))
        .spawn();
}

/// Open a finished file with its default application.
pub fn open_file(path: &Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(not(target_os = "macos"))]
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
