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

/// One line on the new tab page's to-do card.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Task {
    pub text: String,
    pub done: bool,
}

/// A site the history has been seen enough of to rank. Built from the visit
/// log rather than stored, so it never disagrees with it.
#[derive(Clone, Debug)]
pub struct Site {
    pub host: String,
    /// The most recent URL on that host, which is what a click opens.
    pub url: String,
    pub title: String,
    pub visits: usize,
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
    /// The new tab page's scratchpad.
    pub note: String,
    /// The new tab page's to-do card.
    pub tasks: Vec<Task>,
    /// Set when something changed and the file needs writing.
    #[serde(skip)]
    dirty: bool,
    /// Most-visited sites, worked out from the history and kept until the
    /// history moves. Twenty thousand visits is not much to walk, but walking
    /// them thirty times a second while the new tab page animates is.
    #[serde(skip)]
    ranked: Vec<Site>,
    /// Whether `ranked` reflects the history as it stands. Not `ranked
    /// .is_empty()`: a history of nothing but addresses with no host in them
    /// ranks to nothing, and that answer is worth keeping too.
    #[serde(skip)]
    ranked_built: bool,
}

impl Library {
    pub fn load() -> Self {
        crate::store::load_or_default(path())
    }

    pub fn save(&mut self) {
        if !self.dirty {
            return;
        }
        // Cleared once the bytes are on disk, not before. Clearing first — which
        // is what this did — means a write that fails is never retried: every
        // visit and every favourite since the last good save is gone, and
        // nothing anywhere says so. The flag is the only record that there is
        // still something to write.
        if crate::store::save(path(), self) {
            self.dirty = false;
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
                self.ranked_built = false;
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
        self.ranked_built = false;
    }

    pub fn forget(&mut self, index: usize) {
        if index < self.history.len() {
            self.history.remove(index);
            self.dirty = true;
            self.ranked_built = false;
        }
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
        self.dirty = true;
        self.ranked_built = false;
    }

    /// The sites visited most, newest URL each, best first.
    ///
    /// Grouped by host: twenty visits to twenty pages of one site is one site
    /// worth a tile, not twenty. Recomputed only when the history has moved.
    pub fn top_sites(&mut self, limit: usize) -> &[Site] {
        if !self.ranked_built {
            self.ranked_built = true;
            let mut order: Vec<String> = Vec::new();
            let mut by_host: HashMap<String, Site> = HashMap::new();
            for visit in &self.history {
                let Some(host) = host_of(&visit.url) else {
                    continue;
                };
                match by_host.get_mut(&host) {
                    Some(site) => {
                        site.visits += 1;
                        // Later visits win: the newest URL is the live one, and
                        // the title arrives after the URL does.
                        site.url = visit.url.clone();
                        if !visit.title.is_empty() {
                            site.title = visit.title.clone();
                        }
                    },
                    None => {
                        order.push(host.clone());
                        by_host.insert(
                            host.clone(),
                            Site {
                                host,
                                url: visit.url.clone(),
                                title: visit.title.clone(),
                                visits: 1,
                            },
                        );
                    },
                }
            }
            let mut ranked: Vec<Site> = order
                .into_iter()
                .filter_map(|host| by_host.remove(&host))
                .collect();
            // A stable sort, so a tie keeps the order the history put them in:
            // the site seen more recently comes first.
            ranked.sort_by_key(|site| std::cmp::Reverse(site.visits));
            self.ranked = ranked;
        }
        &self.ranked[..limit.min(self.ranked.len())]
    }

    /// The last pages visited, newest first, one row per URL.
    pub fn recent(&self, limit: usize) -> Vec<&Visit> {
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let mut out = Vec::new();
        for visit in self.history.iter().rev() {
            if seen.insert(visit.url.as_str()) {
                out.push(visit);
            }
            if out.len() >= limit {
                break;
            }
        }
        out
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

    // ── The new tab page's own scraps

    /// The scratchpad, to edit in place.
    ///
    /// Pair it with [`Library::changed`] when an edit lands: a text field hands
    /// back a borrow rather than an event, so nothing here can tell on its own
    /// that the string moved.
    pub fn note_mut(&mut self) -> &mut String {
        &mut self.note
    }

    /// Something was edited through a borrow; write the file out.
    pub fn changed(&mut self) {
        self.dirty = true;
    }

    pub fn add_task(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        self.tasks.push(Task {
            text: text.to_owned(),
            done: false,
        });
        self.dirty = true;
    }

    pub fn toggle_task(&mut self, index: usize) {
        if let Some(task) = self.tasks.get_mut(index) {
            task.done = !task.done;
            self.dirty = true;
        }
    }

    pub fn remove_task(&mut self, index: usize) {
        if index < self.tasks.len() {
            self.tasks.remove(index);
            self.dirty = true;
        }
    }
}

/// The host part of a URL, lowercased and without a leading `www.` — what
/// makes two addresses read as the same site.
fn host_of(url: &str) -> Option<String> {
    let host = url::Url::parse(url).ok()?.host_str()?.to_lowercase();
    Some(host.strip_prefix("www.").unwrap_or(&host).to_owned())
}

fn path() -> Option<std::path::PathBuf> {
    Some(crate::settings::data_dir()?.join("library.json"))
}
