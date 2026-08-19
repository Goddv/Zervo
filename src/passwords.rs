//! Saved logins.
//!
//! Two things worth knowing before reading further.
//!
//! **Zervo cannot fill or capture web form logins.** Servo's embedder API has
//! no hook for a submitted form and no way to write into a page's fields, so
//! there is nothing to hang autofill on. What this is, then, is a vault: you
//! put logins in, Zervo keeps them, and it uses them for the one thing the
//! engine *does* ask the embedder about — HTTP authentication. If Servo grows
//! a credential API, this is where filling would attach.
//!
//! **Secrets never touch Zervo's own files.** They go to the operating
//! system's credential store, which is built for exactly this and is not
//! something worth reimplementing. What Zervo stores is an index: which site
//! and username it knows about, so it can list them without asking the
//! keychain to unlock. Deleting `passwords.json` loses the index, not the
//! secrets.

use std::process::Command;

use serde::{Deserialize, Serialize};

/// One saved login. No password here — see the module docs.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Login {
    /// Host the login belongs to, e.g. `example.com`.
    pub site: String,
    pub username: String,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Vault {
    logins: Vec<Login>,
}

/// What went wrong, in words worth showing someone.
pub type Failure = String;

impl Vault {
    pub fn load() -> Self {
        let Some(path) = index_path() else {
            return Self::default();
        };
        std::fs::read_to_string(path)
            .ok()
            .and_then(|contents| serde_json::from_str(&contents).ok())
            .unwrap_or_default()
    }

    fn save_index(&self) {
        let Some(path) = index_path() else { return };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    pub fn logins(&self) -> &[Login] {
        &self.logins
    }

    pub fn is_empty(&self) -> bool {
        self.logins.is_empty()
    }

    /// Add or replace a login. Storing under a site and username that already
    /// exist overwrites the password, which is what saving again means.
    pub fn set(&mut self, site: &str, username: &str, password: &str) -> Result<(), Failure> {
        let site = normalise(site);
        if site.is_empty() || username.is_empty() {
            return Err("A site and a username are both needed.".to_owned());
        }
        store_secret(&site, username, password)?;
        let login = Login {
            site,
            username: username.to_owned(),
        };
        if !self.logins.contains(&login) {
            self.logins.push(login);
        }
        self.logins
            .sort_by(|a, b| (&a.site, &a.username).cmp(&(&b.site, &b.username)));
        self.save_index();
        Ok(())
    }

    pub fn remove(&mut self, site: &str, username: &str) {
        let _ = delete_secret(site, username);
        self.logins
            .retain(|login| !(login.site == site && login.username == username));
        self.save_index();
    }

    /// The best match for a host, used when the engine asks for HTTP
    /// authentication. Exact host first, then a parent domain, so a login for
    /// `example.com` covers `www.example.com`.
    pub fn for_host(&self, host: &str) -> Option<(Login, String)> {
        let host = normalise(host);
        let matched = self
            .logins
            .iter()
            .find(|login| login.site == host)
            .or_else(|| {
                self.logins
                    .iter()
                    .find(|login| host.ends_with(&format!(".{}", login.site)))
            })?
            .clone();
        let password = read_secret(&matched.site, &matched.username)?;
        Some((matched, password))
    }

    /// Write every login, passwords included, as JSON.
    ///
    /// The file is plaintext. Every browser's export works this way and there
    /// is no way around it — an export nothing else can read is not an export —
    /// but it does mean the result deserves the same care as the passwords
    /// themselves.
    pub fn export(&self, path: &std::path::Path) -> Result<usize, Failure> {
        let mut exported = Vec::new();
        for login in &self.logins {
            let password = read_secret(&login.site, &login.username).unwrap_or_default();
            exported.push(Exported {
                site: login.site.clone(),
                username: login.username.clone(),
                password,
            });
        }
        let json = serde_json::to_string_pretty(&exported)
            .map_err(|error| format!("Could not write the export: {error}"))?;
        std::fs::write(path, json).map_err(|error| format!("Could not write {path:?}: {error}"))?;
        Ok(exported.len())
    }

    /// Read an export back. Existing logins for the same site and username are
    /// overwritten.
    pub fn import(&mut self, path: &std::path::Path) -> Result<usize, Failure> {
        let contents = std::fs::read_to_string(path)
            .map_err(|error| format!("Could not read {path:?}: {error}"))?;
        let entries: Vec<Exported> = serde_json::from_str(&contents)
            .map_err(|error| format!("That does not look like a Zervo export: {error}"))?;
        let mut imported = 0;
        for entry in entries {
            if self
                .set(&entry.site, &entry.username, &entry.password)
                .is_ok()
            {
                imported += 1;
            }
        }
        Ok(imported)
    }
}

#[derive(Serialize, Deserialize)]
struct Exported {
    site: String,
    username: String,
    password: String,
}

/// Compare hosts, not URLs: `https://example.com/login` and `example.com` are
/// the same site as far as a saved login is concerned.
fn normalise(site: &str) -> String {
    let site = site.trim().to_lowercase();
    let site = site
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(&site);
    site.split('/').next().unwrap_or("").to_owned()
}

/// Secrets go into the store hex-encoded.
///
/// `security find-generic-password -w` prints the password as-is when it is
/// printable ASCII and as a hex dump when it is not, with nothing to say which
/// you got — so a password containing any non-ASCII character came back as
/// gibberish. Encoding it here means what goes in is always ASCII, and what
/// comes out is always something we know how to read.
fn encode(secret: &str) -> String {
    secret.bytes().map(|byte| format!("{byte:02x}")).collect()
}

fn decode(stored: &str) -> Option<String> {
    if stored.is_empty() || stored.len() % 2 != 0 || !stored.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return None;
    }
    let bytes: Option<Vec<u8>> = (0..stored.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&stored[at..at + 2], 16).ok())
        .collect();
    String::from_utf8(bytes?).ok()
}

fn service(site: &str) -> String {
    format!("Zervo — {site}")
}

fn index_path() -> Option<std::path::PathBuf> {
    Some(crate::settings::data_dir()?.join("passwords.json"))
}

// ── The OS credential store.
//
// Shelling out to the platform tool rather than binding its library: the
// commands are stable, this is not a hot path, and it keeps a security-critical
// dependency out of the build. The cost is that the password is an argument for
// as long as the command runs, so it is briefly visible to anyone who can list
// processes on the machine.

#[cfg(target_os = "macos")]
fn store_secret(site: &str, username: &str, password: &str) -> Result<(), Failure> {
    run(
        Command::new("security").args([
            "add-generic-password",
            "-s",
            &service(site),
            "-a",
            username,
            "-w",
            &encode(password),
            // Update in place if it is already there.
            "-U",
        ]),
        "save the password to your keychain",
    )
}

#[cfg(target_os = "macos")]
fn read_secret(site: &str, username: &str) -> Option<String> {
    let output = Command::new("security")
        .args([
            "find-generic-password",
            "-s",
            &service(site),
            "-a",
            username,
            "-w",
        ])
        .output()
        .ok()?;
    let stored = output.status.success().then(|| {
        String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_owned()
    })?;
    // Anything saved before the encoding went in reads back as itself.
    Some(decode(&stored).unwrap_or(stored))
}

#[cfg(target_os = "macos")]
fn delete_secret(site: &str, username: &str) -> Result<(), Failure> {
    run(
        Command::new("security").args([
            "delete-generic-password",
            "-s",
            &service(site),
            "-a",
            username,
        ]),
        "remove the password from your keychain",
    )
}

#[cfg(target_os = "windows")]
fn store_secret(site: &str, username: &str, password: &str) -> Result<(), Failure> {
    // DPAPI through PowerShell. Windows has no keychain command that hands a
    // password back, but it does have per-user encryption, which is the
    // property that matters: the file is useless to any other account.
    let path = secret_path(site, username)?;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let script = format!(
        "ConvertTo-SecureString -String '{}' -AsPlainText -Force | ConvertFrom-SecureString",
        encode(password)
    );
    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|error| format!("Could not run PowerShell to encrypt the password: {error}"))?;
    if !output.status.success() {
        return Err("Windows would not encrypt the password.".to_owned());
    }
    std::fs::write(&path, String::from_utf8_lossy(&output.stdout).trim())
        .map_err(|error| format!("Could not save the password: {error}"))
}

#[cfg(target_os = "windows")]
fn read_secret(site: &str, username: &str) -> Option<String> {
    let stored = std::fs::read_to_string(secret_path(site, username).ok()?).ok()?;
    let script = format!(
        "$s = ConvertTo-SecureString '{}'; \
         [Runtime.InteropServices.Marshal]::PtrToStringAuto( \
           [Runtime.InteropServices.Marshal]::SecureStringToBSTR($s))",
        stored.trim()
    );
    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .and_then(|hex| decode(&hex))
}

#[cfg(target_os = "windows")]
fn delete_secret(site: &str, username: &str) -> Result<(), Failure> {
    let path = secret_path(site, username)?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Could not remove the password: {error}")),
    }
}

/// Where one encrypted secret lives. Named from the site and username rather
/// than an index, so the file and the entry cannot drift apart.
#[cfg(target_os = "windows")]
fn secret_path(site: &str, username: &str) -> Result<std::path::PathBuf, Failure> {
    let safe: String = format!("{site}-{username}")
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    crate::settings::data_dir()
        .map(|dir| dir.join("credentials").join(format!("{safe}.dpapi")))
        .ok_or_else(|| "No place to keep credentials.".to_owned())
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn store_secret(site: &str, username: &str, password: &str) -> Result<(), Failure> {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = Command::new("secret-tool")
        .args([
            "store",
            "--label",
            &service(site),
            "service",
            &service(site),
            "account",
            username,
        ])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|_| {
            "No credential store found. Install libsecret (secret-tool) to save passwords."
                .to_owned()
        })?;
    // secret-tool takes the secret on stdin, which keeps it out of the process
    // list — better than the macOS path manages.
    if let Some(stdin) = child.stdin.as_mut() {
        let _ = stdin.write_all(encode(password).as_bytes());
    }
    match child.wait() {
        Ok(status) if status.success() => Ok(()),
        _ => Err("The credential store refused to save the password.".to_owned()),
    }
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn read_secret(site: &str, username: &str) -> Option<String> {
    let output = Command::new("secret-tool")
        .args(["lookup", "service", &service(site), "account", username])
        .output()
        .ok()?;
    let stored = output.status.success().then(|| {
        String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_owned()
    })?;
    Some(decode(&stored).unwrap_or(stored))
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn delete_secret(site: &str, username: &str) -> Result<(), Failure> {
    run(
        Command::new("secret-tool").args(["clear", "service", &service(site), "account", username]),
        "remove the password from your credential store",
    )
}

fn run(command: &mut Command, what: &str) -> Result<(), Failure> {
    match command.output() {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => {
            let detail = String::from_utf8_lossy(&output.stderr);
            Err(format!("Could not {what}: {}", detail.trim()))
        },
        Err(error) => Err(format!("Could not {what}: {error}")),
    }
}
