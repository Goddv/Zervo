//! Text and rectangles, without a `Ui` in sight.
//!
//! Trimming a title to fit, the initial for a favicon that never arrived,
//! keeping a popup inside the window, and turning whatever somebody typed in
//! the address bar into a URL or a search. All of it is arithmetic on strings
//! and rects, so all of it is testable without a window.

use egui::{Pos2, Rect, Vec2, pos2};

use crate::settings::SearchEngine;

/// The host part of a URL, without `www.` — the short name a person would use
/// for a site.
///
/// Here rather than beside one of its callers because there are three now: the
/// new tab page's resume cards, the trouble pages' "did you mean", and the
/// crash handler working out which site went down.
pub fn host_of(url: &str) -> String {
    url.split("://")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or(url)
        .trim_start_matches("www.")
        .to_owned()
}

/// A saved page's name, falling back to its address when it has no title.
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

const POPUP_MARGIN: f32 = 8.0;

fn popup_bounds(bounds: Rect) -> Rect {
    bounds.shrink(POPUP_MARGIN.min(bounds.width().min(bounds.height()).max(0.0) * 0.5))
}

fn fit_popup(origin: Pos2, size: Vec2, bounds: Rect) -> Rect {
    let size = size.max(Vec2::ZERO).min(bounds.size());
    let max_origin = bounds.max - size;
    Rect::from_min_size(origin.clamp(bounds.min, max_origin.max(bounds.min)), size)
}

/// Fit the popup to the available bounds, including oversized menus.
pub fn clamp_into(rect: Rect, bounds: Rect) -> Rect {
    fit_popup(rect.min, rect.size(), popup_bounds(bounds))
}

/// Prefer room below the trigger. Flip above it when that fits, or use the
/// larger side as a viewport when the contents need to scroll.
pub fn anchored_popup(anchor: Rect, size: Vec2, bounds: Rect, centered: bool) -> Rect {
    const GAP: f32 = 6.0;
    let bounds = popup_bounds(bounds);
    let below = (anchor.bottom() + GAP).clamp(bounds.top(), bounds.bottom());
    let above = (anchor.top() - GAP).clamp(bounds.top(), bounds.bottom());
    let below_room = bounds.bottom() - below;
    let above_room = above - bounds.top();
    let flip = size.y > below_room && above_room > below_room;
    let room = if flip { above_room } else { below_room };
    let width = size.x.min(bounds.width());
    let left = if centered {
        anchor.center().x - width * 0.5
    } else {
        anchor.left()
    };
    // An unusually large trigger can leave too little room on either side
    // for a usable viewport. The popup may overlap it in that case.
    if room < size.y.min(80.0) {
        return fit_popup(pos2(left, below), size, bounds);
    }
    let height = size.y.min(room);
    fit_popup(
        pos2(left, if flip { above - height } else { below }),
        egui::vec2(width, height),
        bounds,
    )
}

/// Turn address-bar input into a loadable URL: pass URLs through, prefix bare
/// domains with https://, and send everything else to the chosen search engine.
pub fn normalize_url(input: &str, search_engine: SearchEngine) -> String {
    let input = input.trim();
    if let Some(internal) = input.strip_prefix("zervo://") {
        return format!("heion://{internal}");
    }
    if input.starts_with("http://")
        || input.starts_with("https://")
        || input.starts_with("file://")
        || input.starts_with("about:")
        || input.starts_with("heion://")
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

#[cfg(test)]
mod popup_tests {
    use super::*;

    #[test]
    fn the_popup_flips_above_a_trigger_near_the_bottom() {
        let window = Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0));
        let trigger = Rect::from_min_size(pos2(500.0, 351.0), egui::vec2(65.0, 28.0));
        let popup = anchored_popup(trigger, egui::vec2(240.0, 140.0), window, false);
        assert_eq!(popup.bottom(), trigger.top() - 6.0);
        assert_eq!(popup.right(), window.right() - POPUP_MARGIN);
    }

    #[test]
    fn oversized_menus_fit_a_short_window_even_with_a_large_trigger() {
        let window = Rect::from_min_size(Pos2::ZERO, egui::vec2(300.0, 200.0));
        let trigger = window.shrink(25.0);
        let menu = anchored_popup(trigger, egui::vec2(800.0, 600.0), window, false);
        assert_eq!(menu, window.shrink(POPUP_MARGIN));
    }
}
