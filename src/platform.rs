//! The handful of things that differ per platform outside macOS.
//!
//! macOS has its own paths inline, where they can use the AppKit bindings the
//! chrome already depends on. This is for everywhere else: a file dialog and a
//! credential store, each reached through whatever the desktop provides rather
//! than through a binding, so no platform pulls in a dependency the others do
//! not need.

use std::path::PathBuf;

/// One file, for the settings page's import.
pub fn pick_file() -> Option<PathBuf> {
    pick_files(false).into_iter().next()
}

/// Ask the desktop for files. Empty means cancelled, or that there is nothing
/// here to ask.
#[cfg(target_os = "windows")]
pub fn pick_files(multiple: bool) -> Vec<PathBuf> {
    // WinForms through PowerShell rather than a Win32 binding: the dialog is
    // the system one either way, and this keeps a crate out of the build for a
    // feature used a few times a session.
    let script = format!(
        "Add-Type -AssemblyName System.Windows.Forms; \
         $d = New-Object System.Windows.Forms.OpenFileDialog; \
         $d.Multiselect = ${multiple}; \
         if ($d.ShowDialog() -eq 'OK') {{ $d.FileNames -join \"`n\" }}"
    );
    let Ok(output) = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
    else {
        log::warn!("could not open a file dialog");
        return Vec::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn pick_files(multiple: bool) -> Vec<PathBuf> {
    let mut command = std::process::Command::new("zenity");
    command.arg("--file-selection");
    if multiple {
        command.args(["--multiple", "--separator=\n"]);
    }
    let Ok(output) = command.output() else {
        log::warn!("no file dialog available: install zenity to choose files");
        return Vec::new();
    };
    if !output.status.success() {
        // A non-zero exit is how zenity reports that the user cancelled.
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect()
}
