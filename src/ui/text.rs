//! Text and rectangles, without a `Ui` in sight.
//!
//! Trimming a title to fit, the initial for a favicon that never arrived,
//! keeping a popup inside the window, and turning whatever somebody typed in
//! the address bar into a URL or a search. All of it is arithmetic on strings
//! and rects, so all of it is testable without a window.

use egui::Rect;

use crate::settings::SearchEngine;

/// A saved page's name, falling back to its host when it has no title.
pub fn display_name<'a>(title: &'a str, url: &'a str) -> &'a str {
    if !title.is_empty() {
        return title;
    }
    url.split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or(url)
}

/// Whether a colour is light enough that a mark on it wants to be dark.
///
/// Rec. 601 luma, which is close enough for deciding between black and white
/// and is what everything else that has to make this call uses.
pub(crate) fn color_is_light(color: egui::Color32) -> bool {
    let luma =
        0.299 * f32::from(color.r()) + 0.587 * f32::from(color.g()) + 0.114 * f32::from(color.b());
    luma > 140.0
}

/// Stands in for a favicon, which is not stored anywhere yet.
pub fn initial(title: &str, url: &str) -> String {
    display_name(title, url)
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_owned())
}

/// Shorten to `limit` characters on a character boundary, with an ellipsis.
pub fn ellipsize(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    text.chars()
        .take(limit.saturating_sub(1))
        .collect::<String>()
        + "…"
}

/// Keep a rect inside `bounds`.
pub fn clamp_into(rect: Rect, bounds: Rect) -> Rect {
    let mut min = rect.min;
    min.x = min.x.clamp(
        bounds.min.x + 8.0,
        (bounds.max.x - rect.width() - 8.0).max(bounds.min.x + 8.0),
    );
    min.y = min.y.clamp(
        bounds.min.y + 8.0,
        (bounds.max.y - rect.height() - 8.0).max(bounds.min.y + 8.0),
    );
    Rect::from_min_size(min, rect.size())
}

/// Turn address-bar input into a loadable URL: pass URLs through, prefix bare
/// domains with https://, and send everything else to the chosen search engine.
pub fn normalize_url(input: &str, search_engine: SearchEngine) -> String {
    let input = input.trim();
    if input.starts_with("http://")
        || input.starts_with("https://")
        || input.starts_with("file://")
        || input.starts_with("about:")
        || input.starts_with("zervo://")
    {
        input.to_owned()
    } else if input.starts_with('/') || input.starts_with("~/") {
        // An absolute path is a local file, not a search. Without this a
        // `file://` URL fell through to the branch below and became
        // `https://file:///...`, so local files could not be opened at all.
        let path = if let Some(rest) = input.strip_prefix("~/") {
            match std::env::var("HOME") {
                Ok(home) => format!("{home}/{rest}"),
                Err(_) => input.to_owned(),
            }
        } else {
            input.to_owned()
        };
        format!("file://{path}")
    } else if input.contains('.') && !input.contains(' ') {
        format!("https://{input}")
    } else {
        search_engine.query_url(&url_escape(input))
    }
}

pub(crate) fn url_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            },
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

pub(crate) fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_owned()
    } else {
        let truncated: String = text.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{truncated}…")
    }
}
