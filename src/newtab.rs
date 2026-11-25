//! New Tab backdrop, navigation, and saved widget configuration.

mod home;
pub(crate) mod layout;
mod widgets;

use crate::l10n::text as tr;
use egui::{Align2, Color32, CornerRadius, CursorIcon, FontId, Id, Rect, Sense, Ui, pos2, vec2};
use serde::{Deserialize, Serialize};

use crate::glass;
use crate::grid::{Cell, Span};
use crate::icons::{self, Icon};
use crate::theme::{self, Palette, Translucency};
use crate::ui::{ChromeContext, UiAction};

const PAGE_PAD: f32 = 26.0;
const CANVAS_MAX: f32 = 1100.0;
const HEADER: f32 = 46.0;
const CREDIT: f32 = 20.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Widget {
    Clock,
    QuickLinks,
    Recent,
    Writing,
    WorldClocks,
    PinnedTabs,
    MostVisited,
    Downloads,
    NowPlaying,
    Spaces,
}

impl Widget {
    pub const DEFAULT: [Self; 4] = [Self::Clock, Self::QuickLinks, Self::Recent, Self::Writing];
    const ALL: [Self; 10] = [
        Self::Clock,
        Self::QuickLinks,
        Self::Recent,
        Self::Writing,
        Self::WorldClocks,
        Self::PinnedTabs,
        Self::MostVisited,
        Self::Downloads,
        Self::NowPlaying,
        Self::Spaces,
    ];

    fn label(self) -> &'static str {
        tr(match self {
            Self::Clock => "newtab-clock",
            Self::QuickLinks => "newtab-quick-links",
            Self::Recent => "newtab-recent",
            Self::Writing => "newtab-notes-and-tasks",
            Self::WorldClocks => "common-world-clocks",
            Self::PinnedTabs => "sidebar-pinned-tabs",
            Self::MostVisited => "common-most-visited",
            Self::Downloads => "common-downloads",
            Self::NowPlaying => "common-now-playing",
            Self::Spaces => "newtab-workspaces",
        })
    }
}

#[derive(Clone, Deserialize)]
pub(crate) struct LegacyTile {
    card: String,
    at: Cell,
    span: Span,
}

pub(crate) fn migrate_widgets(mut tiles: Vec<LegacyTile>) -> Vec<Widget> {
    let defaults: [&[(&str, u8, u8, u8, u8)]; 2] = [
        &[
            ("Clock", 0, 0, 3, 2),
            ("WorldClocks", 6, 0, 6, 2),
            ("Search", 0, 4, 12, 1),
        ],
        &[
            ("Clock", 4, 1, 4, 2),
            ("Search", 2, 3, 8, 1),
            ("Recent", 2, 5, 4, 3),
            ("TopSites", 6, 5, 4, 3),
        ],
    ];
    if defaults.iter().any(|layout| {
        tiles.len() == layout.len()
            && layout.iter().all(|&(card, col, row, w, h)| {
                tiles.iter().any(|tile| {
                    tile.card == card
                        && tile.at == Cell::new(col, row)
                        && tile.span == Span::new(w, h)
                })
            })
    }) {
        return Widget::DEFAULT.to_vec();
    }
    tiles.sort_by_key(|tile| (tile.at.row, tile.at.col));
    let mut widgets = Vec::new();
    for tile in tiles {
        let widget = match tile.card.as_str() {
            "Clock" => Widget::Clock,
            "QuickLinks" => Widget::PinnedTabs,
            "Favourites" => Widget::QuickLinks,
            "TopSites" => Widget::MostVisited,
            "Recent" => Widget::Recent,
            "Notes" | "Tasks" => Widget::Writing,
            "WorldClocks" => Widget::WorldClocks,
            "Downloads" => Widget::Downloads,
            "NowPlaying" => Widget::NowPlaying,
            "Workspaces" => Widget::Spaces,
            _ => continue,
        };
        if !widgets.contains(&widget) {
            widgets.push(widget);
        }
    }
    widgets
}

/// A clock face on the world-clocks card.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Zone {
    /// What to call it. The IANA name is `Europe/Belgrade`; people say
    /// Belgrade.
    pub label: String,
    /// An IANA zone name, resolved through the compiled-in table.
    pub name: String,
}

impl Zone {
    fn new(label: &str, name: &str) -> Self {
        Zone {
            label: label.to_owned(),
            name: name.to_owned(),
        }
    }

    pub fn defaults() -> Vec<Zone> {
        vec![
            Zone::new("London", "Europe/London"),
            Zone::new("New York", "America/New_York"),
            Zone::new("Tokyo", "Asia/Tokyo"),
        ]
    }

    pub fn display_label(&self) -> &str {
        Self::CATALOGUE
            .iter()
            .find(|(label, name, _)| self.name == *name && self.label == *label)
            .map_or(self.label.as_str(), |(_, _, key)| tr(key))
    }

    /// The zones offered when adding one. A list somebody can read beats every
    /// zone in the table, which is six hundred of them.
    pub const CATALOGUE: [(&'static str, &'static str, &'static str); 24] = [
        ("London", "Europe/London", "clock-city-london"),
        ("Paris", "Europe/Paris", "clock-city-paris"),
        ("Berlin", "Europe/Berlin", "clock-city-berlin"),
        ("Belgrade", "Europe/Belgrade", "clock-city-belgrade"),
        ("Lisbon", "Europe/Lisbon", "clock-city-lisbon"),
        ("Helsinki", "Europe/Helsinki", "clock-city-helsinki"),
        ("Moscow", "Europe/Moscow", "clock-city-moscow"),
        ("Reykjavík", "Atlantic/Reykjavik", "clock-city-reykjavik"),
        ("New York", "America/New_York", "clock-city-new-york"),
        ("Chicago", "America/Chicago", "clock-city-chicago"),
        ("Denver", "America/Denver", "clock-city-denver"),
        (
            "Los Angeles",
            "America/Los_Angeles",
            "clock-city-los-angeles",
        ),
        ("São Paulo", "America/Sao_Paulo", "clock-city-sao-paulo"),
        (
            "Mexico City",
            "America/Mexico_City",
            "clock-city-mexico-city",
        ),
        ("Lagos", "Africa/Lagos", "clock-city-lagos"),
        ("Nairobi", "Africa/Nairobi", "clock-city-nairobi"),
        (
            "Johannesburg",
            "Africa/Johannesburg",
            "clock-city-johannesburg",
        ),
        ("Dubai", "Asia/Dubai", "clock-city-dubai"),
        ("Mumbai", "Asia/Kolkata", "clock-city-mumbai"),
        ("Singapore", "Asia/Singapore", "clock-city-singapore"),
        ("Shanghai", "Asia/Shanghai", "clock-city-shanghai"),
        ("Tokyo", "Asia/Tokyo", "clock-city-tokyo"),
        ("Sydney", "Australia/Sydney", "clock-city-sydney"),
        ("Auckland", "Pacific/Auckland", "clock-city-auckland"),
    ];
}

/// Draw the whole page. Returns true while something on it is still moving,
/// so the caller can schedule the next frame instead of spinning.
///
/// Two rectangles, not one, because at the last step of the seam they are not
/// the same. `page_rect` is the surface — where the ground, the photograph and
/// the blurred copy every card frosts against are painted — and it reaches
/// under a chrome that is floating over it. `content_rect` is where the page's
/// own furniture is laid out, inset so that none of it is permanently hidden
/// behind that chrome. At every other step the two are the same rectangle.
pub fn draw(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    page_rect: Rect,
    content_rect: Rect,
    actions: &mut Vec<UiAction>,
) -> bool {
    let restore = chrome.palette;
    let material = match chrome.settings.newtab_material {
        _ if chrome.native_glass.reduce_transparency => Translucency::Solid,
        Translucency::LiquidGlass if !chrome.native_glass.window_backdrop => Translucency::Frosted,
        material => material,
    };
    let mut palette = restore.with_translucency(material);
    palette.material.animation = restore.material.animation;
    chrome.palette = palette;

    let native_frost = chrome.native_glass.vibrancy_available && material.is_translucent();
    let ground = if native_frost {
        let veil = match material {
            Translucency::LiquidGlass => 0.38,
            _ if palette.dark => 0.58,
            _ => 0.66,
        };
        theme::page_veil(&palette, veil)
    } else {
        theme::page_base(&palette)
    };
    let base = theme::page_base(&palette);
    let radius = theme::content_corners(&palette);

    // The backdrop goes on egui's background layer so every card, menu and
    // text field lands on top of it without having to be ordered.
    let backdrop = root
        .ctx()
        .layer_painter(egui::LayerId::background())
        .with_clip_rect(page_rect);
    backdrop.rect_filled(page_rect, radius, ground);

    let photo = chrome.settings.new_tab_background == crate::settings::NewTabBackground::Photo;
    let mut ambient = false;
    let mut painted = None;
    if photo {
        painted = paint_photo(root, chrome, &backdrop, page_rect, radius, &mut ambient);
    }
    let over_photo = painted.is_some();

    // Hand the palette the blurred copy of what was just painted. From here
    // until the page is finished, every glass surface inside the content rect
    // frosts itself against it — the cards, the search box, the header pills,
    // and anything anyone adds later — without one of them being told to.
    if let (Some((uv, arrival)), Some(frost)) = (painted, chrome.wallpaper.frost)
        && material.is_translucent()
    {
        chrome.palette = chrome.palette.with_backdrop(Some(theme::Backdrop {
            texture: frost.id(),
            luma: frost.luma(),
            rect: page_rect,
            reach: 0.0,
            uv,
            alpha: arrival,
        }));
    }
    if !over_photo {
        let mut pattern = backdrop.clone();
        if native_frost {
            // Patterns and animations remain visible as a tint on the frost.
            pattern.multiply_opacity(0.52);
        }
        ambient |= crate::ui::paint_newtab_background(
            root,
            &pattern,
            page_rect,
            &palette,
            chrome.settings.new_tab_background,
            base,
        );
    }
    // The background is finished and nothing has been drawn on it yet, which
    // is the only moment a copy of it is worth having. Every kind of backdrop
    // has landed by here — the photograph, the gradients, the animated ones —
    // so a menu opened over this page frosts against whichever one it is,
    // rather than against a photograph or nothing.
    if let Some(capture) = &chrome.capture {
        crate::backdrop::capture_into(&backdrop, page_rect, capture);
    }

    let ink = Ink::new(&chrome.palette, over_photo);

    if content_rect.width() < 300.0 || content_rect.height() < 180.0 {
        home::compact(root, chrome, content_rect, actions);
        chrome.palette = restore;
        return ambient;
    }

    // ── The page's own margins, and a ceiling on how wide the grid gets.
    //
    let inner = content_rect.shrink2(vec2(PAGE_PAD, PAGE_PAD * 0.8));
    let width = inner.width().min(CANVAS_MAX);
    let canvas = Rect::from_center_size(
        pos2(inner.center().x, inner.center().y),
        vec2(width, inner.height()),
    );

    let header = Rect::from_min_size(canvas.min, vec2(canvas.width(), HEADER));
    let credit_height = if over_photo { CREDIT } else { 0.0 };
    let dashboard = Rect::from_min_max(
        pos2(canvas.min.x, header.max.y + 8.0),
        pos2(canvas.max.x, canvas.max.y - credit_height),
    );

    draw_header(root, chrome, header, &ink, actions);
    home::draw(root, chrome, dashboard, actions);

    if over_photo {
        draw_credit(
            root,
            chrome,
            Rect::from_min_max(pos2(canvas.min.x, dashboard.max.y), canvas.max),
            &ink,
            actions,
        );
    }

    // Whatever is drawn after the page — a dialog the page raised, the content
    // card's own edges — is not on the wallpaper and must not be frosted
    // against it.
    chrome.palette = restore;
    ambient
}

/// Everything the page needs to know about drawing on what is behind it.
///
/// Over a photograph, text is white and carries a shadow; over the chrome's
/// own base it is the palette's. Working that out once and passing it down
/// beats every card deciding for itself and two of them getting it wrong.
#[derive(Clone)]
struct Ink {
    text: Color32,
    muted: Color32,
    /// What the halo is drawn in — the opposite of the text, or it does
    /// nothing.
    halo: Color32,
    /// True when a photograph is behind everything.
    photo: bool,
}

impl Ink {
    fn new(palette: &Palette, photo: bool) -> Self {
        Ink {
            text: if photo {
                Color32::from_white_alpha(240)
            } else if palette.dark {
                Color32::from_white_alpha(228)
            } else {
                palette.text
            },
            muted: if photo {
                Color32::from_white_alpha(180)
            } else {
                palette.text_muted
            },
            halo: Color32::from_black_alpha(58),
            photo,
        }
    }

    /// This ink, chosen for what is behind `rect` rather than for the page as
    /// a whole.
    ///
    /// One decision for the whole page cannot be right: a photograph is dark
    /// sky at the top and bright water at the bottom, and pale text set for
    /// the sky disappears into the water. The halo helps and is not enough on
    /// its own — it lifts text off a busy picture, not off one the same
    /// brightness as the text.
    fn over(&self, palette: &Palette, rect: Rect, on_glass: bool) -> Ink {
        let Some(light) = palette.prefers_light_ink(rect, on_glass) else {
            return self.clone();
        };
        let (text, muted, halo) = if light {
            (
                Color32::from_white_alpha(240),
                Color32::from_white_alpha(180),
                Color32::from_black_alpha(58),
            )
        } else {
            (
                Color32::from_black_alpha(232),
                Color32::from_black_alpha(160),
                Color32::from_white_alpha(96),
            )
        };
        Ink {
            text,
            muted,
            halo,
            photo: self.photo,
        }
    }

    /// Draw text, lifted off a photograph by a halo when there is one.
    ///
    /// A white caption on a picture that turns out to be a beach in sunlight
    /// is unreadable otherwise, and no veil strong enough to fix that leaves
    /// the picture worth having. A single offset shadow is not enough either:
    /// at one pixel it vanishes under a sixty-point clock, and at the size
    /// that clock needs it stops reading as a shadow and starts reading as a
    /// second, blurry clock. Four diagonal passes scaled to the type size make
    /// a halo instead, which is what the text actually wants — and there are
    /// only ever a handful of strings on this page drawn outside a card.
    fn write(
        &self,
        painter: &egui::Painter,
        pos: egui::Pos2,
        align: Align2,
        text: impl ToString,
        font: FontId,
        color: Color32,
    ) {
        let text = text.to_string();
        if self.photo {
            let spread = (font.size * 0.045).clamp(1.0, 2.5);
            // Eight points rather than four, each fainter. Four corners at
            // full strength stack into a hard ring that reads as an outline
            // around the letters; eight overlapping at a third of the alpha
            // build up near the glyph and thin out from it, which is a halo.
            let halo = self.halo;
            const COMPASS: [(f32, f32); 8] = [
                (0.0, -1.0),
                (0.7, -0.7),
                (1.0, 0.0),
                (0.7, 0.7),
                (0.0, 1.0),
                (-0.7, 0.7),
                (-1.0, 0.0),
                (-0.7, -0.7),
            ];
            for (x, y) in COMPASS {
                painter.text(
                    pos + vec2(x * spread, y * spread),
                    align,
                    &text,
                    font.clone(),
                    halo,
                );
            }
        }
        painter.text(pos, align, text, font, color);
    }
}

/// Paint the photograph and the veil over it. Returns true when there is
/// actually a picture up — a source that has not answered yet leaves the
/// ordinary backdrop showing rather than a black rectangle.
fn paint_photo(
    root: &Ui,
    chrome: &ChromeContext,
    backdrop: &egui::Painter,
    content_rect: Rect,
    radius: CornerRadius,
    ambient: &mut bool,
) -> Option<(Rect, f32)> {
    let texture = chrome.wallpaper.texture?;
    let palette = chrome.palette;
    // Fade in, keyed on the texture: a new picture appearing all at once is
    // the one moment this page draws attention to itself.
    // Fading in, keyed on a *stable* id with the texture as the trigger.
    // Keying the animation on `texture.id()` reads better and does nothing at
    // all: epaint never reuses a texture id, and egui hands back the target
    // value immediately for an id it has not seen before — so every wallpaper
    // arrived at full strength on its first frame and none of them ever faded.
    let seen = Id::unique("zervo_newtab_photo");
    let ramp = seen.with("fade");
    let ctx = root.ctx();
    if ctx.data(|data| data.get_temp::<egui::TextureId>(seen)) != Some(texture.id()) {
        ctx.data_mut(|data| data.insert_temp(seen, texture.id()));
        ctx.animate_value_with_time(ramp, 0.0, 0.0);
    }
    let fade = glass::ease_out(ctx.animate_value_with_time(ramp, 1.0, 0.5));
    // Asked for before the early return, not after. On the first frame of a
    // new picture the ramp is at exactly zero, and returning without setting
    // this meant nothing scheduled the repaint that would have advanced it —
    // so the fade did not merely fail to run, the wallpaper never appeared at
    // all. A frame that draws nothing still has to ask for the next one.
    *ambient |= fade < 1.0;

    let uv = cover(texture.resolution_vec2(), content_rect);
    let (background, opacity) = match (palette.translucency, chrome.wallpaper.frost) {
        (Translucency::Frosted, Some(frost)) => (frost.id(), 0.88),
        (Translucency::LiquidGlass, Some(frost)) => (frost.id(), 0.78),
        _ => (texture.id(), 1.0),
    };
    backdrop.add(
        egui::epaint::RectShape::filled(
            content_rect,
            radius,
            Color32::WHITE.gamma_multiply(fade * opacity),
        )
        .with_texture(background, uv),
    );
    // The veil: a light wash over the whole picture, and a heavier scrim at
    // the top and bottom edges.
    //
    // One flat gradient was the first attempt and it does not work. The two
    // things on this page not drawn inside a card — the header controls and
    // the credit line — sit exactly at those two edges, and a veil strong
    // enough to carry white text there is strong enough to throw the whole
    // photograph away. Darkening only the strips that hold text keeps the
    // middle of the picture, which is the part worth having.
    // Clamped to the same floor the slider offers rather than to zero: the
    // settings file is a text file anyone can edit, and a card on a bright
    // photograph with no veil under it cannot be read at all.
    let dim = chrome.settings.wallpaper_dim.clamp(theme::MIN_VEIL, 1.0) * fade;
    backdrop.rect_filled(content_rect, radius, theme::page_veil(&palette, dim * 0.5));
    let scrim = |height: f32, strength: f32, from_top: bool| {
        let band = if from_top {
            Rect::from_min_max(
                content_rect.min,
                pos2(content_rect.max.x, content_rect.min.y + height),
            )
        } else {
            Rect::from_min_max(
                pos2(content_rect.min.x, content_rect.max.y - height),
                content_rect.max,
            )
        };
        let (top, bottom) = if from_top {
            (theme::page_veil(&palette, strength), Color32::TRANSPARENT)
        } else {
            (Color32::TRANSPARENT, theme::page_veil(&palette, strength))
        };
        crate::ui::vertical_gradient(backdrop, band, top, bottom);
    };
    scrim(HEADER * 3.4, dim * 0.75, true);
    scrim(CREDIT * 6.0, dim * 0.65, false);
    Some((uv, fade))
}

/// The uv rectangle that makes an image cover `target` without distorting it,
/// cropped evenly on the long axis.
fn cover(image: egui::Vec2, target: Rect) -> Rect {
    let image_aspect = (image.x / image.y.max(1.0)).max(0.001);
    let target_aspect = (target.width() / target.height().max(1.0)).max(0.001);
    if image_aspect > target_aspect {
        let fraction = target_aspect / image_aspect;
        let offset = (1.0 - fraction) * 0.5;
        Rect::from_min_max(pos2(offset, 0.0), pos2(offset + fraction, 1.0))
    } else {
        let fraction = image_aspect / target_aspect;
        let offset = (1.0 - fraction) * 0.5;
        Rect::from_min_max(pos2(0.0, offset), pos2(1.0, offset + fraction))
    }
}

// ── Header ─────────────────────────────────────────────────────────────────

fn editing(chrome: &ChromeContext) -> bool {
    chrome
        .browser
        .newtab_editing
        .is_some_and(|tab| chrome.browser.active_tab == Some(tab))
}

fn draw_header(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    header: Rect,
    ink: &Ink,
    actions: &mut Vec<UiAction>,
) {
    let palette = chrome.palette;
    // On the photograph, not on a card, so it follows the photograph — the
    // same rule the cards use, applied to the two strings that are not in one.
    let header_ink = ink.over(&palette, header, false);
    let ink = &header_ink;
    let mark = Rect::from_center_size(
        pos2(header.left() + 15.0, header.center().y),
        vec2(30.0, 30.0),
    );
    crate::ui::paint_application_icon(root, mark, palette.dark);
    if header.width() >= 450.0 {
        ink.write(
            root.painter(),
            pos2(mark.right() + 10.0, header.center().y),
            Align2::LEFT_CENTER,
            zervo_core::APP_NAME,
            FontId::proportional(18.0),
            ink.text,
        );
    }

    let editing = editing(chrome);
    let mut cursor = header.right();
    let customise = header_button(
        root,
        &palette,
        &mut cursor,
        header,
        if editing { Icon::Check } else { Icon::Sliders },
        if editing {
            tr("common-done")
        } else {
            tr("newtab-customise")
        },
        editing,
    );
    if customise.clicked() {
        chrome.browser.newtab_editing = if editing {
            None
        } else {
            chrome.browser.active_tab
        };
    }
    let backdrop = header_button(
        root,
        &palette,
        &mut cursor,
        header,
        Icon::Mountains,
        tr("newtab-backdrop"),
        false,
    );
    draw_backdrop_menu(root, chrome, backdrop.rect, &backdrop, actions);
}

/// One control in the header: a glyph and a word on a small pill. Advances
/// `cursor` leftward by its own width.
///
/// The pill is always there, not only on hover. These four sit at the top of
/// whatever photograph happens to be up, and a bare white word on a bright sky
/// — or a dark one on a dark photograph — is a control nobody can find. Giving
/// them the same material as the cards costs a little ink and means the header
/// reads the same on every wallpaper and in both themes.
fn header_button(
    root: &mut Ui,
    palette: &Palette,
    cursor: &mut f32,
    header: Rect,
    icon: Icon,
    label: &str,
    lit: bool,
) -> egui::Response {
    let font = FontId::proportional(12.5);
    let text_width = root
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), Color32::WHITE)
        .rect
        .width();
    let width = text_width + 42.0;
    let rect = Rect::from_min_size(
        pos2(*cursor - width, header.center().y - 14.0),
        vec2(width, 28.0),
    );
    *cursor -= width + 6.0;

    let response = root.interact(
        rect,
        Id::unique("zervo_newtab_header").with(label),
        Sense::click(),
    );
    let color = crate::widgets::button_ink(root, &response, rect, palette, lit);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    let painter = root.painter();
    icons::draw_icon(
        painter,
        Rect::from_center_size(pos2(rect.min.x + 15.0, rect.center().y), vec2(14.0, 14.0)),
        icon,
        color,
    );
    painter.text(
        pos2(rect.min.x + 27.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        font,
        color,
    );
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// Everything that can go behind the page, in one menu.
///
/// It lives here rather than in Settings because it is a decision about *this*
/// page, taken while looking at it. Choosing a backdrop from another window
/// and coming back to see what it did is two steps and a memory test.
fn draw_backdrop_menu(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    anchor: Rect,
    trigger: &egui::Response,
    actions: &mut Vec<UiAction>,
) {
    use crate::dashboard::Row;
    use crate::settings::NewTabBackground;
    use crate::wallpaper::{Source, Subject};

    let palette = chrome.palette;
    let menu_id = Id::unique("zervo_newtab_backdrop_open");
    if trigger.clicked() {
        let open = root.ctx().data(|data| data.get_temp::<bool>(menu_id)) == Some(true);
        root.ctx().data_mut(|data| data.insert_temp(menu_id, !open));
    }
    if root.ctx().data(|data| data.get_temp::<bool>(menu_id)) != Some(true) {
        return;
    }

    #[derive(Clone, Copy, PartialEq)]
    enum Pick {
        Material(Translucency),
        Generated(NewTabBackground),
        Commons,
        Subject(Subject),
        File,
        Shuffle,
    }

    let background = chrome.settings.new_tab_background;
    let photo = background == NewTabBackground::Photo;
    let source = &chrome.settings.wallpaper_source;
    let generated = |kind: NewTabBackground| {
        Row::Item(
            kind.label().to_owned(),
            Pick::Generated(kind),
            background == kind,
        )
    };

    // Grouped off the variants themselves rather than off a list written out
    // here, so a backdrop added later turns up in the right group without
    // anyone remembering to add it to two places.
    let mut rows = vec![Row::Heading(tr("settings-material"))];
    rows.extend(Translucency::ALL.map(|material| {
        Row::Item(
            material.label().to_owned(),
            Pick::Material(material),
            chrome.settings.newtab_material == material,
        )
    }));
    rows.push(Row::Heading(tr("newtab-static")));
    rows.extend(
        NewTabBackground::ALL
            .into_iter()
            .filter(|kind| !kind.animated() && *kind != NewTabBackground::Photo)
            .map(generated),
    );
    rows.push(Row::Heading(tr("newtab-animated")));
    rows.extend(
        NewTabBackground::ALL
            .into_iter()
            .filter(super::settings::NewTabBackground::animated)
            .map(generated),
    );
    rows.push(Row::Heading(tr("newtab-wallpaper-provider")));
    rows.push(Row::Item(
        tr("newtab-commons-picture-of-the-day").to_owned(),
        Pick::Commons,
        photo && *source == Source::Commons,
    ));
    rows.extend(Subject::ALL.iter().map(|subject| {
        Row::Item(
            Source::Openverse(*subject).label(),
            Pick::Subject(*subject),
            photo && *source == Source::Openverse(*subject),
        )
    }));
    rows.push(Row::Item(
        tr("common-choose-file").to_owned(),
        Pick::File,
        photo && matches!(source, Source::File(_)),
    ));
    if photo {
        rows.push(Row::Heading(tr("newtab-now")));
        rows.push(Row::Item(
            tr("common-another-picture").to_owned(),
            Pick::Shuffle,
            false,
        ));
    }

    match crate::dashboard::menu(
        root,
        &palette,
        "zervo_newtab_backdrop_menu",
        anchor,
        260.0,
        &rows,
    ) {
        Some(pick) => {
            root.ctx().data_mut(|data| data.insert_temp(menu_id, false));
            root.ctx().request_repaint();
            match pick {
                Pick::Material(material) => {
                    chrome.settings.newtab_material = material;
                    actions.push(UiAction::PersistSettings);
                },
                Pick::Generated(kind) => {
                    chrome.settings.new_tab_background = kind;
                    actions.push(UiAction::PersistSettings);
                },
                Pick::Commons => {
                    chrome.settings.wallpaper_source = Source::Commons;
                    actions.push(UiAction::ShuffleWallpaper);
                },
                Pick::Subject(subject) => {
                    chrome.settings.wallpaper_source = Source::Openverse(subject);
                    actions.push(UiAction::ShuffleWallpaper);
                },
                Pick::File => actions.push(UiAction::PickWallpaper),
                Pick::Shuffle => actions.push(UiAction::ShuffleWallpaper),
            }
        },
        None if root.input(|input| input.pointer.any_pressed())
            && !crate::dashboard::over_menu(root.ctx())
            && !trigger.clicked() =>
        {
            root.ctx().data_mut(|data| data.insert_temp(menu_id, false));
        },
        None => {},
    }
}

/// The line under a photograph saying whose it is. Most of what comes back is
/// CC BY or CC BY-SA, so this is a licence condition rather than a courtesy,
/// and it is not something the settings can turn off.
fn draw_credit(
    root: &mut Ui,
    chrome: &ChromeContext,
    rect: Rect,
    ink: &Ink,
    actions: &mut Vec<UiAction>,
) {
    let credit = chrome.wallpaper.credit;
    if credit.title.is_empty() && credit.author.is_empty() {
        return;
    }
    let font = FontId::proportional(10.5);
    let line = credit.line();
    let width = root
        .painter()
        .layout_no_wrap(line.clone(), font.clone(), Color32::WHITE)
        .rect
        .width();
    let hit = Rect::from_min_size(
        pos2(rect.min.x, rect.min.y + 2.0),
        vec2(width + 4.0, CREDIT - 4.0),
    );
    let clickable = !credit.page.is_empty();
    let response = root.interact(
        hit,
        Id::unique("zervo_newtab_credit"),
        if clickable {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let hover = glass::ease_out(root.ctx().animate_bool_with_time(
        response.id.with("hover"),
        clickable && response.hovered(),
        0.12,
    ));
    let credit_ink = ink.over(&chrome.palette, hit, false);
    credit_ink.write(
        root.painter(),
        pos2(hit.min.x, hit.center().y),
        Align2::LEFT_CENTER,
        &line,
        font,
        theme::mix(credit_ink.muted, credit_ink.text, hover),
    );
    if clickable
        && response
            .on_hover_cursor(CursorIcon::PointingHand)
            .on_hover_text(&credit.page)
            .clicked()
    {
        actions.push(UiAction::Navigate(credit.page.clone()));
    }
}

fn weekday(date: &jiff::ZonedDateTime, short: bool) -> &'static str {
    const LONG: [&str; 7] = [
        "clock-weekday-monday",
        "clock-weekday-tuesday",
        "clock-weekday-wednesday",
        "clock-weekday-thursday",
        "clock-weekday-friday",
        "clock-weekday-saturday",
        "clock-weekday-sunday",
    ];
    const SHORT: [&str; 7] = [
        "clock-weekday-monday-short",
        "clock-weekday-tuesday-short",
        "clock-weekday-wednesday-short",
        "clock-weekday-thursday-short",
        "clock-weekday-friday-short",
        "clock-weekday-saturday-short",
        "clock-weekday-sunday-short",
    ];
    let labels = if short { SHORT } else { LONG };
    tr(labels[date.weekday().to_monday_zero_offset() as usize])
}

pub(crate) fn calendar_date(date: &jiff::ZonedDateTime, short: bool) -> String {
    const LONG: [&str; 12] = [
        "clock-month-january",
        "clock-month-february",
        "clock-month-march",
        "clock-month-april",
        "clock-month-may",
        "clock-month-june",
        "clock-month-july",
        "clock-month-august",
        "clock-month-september",
        "clock-month-october",
        "clock-month-november",
        "clock-month-december",
    ];
    const SHORT: [&str; 12] = [
        "clock-month-january-short",
        "clock-month-february-short",
        "clock-month-march-short",
        "clock-month-april-short",
        "clock-month-may-short",
        "clock-month-june-short",
        "clock-month-july-short",
        "clock-month-august-short",
        "clock-month-september-short",
        "clock-month-october-short",
        "clock-month-november-short",
        "clock-month-december-short",
    ];
    let labels = if short { SHORT } else { LONG };
    crate::l10n::message(
        if short {
            "clock-date-short"
        } else {
            "clock-date"
        },
        [
            ("weekday", weekday(date, short).into()),
            ("day", date.day().into()),
            ("month", tr(labels[(date.month() - 1) as usize]).into()),
        ],
    )
}

/// Trim `text` until it fits `width`, saying so with an ellipsis.
///
/// Measured rather than counted: a column of proportional text truncated by
/// character count is ragged, and the ragged edge is the one thing a card
/// full of titles cannot afford.
fn fit(painter: &egui::Painter, text: &str, font: &FontId, width: f32) -> String {
    let measure = |candidate: &str| {
        painter
            .layout_no_wrap(candidate.to_owned(), font.clone(), Color32::WHITE)
            .rect
            .width()
    };
    let full = measure(text);
    if full <= width || width <= 0.0 {
        return text.to_owned();
    }
    let characters: Vec<char> = text.chars().collect();
    // Start from the proportion that fits and walk back, rather than dropping
    // one character at a time: a long title would otherwise cost a hundred
    // layouts a frame.
    let mut keep = ((characters.len() as f32) * (width / full) * 0.96) as usize;
    for _ in 0..8 {
        if keep == 0 {
            break;
        }
        let candidate: String = characters[..keep.min(characters.len())]
            .iter()
            .collect::<String>()
            .trim_end()
            .to_owned()
            + "…";
        if measure(&candidate) <= width {
            return candidate;
        }
        keep = keep.saturating_sub((keep / 8).max(1));
    }
    "…".to_owned()
}
