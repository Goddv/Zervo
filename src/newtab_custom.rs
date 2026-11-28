//! Data and placement for user-created New Tab cards.

use serde::{Deserialize, Serialize};
use url::Url;

use crate::grid::{Cell, Span};
use crate::library::Task;
use crate::newtab_widgets::{self as widgets, Kind, Widget};
use crate::settings::Settings;
use crate::widget_photo::Photo;

const MAX_BLOCKS: usize = 10;
const MAX_ROWS: usize = 8;
pub const COUNTER_LIMIT: i32 = 100_000;

#[derive(Clone, Serialize, Deserialize)]
pub struct CustomWidget {
    pub id: u64,
    pub title: String,
    pub cell: Cell,
    pub span: Span,
    #[serde(rename = "content", deserialize_with = "read_blocks")]
    pub blocks: Vec<Block>,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum Block {
    Text(String),
    Bullets(Vec<String>),
    Checklist(Vec<Task>),
    Note {
        label: String,
        text: String,
    },
    Links(Vec<Link>),
    Image(Photo),
    Metric {
        label: String,
        text: String,
    },
    Counter {
        label: String,
        value: i32,
        goal: Option<i32>,
        step: i32,
    },
    Timer {
        label: String,
        minutes: u16,
        ends_at: Option<i64>,
    },
    Date {
        label: String,
        date: String,
    },
    Clock {
        #[serde(default)]
        label: String,
        #[serde(default)]
        zone: String,
    },
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Link {
    pub label: String,
    pub url: String,
}

fn read_blocks<'de, D: serde::Deserializer<'de>>(decoder: D) -> Result<Vec<Block>, D::Error> {
    #[derive(Deserialize)]
    enum Legacy {
        Checklist(Vec<Task>),
        Countdown { minutes: u16, ends_at: Option<i64> },
        QuickLinks(Vec<Link>),
    }
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Saved {
        Blocks(Vec<Block>),
        Legacy(Legacy),
    }

    Ok(match Saved::deserialize(decoder)? {
        Saved::Blocks(blocks) => blocks,
        Saved::Legacy(legacy) => vec![match legacy {
            Legacy::Checklist(items) => Block::Checklist(items),
            Legacy::Countdown { minutes, ends_at } => Block::Timer {
                label: String::new(),
                minutes,
                ends_at,
            },
            Legacy::QuickLinks(links) => Block::Links(links),
        }],
    })
}

#[cfg(any(test, all(target_os = "macos", target_arch = "aarch64")))]
#[derive(Deserialize)]
struct Draft {
    title: String,
    size: Option<DraftSize>,
    blocks: Vec<DraftBlock>,
}

#[cfg(any(test, all(target_os = "macos", target_arch = "aarch64")))]
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum DraftBlock {
    Text {
        text: String,
    },
    Bullets {
        items: Vec<String>,
    },
    Checklist {
        items: Vec<String>,
    },
    Note {
        label: String,
        text: Option<String>,
    },
    Links {
        links: Vec<Link>,
    },
    Image {
        label: Option<String>,
        text: String,
    },
    Metric {
        label: String,
        text: String,
    },
    Counter {
        label: String,
        value: Option<i32>,
        goal: Option<i32>,
        step: Option<i32>,
    },
    Timer {
        label: Option<String>,
        value: u16,
    },
    Date {
        label: String,
        text: String,
    },
    Clock {
        label: Option<String>,
        text: Option<String>,
    },
}

#[cfg(any(test, all(target_os = "macos", target_arch = "aarch64")))]
impl From<DraftBlock> for Block {
    fn from(block: DraftBlock) -> Self {
        match block {
            DraftBlock::Text { text } => Self::Text(text),
            DraftBlock::Bullets { items } => Self::Bullets(items),
            DraftBlock::Checklist { items } => Self::Checklist(
                items
                    .into_iter()
                    .map(|text| Task { text, done: false })
                    .collect(),
            ),
            DraftBlock::Note { label, text } => Self::Note {
                label,
                text: text.unwrap_or_default(),
            },
            DraftBlock::Links { links } => Self::Links(links),
            DraftBlock::Image { label, text } => Self::Image(Photo {
                title: label.unwrap_or_default(),
                source: text,
                asset: None,
            }),
            DraftBlock::Metric { label, text } => Self::Metric { label, text },
            DraftBlock::Counter {
                label,
                value,
                goal,
                step,
            } => Self::Counter {
                label,
                value: value.unwrap_or(0),
                goal,
                step: step.unwrap_or(1),
            },
            DraftBlock::Timer { label, value } => Self::Timer {
                label: label.unwrap_or_default(),
                minutes: value,
                ends_at: None,
            },
            DraftBlock::Date { label, text } => Self::Date { label, date: text },
            DraftBlock::Clock { label, text } => Self::Clock {
                label: label.unwrap_or_default(),
                zone: text.unwrap_or_default(),
            },
        }
    }
}

#[cfg(any(test, all(target_os = "macos", target_arch = "aarch64")))]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum DraftSize {
    Auto,
    Small,
    Pill,
    Square,
    Wide,
    Large,
}

fn clean_label(label: &str, limit: usize) -> String {
    label
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(limit)
        .collect()
}

fn safe_link(address: &str) -> Option<Url> {
    if address.len() > 2048 {
        return None;
    }
    let url = Url::parse(address).ok()?;
    (matches!(url.scheme(), "http" | "https")
        && url.host().is_some()
        && url.username().is_empty()
        && url.password().is_none())
    .then_some(url)
}

impl CustomWidget {
    fn tile(&self) -> Widget {
        Widget {
            kind: Kind::Custom(self.id),
            cell: self.cell,
            span: self.span,
        }
    }

    fn clean(&mut self) -> bool {
        self.title = clean_label(&self.title, 64);
        self.span = self.span.fit(widgets::COLUMNS, widgets::MAX_HEIGHT);
        self.blocks.truncate(MAX_BLOCKS);
        self.blocks.retain_mut(Block::clean);
        !self.title.is_empty() && !self.blocks.is_empty()
    }
}

fn limit_text(text: &mut String, max_chars: usize) {
    if let Some((end, _)) = text.char_indices().nth(max_chars) {
        text.truncate(end);
    }
}

impl Block {
    pub fn limit_note(text: &mut String) {
        limit_text(text, 500);
    }

    fn clean(&mut self) -> bool {
        match self {
            Self::Image(photo) => photo.clean(),
            Self::Text(text) => {
                limit_text(text, 500);
                !text.trim().is_empty()
            },
            Self::Bullets(rows) => {
                rows.truncate(MAX_ROWS);
                rows.retain_mut(|row| {
                    *row = clean_label(row, 160);
                    !row.is_empty()
                });
                !rows.is_empty()
            },
            Self::Checklist(items) => {
                items.truncate(MAX_ROWS);
                items.retain_mut(|item| {
                    item.text = clean_label(&item.text, 160);
                    !item.text.is_empty()
                });
                !items.is_empty()
            },
            Self::Note { label, text } => {
                *label = clean_label(label, 64);
                Self::limit_note(text);
                !label.is_empty()
            },
            Self::Links(links) => {
                links.truncate(MAX_ROWS);
                links.retain_mut(|link| {
                    let Some(url) = safe_link(&link.url) else {
                        return false;
                    };
                    link.label = clean_label(&link.label, 64);
                    if link.label.is_empty() {
                        link.label = url.host_str().unwrap_or_default().to_owned();
                    }
                    link.url = url.into();
                    true
                });
                !links.is_empty()
            },
            Self::Metric { label, text } => {
                *label = clean_label(label, 64);
                *text = clean_label(text, 80);
                !label.is_empty() && !text.is_empty()
            },
            Self::Counter {
                label,
                value,
                goal,
                step,
            } => {
                *label = clean_label(label, 64);
                *value = (*value).clamp(-COUNTER_LIMIT, COUNTER_LIMIT);
                *goal = goal.filter(|goal| (1..=COUNTER_LIMIT).contains(goal));
                *step = (*step).clamp(1, COUNTER_LIMIT);
                !label.is_empty()
            },
            Self::Timer { label, minutes, .. } => {
                *label = clean_label(label, 64);
                (1..=1440).contains(minutes)
            },
            Self::Date { label, date } => {
                *label = clean_label(label, 64);
                date.parse::<jiff::civil::Date>().is_ok()
            },
            Self::Clock { label, zone } => {
                *label = clean_label(label, 64);
                limit_text(zone, 100);
                if let Some(resolved) = resolve_clock_zone(zone, label) {
                    *zone = resolved;
                }
                true
            },
        }
    }
}

/// A generated clock may have a city label even if the model omitted its zone.
fn resolve_clock_zone(zone: &str, label: &str) -> Option<String> {
    use jiff::tz::TimeZone;

    let mut names = [zone.trim(), label.trim()].into_iter();
    names
        .find_map(|name| {
            let key: String = name
                .chars()
                .filter(|ch| ch.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect();
            let alias = match key.as_str() {
                "local" | "localtime" | "system" | "mytime" => return Some("local".into()),
                "" => return None,
                "nyc" => Some("America/New_York"),
                "kiev" | "київ" | "киев" => Some("Europe/Kyiv"),
                "la" => Some("America/Los_Angeles"),
                "delhi" | "newdelhi" | "mumbai" => Some("Asia/Kolkata"),
                "beijing" => Some("Asia/Shanghai"),
                "sfo" => Some("America/Los_Angeles"),
                "gmt" => Some("UTC"),
                _ => None,
            };
            if let Some(alias) = alias {
                return Some(alias.to_owned());
            }
            if let Ok(zone) = TimeZone::get(name) {
                return Some(zone.iana_name().unwrap_or("UTC").to_owned());
            }
            let mut matches = jiff::tz::db().available().filter_map(|candidate| {
                let short = candidate.as_str().rsplit_once('/')?.1;
                let candidate_key = short
                    .chars()
                    .filter(|ch| ch.is_alphanumeric())
                    .flat_map(char::to_lowercase);
                candidate_key
                    .eq(key.chars())
                    .then(|| candidate.as_str().to_owned())
            });
            let found = matches.next()?;
            matches.next().is_none().then_some(found)
        })
        .or_else(|| (zone.trim().is_empty() && label.trim().is_empty()).then(|| "local".into()))
}

pub fn clock_time(zone: &str, now: jiff::Timestamp) -> Option<String> {
    let zone = if zone == "local" {
        jiff::tz::TimeZone::system()
    } else {
        jiff::tz::TimeZone::get(zone).ok()?
    };
    Some(now.to_zoned_datetime(zone).strftime("%H:%M").to_string())
}

/// Pick grid dimensions from the content rather than the widget's description.
pub fn preferred_span(title: &str, blocks: &[Block]) -> Span {
    if let [Block::Image(photo)] = blocks {
        let aspect = photo.asset.as_ref().map_or(1.35, |asset| {
            asset.width as f32 / asset.height.max(1) as f32
        });
        let width = if aspect < 1.05 {
            3
        } else if aspect >= 2.15 {
            5
        } else {
            4
        };
        let rows = ((f32::from(width) * 66.0 / aspect + 60.0) / 77.0).round() as u8;
        return Span::new(width, rows.clamp(2, widgets::MAX_HEIGHT));
    }
    if matches!(
        blocks,
        [Block::Timer { .. }
            | Block::Counter { goal: None, .. }
            | Block::Clock { .. }
            | Block::Metric { .. }]
    ) {
        return Span::new(if title.chars().count() > 17 { 3 } else { 2 }, 1);
    }

    let wide = blocks.len() >= 4
        || blocks.iter().any(|block| match block {
            Block::Text(text) => text.chars().count() > 150,
            Block::Bullets(items) => items.iter().any(|item| item.chars().count() > 55),
            Block::Checklist(items) => items.iter().any(|item| item.text.chars().count() > 55),
            _ => false,
        });
    let width = if wide { 6 } else { 4 };
    let lines = if wide { 55 } else { 34 };
    let rows = |text: &str| {
        text.lines()
            .map(|line| line.chars().count().max(1).div_ceil(lines))
            .sum::<usize>()
    };
    let content: usize = blocks
        .iter()
        .map(|block| match block {
            Block::Text(text) => rows(text) * 20,
            Block::Bullets(items) => items.iter().map(|item| rows(item) * 22).sum(),
            Block::Checklist(items) => items.iter().map(|item| rows(&item.text) * 23).sum(),
            Block::Links(links) => links.len() * 22,
            Block::Note { .. } | Block::Counter { goal: Some(_), .. } => 68,
            Block::Image(_) => 158,
            _ => 42,
        })
        .sum();
    let rows = (content + blocks.len().saturating_sub(1) * 6 + 51).div_ceil(74);
    Span::new(width, rows.clamp(2, usize::from(widgets::MAX_HEIGHT)) as u8)
}

pub fn normalize(custom: &mut Vec<CustomWidget>) {
    let mut seen = std::collections::HashSet::new();
    custom.retain_mut(|card| card.id > 0 && seen.insert(card.id) && card.clean());
}

pub fn tiles(settings: &Settings) -> Vec<Widget> {
    settings
        .new_tab_widgets
        .iter()
        .copied()
        .filter(|tile| !matches!(tile.kind, Kind::Custom(_)))
        .chain(
            settings
                .new_tab_custom_widgets
                .iter()
                .map(CustomWidget::tile),
        )
        .collect()
}

pub fn save_layout(settings: &mut Settings, tiles: &[Widget]) {
    settings.new_tab_widgets = tiles
        .iter()
        .copied()
        .filter(|tile| !matches!(tile.kind, Kind::Custom(_)))
        .collect();
    settings.new_tab_custom_widgets.retain_mut(|card| {
        let Some(tile) = tiles.iter().find(|tile| tile.kind == Kind::Custom(card.id)) else {
            return false;
        };
        card.cell = tile.cell;
        card.span = tile.span;
        true
    });
}

#[cfg(any(test, all(target_os = "macos", target_arch = "aarch64")))]
pub struct Preview {
    pub title: String,
    pub span: Span,
    pub blocks: Vec<Block>,
    pub automatic: bool,
}

#[cfg(any(test, all(target_os = "macos", target_arch = "aarch64")))]
impl Preview {
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let draft: Draft = serde_json::from_slice(bytes).ok()?;
        let requested = match draft.size {
            None | Some(DraftSize::Auto) => None,
            Some(DraftSize::Small) => Some(Span::new(1, 1)),
            Some(DraftSize::Pill) => Some(Span::new(2, 1)),
            Some(DraftSize::Square) => Some(Span::new(2, 2)),
            Some(DraftSize::Wide) => Some(Span::new(4, 2)),
            Some(DraftSize::Large) => Some(Span::new(4, 4)),
        };
        let title = clean_label(&draft.title, 64);
        let blocks: Vec<Block> = draft
            .blocks
            .into_iter()
            .take(MAX_BLOCKS)
            .map(Block::from)
            .filter_map(|mut block| block.clean().then_some(block))
            .collect();
        if title.is_empty() || blocks.is_empty() {
            return None;
        }
        let span = requested.unwrap_or_else(|| preferred_span(&title, &blocks));
        Some(Self {
            title,
            span,
            blocks,
            automatic: requested.is_none(),
        })
    }

    pub fn has_photos(&self) -> bool {
        self.blocks
            .iter()
            .any(|block| matches!(block, Block::Image(_)))
    }

    pub fn resolve_photos(
        &mut self,
        mut used: std::collections::HashSet<String>,
    ) -> Result<(), String> {
        let mut found = 0;
        for photo in self.blocks.iter_mut().filter_map(|block| match block {
            Block::Image(photo) => Some(photo),
            _ => None,
        }) {
            if found >= 3 {
                return Err("A card can load at most three photos".into());
            }
            photo.fetch(&used)?;
            if let Some(asset) = &photo.asset {
                used.insert(asset.source_page.clone());
            }
            found += 1;
        }
        if self.automatic {
            self.span = preferred_span(&self.title, &self.blocks);
        }
        Ok(())
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
impl std::fmt::Debug for Preview {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Preview")
            .field("span", &self.span)
            .finish_non_exhaustive()
    }
}

#[cfg(any(test, all(target_os = "macos", target_arch = "aarch64")))]
pub fn insert(settings: &mut Settings, draft: &Preview) -> Option<u64> {
    let id = (1..).find(|id| {
        settings
            .new_tab_custom_widgets
            .iter()
            .all(|card| card.id != *id)
    })?;
    let mut card = CustomWidget {
        id,
        title: draft.title.clone(),
        cell: Cell::new(0, 0),
        span: draft.span,
        blocks: draft.blocks.clone(),
    };
    let mut layout = widgets::arrange(&tiles(settings), widgets::COLUMNS);
    if !widgets::add_with_size(&mut layout, Kind::Custom(id), card.span, widgets::COLUMNS) {
        return None;
    }
    let tile = layout.last()?;
    card.cell = tile.cell;
    card.span = tile.span;
    settings.new_tab_custom_widgets.push(card);
    Some(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restores_city_zones_when_the_model_saved_only_labels() {
        let card: CustomWidget = serde_json::from_str(
            r#"{
          "id":2,"title":"World clocks","cell":{"col":1,"row":3},"span":{"w":4,"h":3},
          "content":[
            {"Clock":{"label":"Paris","zone":""}},
            {"Clock":{"label":"Kyiv","zone":""}},
            {"Clock":{"label":"NYC"}},
            {"Clock":{"label":"Unknown city","zone":""}}
          ]
        }"#,
        )
        .unwrap();
        let mut cards = vec![card];
        normalize(&mut cards);
        let when = "2026-11-28T01:51:00Z".parse::<jiff::Timestamp>().unwrap();
        let times = cards[0]
            .blocks
            .iter()
            .map(|block| match block {
                Block::Clock { zone, .. } => clock_time(zone, when),
                _ => unreachable!(),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            times,
            [
                Some("02:51".into()),
                Some("03:51".into()),
                Some("20:51".into()),
                None
            ]
        );
    }

    #[test]
    fn automatic_size_distinguishes_a_timer_from_a_dashboard() {
        let timer = Preview::decode(
            br#"{"title":"Focus","size":"auto","blocks":[{"kind":"timer","value":25}]}"#,
        )
        .unwrap();
        let dashboard = Preview::decode(
            br#"{
            "title":"Focus","size":"auto","blocks":[
                {"kind":"timer","value":25},
                {"kind":"checklist","items":["First","Second","Third"]},
                {"kind":"note","label":"Notes"}
            ]
        }"#,
        )
        .unwrap();
        assert_eq!(timer.span, Span::new(2, 1));
        assert!(dashboard.span.w >= 4 && dashboard.span.h > timer.span.h);
    }

    #[test]
    fn two_checklists_keep_their_identity_and_layout_through_save() {
        let mut settings = Settings::default();
        let draft = br#"{"title":"Before I leave","size":"wide","blocks":[{"kind":"checklist","items":["Book the train"]}]}"#;
        let first = insert(&mut settings, &Preview::decode(draft).unwrap()).unwrap();
        let second = insert(&mut settings, &Preview::decode(draft).unwrap()).unwrap();
        assert_ne!(first, second);

        let mut layout = widgets::arrange(&tiles(&settings), widgets::COLUMNS);
        layout.retain(|tile| tile.kind != Kind::Custom(first));
        save_layout(&mut settings, &layout);
        let restored: Settings =
            serde_json::from_slice(&serde_json::to_vec(&settings).unwrap()).unwrap();
        assert_eq!(restored.new_tab_custom_widgets[0].id, second);
        assert_eq!(restored.new_tab_custom_widgets[0].span, Span::new(4, 2));
        assert!(
            !restored
                .new_tab_widgets
                .iter()
                .any(|tile| matches!(tile.kind, Kind::Custom(_)))
        );
    }

    #[test]
    fn composed_widgets_restore_multiple_independent_controls() {
        let draft = br#"{
          "title": "Studio", "size": "large",
          "blocks": [
            {"kind":"counter","label":"Pages","value":3,"goal":12,"step":2},
            {"kind":"note","label":"Ideas","text":"A first draft"},
            {"kind":"clock","label":"Tokyo","text":"Asia/Tokyo"}
          ]
        }"#;
        let mut settings = Settings::default();
        let id = insert(&mut settings, &Preview::decode(draft).unwrap()).unwrap();
        let restored: Settings =
            serde_json::from_slice(&serde_json::to_vec(&settings).unwrap()).unwrap();
        let card = &restored.new_tab_custom_widgets[0];
        assert_eq!(card.id, id);
        assert!(matches!(
            card.blocks.as_slice(),
            [
                Block::Counter { value: 3, goal: Some(12), step: 2, .. },
                Block::Note { text, .. },
                Block::Clock { zone, .. },
            ] if text == "A first draft" && zone == "Asia/Tokyo"
        ));
    }

    #[test]
    fn restores_a_running_timer_from_the_previous_prototype() {
        let card: CustomWidget = serde_json::from_str(
            r#"{
          "id":2,"title":"Focus","cell":{"col":1,"row":3},"span":{"w":2,"h":1},
          "content":{"Countdown":{"minutes":25,"ends_at":4102444800}}
        }"#,
        )
        .unwrap();
        assert!(matches!(
            card.blocks.as_slice(),
            [Block::Timer {
                minutes: 25,
                ends_at: Some(4102444800),
                ..
            }]
        ));
    }

    #[test]
    fn rejects_link_actions_that_cannot_open_a_web_page() {
        let draft = br#"{"title":"Links","size":"pill","blocks":[{"kind":"links","links":[{"label":"script","url":"javascript:alert(1)"}]}]}"#;
        assert!(Preview::decode(draft).is_none());
    }
}
