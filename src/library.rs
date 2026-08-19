//! Favourites and history, and the file they live in.
//!
//! Both are small enough to keep in memory and write out as one JSON file
//! beside the settings. History is capped rather than pruned by age: a cap is
//! predictable, and nobody wants to discover their history vanished because a
//! date arrived.

use std::collections::HashMap;

use chrono::{DateTime, Datelike, Local, TimeZone};
use serde::{Deserialize, Serialize};

/// How many visits to keep. Roughly a year of ordinary browsing.
const HISTORY_LIMIT: usize = 20_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Favourite {
    pub url: String,
    pub title: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Visit {
    pub url: String,
    pub title: String,
    /// Unix seconds. Stored rather than a formatted string so the grouping can
    /// change without rewriting the file.
    pub at: i64,
}

impl Visit {
    pub fn local_time(&self) -> DateTime<Local> {
        Local.timestamp_opt(self.at, 0).single().unwrap_or_default()
    }
}

/// How far back a group of visits reaches.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Bucket {
    Today,
    Yesterday,
    ThisWeek,
    ThisMonth,
    Earlier,
}

impl Bucket {
    pub fn label(self) -> &'static str {
        match self {
            Bucket::Today => "Today",
            Bucket::Yesterday => "Yesterday",
            Bucket::ThisWeek => "Earlier this week",
            Bucket::ThisMonth => "Earlier this month",
            Bucket::Earlier => "Older",
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Library {
    pub favourites: Vec<Favourite>,
    pub history: Vec<Visit>,
    /// Set when something changed and the file needs writing.
    #[serde(skip)]
    dirty: bool,
}

impl Library {
    pub fn load() -> Self {
        let Some(path) = path() else {
            return Self::default();
        };
        std::fs::read_to_string(path)
            .ok()
            .and_then(|contents| serde_json::from_str(&contents).ok())
            .unwrap_or_default()
    }

    pub fn save(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        let Some(path) = path() else { return };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    pub fn needs_save(&self) -> bool {
        self.dirty
    }

    // ── History

    /// Record a visit, unless it repeats the page already at the top. Servo
    /// reports the URL more than once for a single navigation, and a history
    /// listing full of consecutive duplicates is useless.
    pub fn record(&mut self, url: &str, title: &str, now: i64) {
        if url.is_empty() || url.starts_with("zervo://") || url == "about:blank" {
            return;
        }
        if let Some(last) = self.history.last_mut()
            && last.url == url
        {
            // Titles arrive after the URL, so fill it in rather than duplicate.
            if !title.is_empty() && last.title != title {
                last.title = title.to_owned();
                self.dirty = true;
            }
            return;
        }
        self.history.push(Visit {
            url: url.to_owned(),
            title: title.to_owned(),
            at: now,
        });
        if self.history.len() > HISTORY_LIMIT {
            let excess = self.history.len() - HISTORY_LIMIT;
            self.history.drain(..excess);
        }
        self.dirty = true;
    }

    pub fn forget(&mut self, index: usize) {
        if index < self.history.len() {
            self.history.remove(index);
            self.dirty = true;
        }
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
        self.dirty = true;
    }

    /// History newest first, filtered by `query`, grouped into buckets in the
    /// order they should be shown. Indices are into `self.history`, so a row
    /// can be removed without a second lookup.
    pub fn browse(&self, query: &str, now: i64) -> Vec<(Bucket, Vec<(usize, &Visit)>)> {
        let needle = query.trim().to_lowercase();
        let today = Local.timestamp_opt(now, 0).single().unwrap_or_default();

        let mut grouped: HashMap<Bucket, Vec<(usize, &Visit)>> = HashMap::new();
        for (index, visit) in self.history.iter().enumerate().rev() {
            if !needle.is_empty()
                && !visit.url.to_lowercase().contains(&needle)
                && !visit.title.to_lowercase().contains(&needle)
            {
                continue;
            }
            let when = visit.local_time();
            let days = (today.date_naive() - when.date_naive()).num_days();
            let bucket = if days <= 0 {
                Bucket::Today
            } else if days == 1 {
                Bucket::Yesterday
            } else if days < 7 {
                Bucket::ThisWeek
            } else if when.year() == today.year() && when.month() == today.month() {
                Bucket::ThisMonth
            } else {
                Bucket::Earlier
            };
            grouped.entry(bucket).or_default().push((index, visit));
        }

        [
            Bucket::Today,
            Bucket::Yesterday,
            Bucket::ThisWeek,
            Bucket::ThisMonth,
            Bucket::Earlier,
        ]
        .into_iter()
        .filter_map(|bucket| grouped.remove(&bucket).map(|rows| (bucket, rows)))
        .collect()
    }

    // ── Favourites

    pub fn is_favourite(&self, url: &str) -> bool {
        self.favourites.iter().any(|entry| entry.url == url)
    }

    /// Returns true if the page is a favourite afterwards.
    pub fn toggle_favourite(&mut self, url: &str, title: &str) -> bool {
        self.dirty = true;
        if let Some(at) = self.favourites.iter().position(|entry| entry.url == url) {
            self.favourites.remove(at);
            return false;
        }
        self.favourites.push(Favourite {
            url: url.to_owned(),
            title: if title.is_empty() {
                url.to_owned()
            } else {
                title.to_owned()
            },
        });
        true
    }

    pub fn rename_favourite(&mut self, url: &str, title: &str) {
        if let Some(entry) = self.favourites.iter_mut().find(|entry| entry.url == url) {
            entry.title = title.to_owned();
            self.dirty = true;
        }
    }

    pub fn remove_favourite(&mut self, url: &str) {
        self.favourites.retain(|entry| entry.url != url);
        self.dirty = true;
    }
}

fn path() -> Option<std::path::PathBuf> {
    Some(crate::settings::data_dir()?.join("library.json"))
}
