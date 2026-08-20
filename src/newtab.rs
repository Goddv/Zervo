//! zervo://newtab — a page of cards the reader arranges.
//!
//! Everything on the page is a card on a twelve-column grid, and every card
//! can be moved, resized or taken off. Nothing is packed: a card stays where
//! it was put, including in a row with nothing above it, because an
//! arrangement that rearranges itself is not an arrangement.
//!
//! Dragging only happens in edit mode. The alternative — a page where a card
//! is always draggable — has to guess whether a press on a link meant *open
//! that* or *pick this up*, and it guesses wrong often enough to be worse than
//! a button. So the cards are inert until Customise is pressed, and then they
//! are nothing but handles.
//!
//! What the cards show is what the browser already knows: pinned tabs, the
//! sites visited most, favourites, downloads, what a page is playing. The two
//! that hold their own material — a note and a to-do list — keep it in the
//! library beside the favourites, not in the settings, because it is the
//! reader's writing rather than their preferences.

use egui::{
    Align2, Color32, CornerRadius, CursorIcon, FontId, Frame, Id, Key, Rect, RichText, Sense,
    Stroke, StrokeKind, TextEdit, Ui, pos2, vec2,
};
use serde::{Deserialize, Serialize};

use crate::glass::{self, Glass};
use crate::grid::{self, Cell, Span};
use crate::icons::{self, Icon};
use crate::state::TabId;
use crate::theme::{self, Palette};
use crate::ui::{ChromeContext, UiAction};

// ── The grid ───────────────────────────────────────────────────────────────

/// Twelve columns, always. Cells narrow with the window rather than the grid
/// losing columns: a card at column nine is at column nine in any window, and
/// the arrangement survives a resize.
pub const COLUMNS: u8 = 12;
const ROW_HEIGHT: f32 = 68.0;
const GAP: f32 = 12.0;
/// The page's own margin, inside the content card.
const PAGE_PAD: f32 = 26.0;
/// The widest the grid is allowed to get. Past this the cards stop being cards
/// and start being stripes.
const CANVAS_MAX: f32 = 1240.0;
/// The header row: the greeting, and the three page controls.
const HEADER: f32 = 46.0;
/// The credit line under a photograph.
const CREDIT: f32 = 20.0;
/// Below this the grid is not worth drawing; the page shows a clock and a
/// search box instead.
const NARROW: f32 = 460.0;
/// How deep the board is allowed to get when a card is added and has to be put
/// somewhere. Past what the window shows the page scrolls, so a card never
/// lands nowhere — but nor does it land forty rows down.
const BOARD_ROWS: u8 = 12;

// ── Cards ──────────────────────────────────────────────────────────────────

/// What a card shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Card {
    Search,
    Clock,
    WorldClocks,
    QuickLinks,
    TopSites,
    Recent,
    Favourites,
    Downloads,
    NowPlaying,
    Notes,
    Tasks,
    Workspaces,
    Mark,
}

impl Card {
    pub const ALL: [Card; 13] = [
        Card::Search,
        Card::Clock,
        Card::WorldClocks,
        Card::QuickLinks,
        Card::TopSites,
        Card::Recent,
        Card::Favourites,
        Card::Downloads,
        Card::NowPlaying,
        Card::Notes,
        Card::Tasks,
        Card::Workspaces,
        Card::Mark,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Card::Search => "Search",
            Card::Clock => "Clock",
            Card::WorldClocks => "World clocks",
            Card::QuickLinks => "Quick links",
            Card::TopSites => "Most visited",
            Card::Recent => "Recent",
            Card::Favourites => "Favourites",
            Card::Downloads => "Downloads",
            Card::NowPlaying => "Now playing",
            Card::Notes => "Note",
            Card::Tasks => "To-do",
            Card::Workspaces => "Workspaces",
            Card::Mark => "Zervo mark",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Card::Search => Icon::Search,
            Card::Clock => Icon::Clock,
            Card::WorldClocks => Icon::World,
            Card::QuickLinks => Icon::Apps,
            Card::TopSites => Icon::TrendUp,
            Card::Recent => Icon::Recent,
            Card::Favourites => Icon::Star,
            Card::Downloads => Icon::Download,
            Card::NowPlaying => Icon::Music,
            Card::Notes => Icon::Note,
            Card::Tasks => Icon::Todo,
            Card::Workspaces => Icon::Stack,
            Card::Mark => Icon::Browser,
        }
    }

    fn default_span(self) -> Span {
        match self {
            Card::Search => Span::new(9, 1),
            Card::Clock => Span::new(3, 2),
            Card::WorldClocks => Span::new(6, 2),
            Card::QuickLinks => Span::new(9, 2),
            Card::TopSites => Span::new(5, 2),
            Card::Recent => Span::new(3, 4),
            Card::Favourites => Span::new(4, 2),
            Card::Downloads => Span::new(4, 2),
            Card::NowPlaying => Span::new(4, 1),
            Card::Notes => Span::new(4, 3),
            Card::Tasks => Span::new(3, 3),
            Card::Workspaces => Span::new(3, 2),
            Card::Mark => Span::new(2, 2),
        }
    }

    /// Cards that carry their own material rather than sitting in a card:
    /// over a photograph these read better as light on the picture than as
    /// another rectangle on top of it.
    fn bare(self) -> bool {
        matches!(self, Card::Clock | Card::Mark)
    }

    /// Cards with a heading. The ones without are either bare or so obviously
    /// themselves that a label would be noise.
    fn heading(self) -> bool {
        !matches!(self, Card::Search | Card::Clock | Card::Mark)
    }
}

/// A card, and where it sits.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Tile {
    pub card: Card,
    pub at: Cell,
    pub span: Span,
}

impl Tile {
    fn new(card: Card, col: u8, row: u8) -> Self {
        Tile {
            card,
            at: Cell::new(col, row),
            span: card.default_span(),
        }
    }

    /// The arrangement a fresh install opens with, and the one Reset restores.
    ///
    /// Seven rows deep, which fits the window Zervo opens at without the page
    /// having to be scrolled on the first run.
    pub fn defaults() -> Vec<Tile> {
        vec![
            Tile::new(Card::Clock, 0, 0),
            Tile::new(Card::WorldClocks, 3, 0),
            Tile::new(Card::Recent, 9, 3),
            Tile::new(Card::Search, 0, 2),
            Tile::new(Card::QuickLinks, 0, 3),
            Tile::new(Card::TopSites, 0, 5),
            Tile::new(Card::Favourites, 5, 5),
            {
                let mut tasks = Tile::new(Card::Tasks, 9, 0);
                tasks.span = Span::new(3, 3);
                tasks
            },
        ]
    }

    fn placement(self) -> grid::Placement {
        (self.at, self.span)
    }
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

    /// The zones offered when adding one. A list somebody can read beats every
    /// zone in the table, which is six hundred of them.
    pub const CATALOGUE: [(&'static str, &'static str); 24] = [
        ("London", "Europe/London"),
        ("Paris", "Europe/Paris"),
        ("Berlin", "Europe/Berlin"),
        ("Belgrade", "Europe/Belgrade"),
        ("Lisbon", "Europe/Lisbon"),
        ("Helsinki", "Europe/Helsinki"),
        ("Moscow", "Europe/Moscow"),
        ("Reykjavík", "Atlantic/Reykjavik"),
        ("New York", "America/New_York"),
        ("Chicago", "America/Chicago"),
        ("Denver", "America/Denver"),
        ("Los Angeles", "America/Los_Angeles"),
        ("São Paulo", "America/Sao_Paulo"),
        ("Mexico City", "America/Mexico_City"),
        ("Lagos", "Africa/Lagos"),
        ("Nairobi", "Africa/Nairobi"),
        ("Johannesburg", "Africa/Johannesburg"),
        ("Dubai", "Asia/Dubai"),
        ("Mumbai", "Asia/Kolkata"),
        ("Singapore", "Asia/Singapore"),
        ("Shanghai", "Asia/Shanghai"),
        ("Tokyo", "Asia/Tokyo"),
        ("Sydney", "Australia/Sydney"),
        ("Auckland", "Pacific/Auckland"),
    ];
}

/// What the page asks the arrangement to do. Collected and applied after the
/// cards are drawn, so nothing is mutating the list it is iterating.
enum Change {
    Remove(usize),
    Place { index: usize, at: Cell },
    Resize { index: usize, span: Span },
    Add(Card),
    Reset,
}

// ── The page ───────────────────────────────────────────────────────────────

/// Draw the whole page. Returns true while something on it is still moving,
/// so the caller can schedule the next frame instead of spinning.
pub fn draw(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    content_rect: Rect,
    actions: &mut Vec<UiAction>,
) -> bool {
    let palette = chrome.palette;
    let base = theme::page_base(&palette);
    let radius = CornerRadius::same(theme::CONTENT_RADIUS as u8);

    // The backdrop goes on egui's background layer so every card, menu and
    // text field lands on top of it without having to be ordered.
    let backdrop = root
        .ctx()
        .layer_painter(egui::LayerId::background())
        .with_clip_rect(content_rect);
    backdrop.rect_filled(content_rect, radius, base);

    let photo = chrome.settings.new_tab_background == crate::settings::NewTabBackground::Photo;
    let mut ambient = false;
    let mut over_photo = false;
    if photo {
        over_photo = paint_photo(root, chrome, &backdrop, content_rect, radius, &mut ambient);
    }
    if !over_photo {
        ambient |= crate::ui::paint_newtab_background(
            root,
            &backdrop,
            content_rect,
            &palette,
            chrome.settings.new_tab_background,
            base,
        );
    }

    let ink = Ink::new(&palette, over_photo);

    if content_rect.width() < NARROW || content_rect.height() < 260.0 {
        draw_narrow(root, chrome, content_rect, &ink, actions);
        return ambient;
    }

    // ── The page's own margins, and a ceiling on how wide the grid gets.
    let inner = content_rect.shrink2(vec2(PAGE_PAD, PAGE_PAD * 0.8));
    let width = inner.width().min(CANVAS_MAX);
    let canvas = Rect::from_center_size(
        pos2(inner.center().x, inner.center().y),
        vec2(width, inner.height()),
    );

    let header = Rect::from_min_size(canvas.min, vec2(canvas.width(), HEADER));
    let credit_height = if over_photo { CREDIT } else { 0.0 };
    let board = Rect::from_min_max(
        pos2(canvas.min.x, header.max.y + 8.0),
        pos2(canvas.max.x, canvas.max.y - credit_height),
    );

    draw_header(root, chrome, header, &ink, actions);

    let mut changes = Vec::new();
    ambient |= draw_board(root, chrome, board, &ink, &mut changes, actions);

    if over_photo {
        draw_credit(
            root,
            chrome,
            Rect::from_min_max(pos2(canvas.min.x, board.max.y), canvas.max),
            &ink,
            actions,
        );
    }

    if apply(chrome, changes) {
        actions.push(UiAction::PersistSettings);
    }
    ambient
}

/// Everything the page needs to know about drawing on what is behind it.
///
/// Over a photograph, text is white and carries a shadow; over the chrome's
/// own base it is the palette's. Working that out once and passing it down
/// beats every card deciding for itself and two of them getting it wrong.
struct Ink {
    text: Color32,
    muted: Color32,
    /// True when a photograph is behind everything.
    photo: bool,
    dark: bool,
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
            photo,
            dark: palette.dark,
        }
    }

    /// Draw text, lifted off a photograph by a shadow when there is one. A
    /// white caption on a picture that turns out to be a snowfield is
    /// unreadable otherwise, and no veil strong enough to fix that leaves the
    /// picture worth having.
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
            painter.text(
                pos + vec2(0.0, 1.0),
                align,
                &text,
                font.clone(),
                Color32::from_black_alpha(150),
            );
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
) -> bool {
    let Some(texture) = chrome.wallpaper.texture else {
        return false;
    };
    let palette = chrome.palette;
    // Fade in, keyed on the texture: a new picture appearing all at once is
    // the one moment this page draws attention to itself.
    let fade = glass::ease_out(root.ctx().animate_bool_with_time(
        Id::new("zervo_newtab_photo").with(texture.id()),
        true,
        0.5,
    ));
    if fade <= 0.0 {
        return false;
    }
    *ambient |= fade < 1.0;

    // The picture is fitted to the page at its *full* height and then cropped
    // as the widget shelf takes space off the top, rather than refitted to
    // whatever is left. Refitting rescales the photograph on every frame of
    // the drag, which reads as the wallpaper breathing in and out; cropping
    // holds it still.
    //
    // Between held-still and moving-with-the-page, it drifts down at half the
    // rate the page does. That is what makes opening the shelf read as a bar
    // sliding down over a wallpaper that is behind it, rather than the whole
    // page moving as one sheet.
    const PARALLAX: f32 = 0.5;
    let reveal = crate::ui::shelf_reveal(chrome.browser, chrome.settings).max(0.0);
    let full = Rect::from_min_max(
        pos2(content_rect.min.x, content_rect.min.y - reveal),
        content_rect.max,
    );
    let fitted = cover(texture.size_vec2(), full);
    let hidden = if full.height() > 1.0 {
        (reveal / full.height()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    // The band's height is the visible fraction of the full page, so the
    // points-per-pixel of the picture never changes — only which part shows.
    let top = fitted.min.y + fitted.height() * hidden * PARALLAX;
    let uv = Rect::from_min_max(
        pos2(fitted.min.x, top),
        pos2(fitted.max.x, top + fitted.height() * (1.0 - hidden)),
    );
    backdrop.add(
        egui::epaint::RectShape::filled(content_rect, radius, Color32::WHITE.gamma_multiply(fade))
            .with_texture(texture.id(), uv),
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
    let dim = chrome.settings.wallpaper_dim.clamp(0.0, 1.0) * fade;
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
    true
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

/// Time-of-day greeting, or the reader's own message when they set one.
fn greeting(settings: &crate::settings::Settings) -> String {
    if !settings.newtab_message.trim().is_empty() {
        return settings.newtab_message.trim().to_owned();
    }
    use chrono::Timelike as _;
    match chrono::Local::now().hour() {
        5..=11 => "Good morning",
        12..=17 => "Good afternoon",
        18..=21 => "Good evening",
        _ => "Good night",
    }
    .to_owned()
}

fn draw_header(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    header: Rect,
    ink: &Ink,
    actions: &mut Vec<UiAction>,
) {
    let palette = chrome.palette;
    ink.write(
        root.painter(),
        pos2(header.min.x, header.center().y),
        Align2::LEFT_CENTER,
        greeting(chrome.settings),
        FontId::proportional(19.0),
        ink.text,
    );

    // Right to left, so the buttons keep their order as the window narrows.
    let editing = chrome.browser.newtab_editing;
    let mut cursor = header.max.x;
    let done = header_button(
        root,
        ink,
        &palette,
        &mut cursor,
        header,
        if editing { Icon::Check } else { Icon::Sliders },
        if editing { "Done" } else { "Customise" },
        editing,
    );
    if done.clicked() {
        chrome.browser.newtab_editing = !editing;
    }

    let add = header_button(
        root,
        ink,
        &palette,
        &mut cursor,
        header,
        Icon::Plus,
        "Add card",
        false,
    );
    let add_anchor = add.rect;
    let menu_id = Id::new("zervo_newtab_add_open");
    if add.clicked() {
        // Adding puts the page into edit mode: the card lands somewhere, and
        // somewhere is rarely where it is wanted.
        chrome.browser.newtab_editing = true;
        let open = root.ctx().data(|data| data.get_temp::<bool>(menu_id)) == Some(true);
        root.ctx().data_mut(|data| data.insert_temp(menu_id, !open));
    }
    if root.ctx().data(|data| data.get_temp::<bool>(menu_id)) == Some(true) {
        let rows: Vec<(String, Card, bool)> = Card::ALL
            .iter()
            .map(|card| {
                let already = chrome
                    .settings
                    .newtab_tiles
                    .iter()
                    .any(|tile| tile.card == *card);
                (card.label().to_owned(), *card, already)
            })
            .collect();
        match crate::dashboard::menu(
            root,
            &palette,
            "zervo_newtab_add_menu",
            add_anchor,
            200.0,
            &rows,
        ) {
            Some(card) => {
                root.ctx().data_mut(|data| data.insert_temp(menu_id, false));
                root.ctx()
                    .data_mut(|data| data.insert_temp(Id::new("zervo_newtab_added"), card));
            },
            None if root.input(|input| input.pointer.any_pressed())
                && !crate::dashboard::over_menu(root.ctx())
                && !add.clicked() =>
            {
                root.ctx().data_mut(|data| data.insert_temp(menu_id, false));
            },
            None => {},
        }
    }

    let wallpaper = header_button(
        root,
        ink,
        &palette,
        &mut cursor,
        header,
        Icon::Mountains,
        // Never lit. It opens a menu; it is not a state. Tinting it because a
        // photograph happens to be up made it read as a button stuck down.
        "Wallpaper",
        false,
    );
    let wallpaper_anchor = wallpaper.rect;
    draw_wallpaper_menu(root, chrome, wallpaper_anchor, &wallpaper, actions);

    if editing {
        let reset = header_button(
            root,
            ink,
            &palette,
            &mut cursor,
            header,
            Icon::Reset,
            "Reset",
            false,
        );
        if reset.clicked() {
            root.ctx()
                .data_mut(|data| data.insert_temp(Id::new("zervo_newtab_reset"), true));
        }
    }
}

/// One control in the header: a glyph and a word on a small pill. Advances
/// `cursor` leftward by its own width.
///
/// The pill is always there, not only on hover. These four sit at the top of
/// whatever photograph happens to be up, and a bare white word on a bright sky
/// — or a dark one on a dark photograph — is a control nobody can find. Giving
/// them the same material as the cards costs a little ink and means the header
/// reads the same on every wallpaper and in both themes.
#[allow(clippy::too_many_arguments)]
fn header_button(
    root: &mut Ui,
    ink: &Ink,
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
        Id::new("zervo_newtab_header").with(label),
        Sense::click(),
    );
    let hover = glass::ease_out(root.ctx().animate_bool_with_time(
        response.id.with("hover"),
        response.hovered(),
        0.12,
    ));
    let on = glass::ease_out(
        root.ctx()
            .animate_bool_with_time(response.id.with("on"), lit, 0.18),
    );

    // No shadow: four of these sit shoulder to shoulder, and a drop shadow
    // each would bleed onto the neighbour.
    let mut material = Glass::new(9)
        .strength(0.7 + 0.3 * hover.max(on))
        .no_shadow()
        .opaque(theme::card_backing(palette, ink.photo));
    if on > 0.0 {
        material = material.tint(theme::mix(palette.surface, palette.accent, 0.45 * on));
    }
    let painter = root.painter();
    painter.extend(glass::shapes(rect, palette, material));

    let color = if on > 0.5 {
        theme::mix(palette.text, palette.accent, 0.85)
    } else {
        theme::mix(palette.text_muted, palette.text, hover)
    };
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

/// The wallpaper control's menu: shuffle, a source, a file, or off.
fn draw_wallpaper_menu(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    anchor: Rect,
    trigger: &egui::Response,
    actions: &mut Vec<UiAction>,
) {
    let palette = chrome.palette;
    let menu_id = Id::new("zervo_newtab_wallpaper_open");
    if trigger.clicked() {
        let open = root.ctx().data(|data| data.get_temp::<bool>(menu_id)) == Some(true);
        root.ctx().data_mut(|data| data.insert_temp(menu_id, !open));
    }
    if root.ctx().data(|data| data.get_temp::<bool>(menu_id)) != Some(true) {
        return;
    }

    use crate::wallpaper::{Source, Subject};
    #[derive(Clone, Copy, PartialEq)]
    enum Pick {
        Shuffle,
        Commons,
        Subject(Subject),
        File,
        Off,
    }
    let source = &chrome.settings.wallpaper_source;
    let mut rows: Vec<(String, Pick, bool)> = vec![
        ("Another picture".to_owned(), Pick::Shuffle, false),
        (
            "Commons picture of the day".to_owned(),
            Pick::Commons,
            *source == Source::Commons,
        ),
    ];
    rows.extend(Subject::ALL.iter().map(|subject| {
        (
            format!("Openverse — {}", subject.label().to_lowercase()),
            Pick::Subject(*subject),
            *source == Source::Openverse(*subject),
        )
    }));
    rows.push((
        "Choose a file…".to_owned(),
        Pick::File,
        matches!(source, Source::File(_)),
    ));
    rows.push(("No photograph".to_owned(), Pick::Off, false));

    match crate::dashboard::menu(
        root,
        &palette,
        "zervo_newtab_wallpaper_menu",
        anchor,
        250.0,
        &rows,
    ) {
        Some(pick) => {
            root.ctx().data_mut(|data| data.insert_temp(menu_id, false));
            match pick {
                Pick::Shuffle => actions.push(UiAction::ShuffleWallpaper),
                Pick::Commons => {
                    chrome.settings.wallpaper_source = Source::Commons;
                    chrome.settings.new_tab_background = crate::settings::NewTabBackground::Photo;
                    actions.push(UiAction::ShuffleWallpaper);
                },
                Pick::Subject(subject) => {
                    chrome.settings.wallpaper_source = Source::Openverse(subject);
                    chrome.settings.new_tab_background = crate::settings::NewTabBackground::Photo;
                    actions.push(UiAction::ShuffleWallpaper);
                },
                Pick::File => actions.push(UiAction::PickWallpaper),
                Pick::Off => {
                    chrome.settings.new_tab_background = crate::settings::NewTabBackground::Aurora;
                    actions.push(UiAction::PersistSettings);
                },
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
        Id::new("zervo_newtab_credit"),
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
    ink.write(
        root.painter(),
        pos2(hit.min.x, hit.center().y),
        Align2::LEFT_CENTER,
        &line,
        font,
        theme::mix(ink.muted, ink.text, hover),
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

// ── The board ──────────────────────────────────────────────────────────────

/// Draw the arrangement, and take drags, drops, resizes and removals.
fn draw_board(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    board: Rect,
    ink: &Ink,
    changes: &mut Vec<Change>,
    actions: &mut Vec<UiAction>,
) -> bool {
    let ctx = root.ctx().clone();
    let palette = chrome.palette;
    let editing = chrome.browser.newtab_editing;

    // Deferred asks from the header, which cannot reach the change list from
    // where it is drawn.
    let added = Id::new("zervo_newtab_added");
    if let Some(card) = ctx.data(|data| data.get_temp::<Card>(added)) {
        ctx.data_mut(|data| data.remove::<Card>(added));
        changes.push(Change::Add(card));
    }
    if ctx
        .data_mut(|data| data.remove_temp::<bool>(Id::new("zervo_newtab_reset")))
        .is_some()
    {
        changes.push(Change::Reset);
    }

    let tiles = chrome.settings.newtab_tiles.clone();
    let metrics = grid::Metrics::new(board, COLUMNS, ROW_HEIGHT, GAP);
    // The grid is as deep as the arrangement needs or as the window allows,
    // whichever is more; anything past the window scrolls.
    let needed = tiles
        .iter()
        .map(|tile| tile.at.row.saturating_add(tile.span.h.max(1)))
        .max()
        .unwrap_or(1)
        .max(1);
    let rows = needed.max(metrics.rows);
    let overflow = (metrics.height_for(rows) - board.height()).max(0.0);

    let scroll_id = Id::new("zervo_newtab_scroll");
    let mut scroll = ctx
        .data(|data| data.get_temp::<f32>(scroll_id))
        .unwrap_or(0.0);
    let pointer = ctx.input(|input| input.pointer.latest_pos());
    if overflow > 0.0 && pointer.is_some_and(|pos| board.contains(pos)) {
        scroll -= ctx.input(|input| input.smooth_scroll_delta.y);
    }
    scroll = scroll.clamp(0.0, overflow);
    ctx.data_mut(|data| data.insert_temp(scroll_id, scroll));

    let metrics = grid::Metrics {
        rows,
        origin: pos2(board.min.x, board.min.y - scroll),
        ..metrics
    };

    let mut area = root.new_child(egui::UiBuilder::new().max_rect(board));
    // Wide enough for a card in the first or last column to keep its shadow,
    // and no taller: the vertical clip is what stops a scrolled card spilling
    // over the header and the credit line. The number is `glass`'s own spread
    // for a card-sized radius; it is written out rather than imported because
    // being a point or two generous here costs nothing and a dependency on
    // the material's internals costs something.
    const SHADOW_ROOM: f32 = 12.0;
    area.set_clip_rect(board.expand2(vec2(SHADOW_ROOM, 0.0)));
    let area = &mut area;

    if editing {
        // The board itself, so it is obvious what the cards are being
        // arranged on. One faint wash, not a grid of lines: the cards are
        // what the eye should be following.
        area.painter().rect_filled(
            board.expand(6.0),
            CornerRadius::same(12),
            palette
                .accent
                .gamma_multiply(if ink.dark { 0.05 } else { 0.045 }),
        );
    }

    let drag_id = Id::new("zervo_newtab_drag");
    let grab_id = Id::new("zervo_newtab_grab");
    let resize_id = Id::new("zervo_newtab_resize");
    let held = ctx.data(|data| data.get_temp::<usize>(drag_id));
    let resizing = ctx.data(|data| data.get_temp::<usize>(resize_id));
    // Where inside the card the pointer went down. Kept, so a card follows the
    // pointer from the corner it was picked up by. Centring it on the pointer
    // instead — which is the obvious thing, and what the shelf does with its
    // one-cell widgets — makes a card nine columns wide jump half its own
    // width the moment it is touched, and a card grabbed near its right edge
    // lands several rows further down than the pointer ever went.
    let grab = ctx
        .data(|data| data.get_temp::<egui::Vec2>(grab_id))
        .unwrap_or_default();

    // Where a carried card would land, and what is already there.
    let landing = match (editing.then_some(held).flatten(), pointer) {
        (Some(index), Some(pos)) => tiles
            .get(index)
            .map(|tile| metrics.cell_at(pos - grab, tile.span)),
        _ => None,
    };
    if let Some(at) = landing
        && let Some(tile) = held.and_then(|index| tiles.get(index))
    {
        let destination = metrics.rect(at, tile.span);
        area.painter().rect_filled(
            destination,
            CornerRadius::same(10),
            palette.accent.gamma_multiply(0.10),
        );
        area.painter().rect_stroke(
            destination,
            CornerRadius::same(10),
            Stroke::new(1.0_f32, palette.accent.gamma_multiply(0.85)),
            StrokeKind::Inside,
        );
    }

    let mut ambient = false;
    let mut carried = None;
    for (index, tile) in tiles.iter().enumerate() {
        let slot = metrics.rect(tile.at, tile.span);
        if !slot.intersects(board.expand(ROW_HEIGHT)) {
            continue; // Scrolled out of sight; nothing to draw or interact with.
        }
        let mine = held == Some(index);
        let over = pointer.is_some_and(|pos| slot.contains(pos) && board.contains(pos));

        if editing {
            let response = area.interact(slot, drag_id.with(index), Sense::click_and_drag());
            if response.drag_started() {
                let offset = response
                    .interact_pointer_pos()
                    .map(|pos| pos - slot.min)
                    .unwrap_or_default();
                ctx.data_mut(|data| {
                    data.insert_temp(drag_id, index);
                    data.insert_temp(grab_id, offset);
                });
            }
            if mine && response.drag_stopped() {
                ctx.data_mut(|data| data.remove::<usize>(drag_id));
                if let Some(at) = landing {
                    changes.push(Change::Place { index, at });
                }
            }
            if over || mine {
                ctx.set_cursor_icon(if mine {
                    CursorIcon::Grabbing
                } else {
                    CursorIcon::Grab
                });
            }
        }

        // A carried card follows the pointer, and is drawn last so it passes
        // over its neighbours rather than under them.
        let drawn = match (mine, pointer) {
            (true, Some(pos)) => {
                Rect::from_min_size(pos - metrics.size(tile.span) / 2.0, slot.size())
            },
            _ => slot,
        };
        if mine {
            carried = Some((index, *tile, drawn));
            continue;
        }

        ambient |= draw_tile(
            area,
            chrome,
            *tile,
            drawn,
            ink,
            false,
            card_id(index),
            actions,
        );

        if editing && over {
            draw_handles(area, &palette, drawn, index, changes);
            draw_resizer(
                area, &palette, &metrics, *tile, index, drawn, changes, resizing,
            );
        }
    }

    if let Some((index, tile, drawn)) = carried {
        ambient |= draw_tile(
            area,
            chrome,
            tile,
            drawn,
            ink,
            true,
            card_id(index),
            actions,
        );
    }

    if overflow > 0.0 {
        // A thumb rather than a scrollbar: there is nothing to grab, it is
        // only there to say the page continues.
        let travel = board.height() - 40.0;
        let thumb = Rect::from_min_size(
            pos2(
                board.max.x + 4.0,
                board.min.y + travel * (scroll / overflow),
            ),
            vec2(3.0, 40.0),
        );
        area.painter()
            .rect_filled(thumb, CornerRadius::same(2), ink.muted.gamma_multiply(0.5));
    }

    ambient
}

/// The remove button, and the mark that says a card can be picked up.
fn draw_handles(
    area: &mut Ui,
    palette: &Palette,
    rect: Rect,
    index: usize,
    changes: &mut Vec<Change>,
) {
    let close =
        Rect::from_center_size(pos2(rect.max.x - 12.0, rect.min.y + 12.0), vec2(18.0, 18.0));
    area.painter()
        .rect_filled(close, CornerRadius::same(7), palette.surface_hover);
    icons::draw_icon(
        area.painter(),
        close.shrink(4.0),
        Icon::Close,
        palette.text_muted,
    );
    if area
        .interact(
            close,
            Id::new("zervo_newtab_remove").with(index),
            Sense::click(),
        )
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text("Take this card off")
        .clicked()
    {
        changes.push(Change::Remove(index));
    }
    icons::draw_icon(
        area.painter(),
        Rect::from_center_size(pos2(rect.min.x + 12.0, rect.min.y + 12.0), vec2(13.0, 13.0)),
        Icon::DragHandle,
        palette.text_muted.gamma_multiply(0.8),
    );
}

/// The corner that resizes by whole cells, with a preview of what it would
/// come to.
#[allow(clippy::too_many_arguments)]
fn draw_resizer(
    area: &mut Ui,
    palette: &Palette,
    metrics: &grid::Metrics,
    tile: Tile,
    index: usize,
    rect: Rect,
    changes: &mut Vec<Change>,
    resizing: Option<usize>,
) {
    let ctx = area.ctx().clone();
    let resize_id = Id::new("zervo_newtab_resize");
    let corner =
        Rect::from_center_size(pos2(rect.max.x - 12.0, rect.max.y - 12.0), vec2(22.0, 22.0));
    icons::draw_icon(
        area.painter(),
        corner.shrink(5.0),
        Icon::Resize,
        palette.text_muted.gamma_multiply(0.8),
    );
    let handle = area.interact(
        corner,
        Id::new("zervo_newtab_size").with(index),
        Sense::click_and_drag(),
    );
    if handle.hovered() || resizing == Some(index) {
        ctx.set_cursor_icon(CursorIcon::ResizeNwSe);
    }
    if handle.drag_started() {
        ctx.data_mut(|data| data.insert_temp(resize_id, index));
    }
    if resizing != Some(index) {
        return;
    }
    let Some(pointer) = ctx.input(|input| input.pointer.latest_pos()) else {
        return;
    };
    let stride = metrics.cell + vec2(metrics.gap, metrics.gap);
    let wanted = Span::new(
        (((pointer.x - rect.min.x + metrics.gap) / stride.x).round() as i32)
            .clamp(1, metrics.columns as i32) as u8,
        (((pointer.y - rect.min.y + metrics.gap) / stride.y).round() as i32)
            .clamp(1, metrics.rows as i32) as u8,
    );
    area.painter().rect_stroke(
        Rect::from_min_size(rect.min, metrics.size(wanted)),
        CornerRadius::same(10),
        Stroke::new(1.0_f32, palette.accent.gamma_multiply(0.85)),
        StrokeKind::Inside,
    );
    if handle.drag_stopped() {
        ctx.data_mut(|data| data.remove::<usize>(resize_id));
        if wanted != tile.span {
            changes.push(Change::Resize {
                index,
                span: wanted,
            });
        }
    }
}

/// Apply what the page asked for. Returns true when the arrangement moved and
/// wants writing out.
fn apply(chrome: &mut ChromeContext, changes: Vec<Change>) -> bool {
    if changes.is_empty() {
        return false;
    }
    // A change that adds or removes a card renumbers the ones after it, so
    // anything still holding an index from this frame is now pointing at the
    // wrong card. Nothing here can generate two structural changes at once
    // anyway — you cannot drop a card and press its ✕ in the same frame — so
    // the rest are dropped rather than applied to the wrong tile.
    let mut structural = false;
    for change in changes {
        if structural {
            break;
        }
        structural = matches!(change, Change::Remove(_) | Change::Add(_) | Change::Reset);
        let tiles = &mut chrome.settings.newtab_tiles;
        match change {
            Change::Remove(index) => {
                if index < tiles.len() {
                    tiles.remove(index);
                }
            },
            Change::Place { index, at } => {
                if let Some(tile) = tiles.get_mut(index) {
                    tile.at = at;
                }
            },
            Change::Resize { index, span } => {
                if let Some(tile) = tiles.get_mut(index) {
                    tile.span = span;
                }
            },
            Change::Add(card) => {
                let taken: Vec<grid::Placement> =
                    tiles.iter().map(|tile| tile.placement()).collect();
                let span = card.default_span();
                let at =
                    grid::free_cell(&taken, span, COLUMNS, BOARD_ROWS).unwrap_or(Cell::new(0, 0));
                tiles.push(Tile { card, at, span });
            },
            Change::Reset => *tiles = Tile::defaults(),
        }
    }
    true
}

// ── Drawing one card ───────────────────────────────────────────────────────

/// Draw a card's frame and its contents. Returns true when it wants another
/// frame soon.
/// Every widget inside a card hangs off this, and it is the card's place in
/// the arrangement rather than its kind or its position on screen. Two cards
/// of the same kind are allowed, and two cards side by side have rows at
/// exactly the same height — either would collide on any other key, and egui
/// answers a collision with a red banner across the page.
fn card_id(index: usize) -> Id {
    Id::new("zervo_newtab_card").with(index)
}

#[allow(clippy::too_many_arguments)]
fn draw_tile(
    area: &mut Ui,
    chrome: &mut ChromeContext,
    tile: Tile,
    rect: Rect,
    ink: &Ink,
    carried: bool,
    id: Id,
    actions: &mut Vec<UiAction>,
) -> bool {
    let palette = chrome.palette;
    let editing = chrome.browser.newtab_editing;
    // A bare card has no material of its own — until it is being arranged,
    // when it needs an edge to be grabbed by.
    let framed = !tile.card.bare() || editing;
    if framed && tile.card != Card::Search {
        let material = if carried {
            Glass::new(10)
        } else if editing {
            Glass::new(10)
                .strength(0.8)
                .border(palette.accent.gamma_multiply(0.55))
        } else {
            Glass::new(10).strength(if ink.photo { 0.95 } else { 0.85 })
        }
        .opaque(theme::card_backing(&palette, ink.photo));
        area.painter()
            .extend(glass::shapes(rect, &palette, material));
    }

    let mut body = rect.shrink(12.0);
    if tile.card.heading() {
        let head = pos2(rect.min.x + 13.0, rect.min.y + 17.0);
        icons::draw_icon(
            area.painter(),
            Rect::from_center_size(head + vec2(0.0, 0.0), vec2(12.0, 12.0)),
            tile.card.icon(),
            palette.text_muted,
        );
        area.painter().text(
            head + vec2(11.0, 0.0),
            Align2::LEFT_CENTER,
            tile.card.label().to_uppercase(),
            FontId::proportional(10.0),
            palette.text_muted,
        );
        body = Rect::from_min_max(
            pos2(rect.min.x + 12.0, rect.min.y + 30.0),
            rect.max - vec2(12.0, 10.0),
        );
    }
    if body.width() < 8.0 || body.height() < 8.0 {
        return false;
    }

    // While a card is being arranged it is a handle, not a control: nothing
    // inside it takes a click, so a drag never opens a link by accident.
    let live = !editing && !carried;

    match tile.card {
        Card::Search => draw_search(area, chrome, rect, ink, live, id, actions),
        Card::Clock => return draw_clock(area, rect, ink),
        Card::WorldClocks => return draw_world_clocks(area, chrome, body, ink),
        Card::QuickLinks => draw_quick_links(area, chrome, body, live, id, actions),
        Card::TopSites => draw_top_sites(area, chrome, body, live, id, actions),
        Card::Recent => draw_recent(area, chrome, body, live, id, actions),
        Card::Favourites => draw_favourites(area, chrome, body, live, id, actions),
        Card::Downloads => draw_downloads(area, chrome, body, live, id, actions),
        Card::NowPlaying => draw_now_playing(area, chrome, body, live, id, actions),
        Card::Notes => draw_notes(area, chrome, body, live, id),
        Card::Tasks => draw_tasks(area, chrome, body, live, id),
        Card::Workspaces => draw_workspaces(area, chrome, body, live, id, actions),
        Card::Mark => draw_mark(area, &palette, rect, ink),
    }
    false
}

// ── The narrow page ────────────────────────────────────────────────────────

/// What is left when the window is too small for a grid: the two things
/// somebody opening a new tab in a narrow window actually wants.
fn draw_narrow(
    root: &mut Ui,
    chrome: &mut ChromeContext,
    content_rect: Rect,
    ink: &Ink,
    actions: &mut Vec<UiAction>,
) {
    let centre = content_rect.center();
    let now = chrono::Local::now();
    ink.write(
        root.painter(),
        pos2(centre.x, centre.y - 58.0),
        Align2::CENTER_CENTER,
        now.format("%H:%M").to_string(),
        FontId::proportional(44.0),
        ink.text,
    );
    let pill = Rect::from_center_size(
        pos2(centre.x, centre.y + 8.0),
        vec2((content_rect.width() - 48.0).max(120.0), 44.0),
    );
    draw_search(
        root,
        chrome,
        pill,
        ink,
        true,
        Id::new("zervo_newtab_narrow"),
        actions,
    );
    root.ctx()
        .request_repaint_after(std::time::Duration::from_secs(20));
}

// ── Cards ──────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn draw_search(
    area: &mut Ui,
    chrome: &mut ChromeContext,
    rect: Rect,
    ink: &Ink,
    live: bool,
    id: Id,
    actions: &mut Vec<UiAction>,
) {
    let palette = chrome.palette;
    let height = rect.height().clamp(38.0, 52.0);
    let pill = Rect::from_center_size(rect.center(), vec2(rect.width(), height));

    // Focus is read from what last frame recorded. A frame's lag on a
    // two-tenths-of-a-second bloom is not something anyone can see, and the
    // alternative is painting the pill after the field that sits inside it.
    let focus_id = id.with("focused");
    let focused = area.ctx().data(|data| data.get_temp::<bool>(focus_id)) == Some(true);
    let focus = glass::ease_out(
        area.ctx()
            .animate_bool_with_time(id.with("focus"), focused, 0.22),
    );

    // Over a photograph the pill has to be found at a glance, so it sits a
    // shade above the cards rather than level with them. Level with them, on a
    // picture with a bright sky in it, is invisible.
    let backing = if ink.photo {
        theme::mix(
            theme::card_backing(&palette, true),
            palette.surface_hover,
            0.75,
        )
    } else {
        theme::card_backing(&palette, false)
    };
    glass::paint(
        area.painter(),
        pill,
        &palette,
        Glass::new((height * 0.3) as u8)
            .strength(1.0)
            .glow(focus)
            .opaque(backing),
    );
    icons::draw_icon(
        area.painter(),
        Rect::from_center_size(pos2(pill.min.x + 20.0, pill.center().y), vec2(16.0, 16.0)),
        Icon::Search,
        theme::mix(palette.text_muted, palette.accent, focus),
    );

    let hint = format!(
        "Search with {} or enter an address…",
        chrome.settings.search_engine.label()
    );
    let field = Rect::from_min_max(
        pos2(pill.min.x + 36.0, pill.min.y),
        pos2(pill.max.x - 14.0, pill.max.y),
    );
    if !live {
        // Inert while the page is being arranged: a text field that takes
        // focus mid-drag steals the keyboard from whatever was doing.
        let shown = if chrome.browser.newtab_query.trim().is_empty() {
            hint
        } else {
            chrome.browser.newtab_query.clone()
        };
        area.painter().text(
            pos2(field.min.x, field.center().y),
            Align2::LEFT_CENTER,
            fit(
                area.painter(),
                &shown,
                &FontId::proportional(14.5),
                field.width(),
            ),
            FontId::proportional(14.5),
            palette.text_muted,
        );
        return;
    }

    // The field's own id comes from the card's, not from where it happens to
    // sit: two search cards on one page would otherwise be one text field.
    let mut inner = area.new_child(
        egui::UiBuilder::new()
            .max_rect(field)
            .id_salt(id)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let editor = TextEdit::singleline(&mut chrome.browser.newtab_query)
        .frame(Frame::NONE)
        .font(FontId::proportional(14.5))
        .text_color(palette.text)
        .vertical_align(egui::Align::Center)
        .hint_text(RichText::new(hint).color(theme::mix(
            palette.text_muted,
            palette.text,
            if ink.photo { 0.45 } else { 0.15 },
        )))
        .desired_width(field.width());
    let response = inner.add(editor);
    let has_focus = response.has_focus();
    area.ctx()
        .data_mut(|data| data.insert_temp(focus_id, has_focus));
    if response.lost_focus()
        && inner.input(|input| input.key_pressed(Key::Enter))
        && !chrome.browser.newtab_query.trim().is_empty()
    {
        let target =
            crate::ui::normalize_url(&chrome.browser.newtab_query, chrome.settings.search_engine);
        chrome.browser.newtab_query.clear();
        actions.push(UiAction::Navigate(target));
    }
}

fn draw_clock(area: &mut Ui, rect: Rect, ink: &Ink) -> bool {
    let now = chrono::Local::now();
    // The time scales with the card: at one row it is a line of text, at three
    // it is the thing you look at first.
    let size = (rect.height() * 0.42).clamp(22.0, 62.0);
    ink.write(
        area.painter(),
        pos2(rect.center().x, rect.center().y - size * 0.22),
        Align2::CENTER_CENTER,
        now.format("%H:%M").to_string(),
        FontId::proportional(size),
        ink.text,
    );
    if rect.height() > 56.0 {
        ink.write(
            area.painter(),
            pos2(rect.center().x, rect.center().y + size * 0.48),
            Align2::CENTER_CENTER,
            now.format("%A, %-d %B").to_string(),
            FontId::proportional(12.0),
            ink.muted,
        );
    }
    area.ctx()
        .request_repaint_after(std::time::Duration::from_secs(20));
    false
}

fn draw_world_clocks(area: &mut Ui, chrome: &ChromeContext, body: Rect, ink: &Ink) -> bool {
    let palette = chrome.palette;
    let zones = &chrome.settings.newtab_world_clocks;
    if zones.is_empty() {
        empty(area, &palette, body, "Add a city in Settings › Layout.");
        return false;
    }
    let now = chrono::Utc::now();
    // As many across as fit at a readable width, the rest wrap onto a row of
    // their own.
    let per_row = ((body.width() / 96.0).floor() as usize).clamp(1, zones.len());
    let rows = zones.len().div_ceil(per_row);
    let cell = vec2(body.width() / per_row as f32, body.height() / rows as f32);
    for (index, zone) in zones.iter().enumerate() {
        let slot = Rect::from_min_size(
            pos2(
                body.min.x + (index % per_row) as f32 * cell.x,
                body.min.y + (index / per_row) as f32 * cell.y,
            ),
            cell,
        );
        let Ok(tz) = zone.name.parse::<chrono_tz::Tz>() else {
            area.painter().text(
                slot.center(),
                Align2::CENTER_CENTER,
                "—",
                FontId::proportional(13.0),
                palette.text_muted,
            );
            continue;
        };
        let there = now.with_timezone(&tz);
        let clock = (slot.height() * 0.34).clamp(16.0, 26.0);
        area.painter().text(
            pos2(slot.min.x + 2.0, slot.min.y + 9.0),
            Align2::LEFT_TOP,
            fit(
                area.painter(),
                &zone.label,
                &FontId::proportional(11.0),
                cell.x - 8.0,
            ),
            FontId::proportional(11.0),
            palette.text_muted,
        );
        area.painter().text(
            pos2(slot.min.x + 2.0, slot.min.y + 24.0),
            Align2::LEFT_TOP,
            there.format("%H:%M").to_string(),
            FontId::proportional(clock),
            ink.text,
        );
        if slot.height() > 58.0 {
            area.painter().text(
                pos2(slot.min.x + 2.0, slot.min.y + 26.0 + clock),
                Align2::LEFT_TOP,
                there.format("%Z · %a").to_string(),
                FontId::proportional(10.0),
                palette.text_muted,
            );
        }
    }
    area.ctx()
        .request_repaint_after(std::time::Duration::from_secs(20));
    false
}

fn draw_quick_links(
    area: &mut Ui,
    chrome: &mut ChromeContext,
    body: Rect,
    live: bool,
    id: Id,
    actions: &mut Vec<UiAction>,
) {
    let palette = chrome.palette;
    struct Link {
        tab_id: TabId,
        workspace: usize,
        title: String,
    }
    let links: Vec<Link> = chrome
        .browser
        .workspaces
        .iter()
        .enumerate()
        .flat_map(|(workspace, space)| {
            space
                .tabs
                .iter()
                .filter(|tab| tab.pinned)
                .map(move |tab| Link {
                    tab_id: tab.id,
                    workspace,
                    title: crate::ui::display_name(&tab.title, &tab.url).to_owned(),
                })
        })
        .take(24)
        .collect();
    if links.is_empty() {
        empty(area, &palette, body, "Pin a tab and it turns up here.");
        return;
    }

    let tile = vec2(74.0, 70.0);
    let per_row = ((body.width() / tile.x).floor() as usize).max(1);
    for (index, link) in links.iter().enumerate() {
        let column = index % per_row;
        let row = index / per_row;
        let slot = Rect::from_min_size(
            pos2(
                body.min.x + column as f32 * tile.x,
                body.min.y + row as f32 * tile.y,
            ),
            tile,
        )
        .shrink(3.0);
        if slot.max.y > body.max.y + 6.0 {
            break; // The card is full; the rest are in the sidebar.
        }
        let response = area.interact(
            slot,
            id.with(("link", link.tab_id)),
            if live { Sense::click() } else { Sense::hover() },
        );
        let hover = glass::ease_out(area.ctx().animate_bool_with_time(
            response.id.with("hover"),
            live && response.hovered(),
            0.12,
        ));
        if hover > 0.0 {
            area.painter().rect_filled(
                slot,
                CornerRadius::same(9),
                palette.surface_hover.gamma_multiply(hover),
            );
        }
        let icon = pos2(slot.center().x, slot.min.y + 22.0);
        match chrome.favicons.get(&link.tab_id) {
            Some(texture) => {
                area.painter().image(
                    texture.id(),
                    Rect::from_center_size(icon, vec2(22.0, 22.0)),
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            },
            None => icons::draw_icon(
                area.painter(),
                Rect::from_center_size(icon, vec2(19.0, 19.0)),
                Icon::Globe,
                palette.text_muted,
            ),
        }
        area.painter().text(
            pos2(slot.center().x, slot.max.y - 12.0),
            Align2::CENTER_CENTER,
            fit(
                area.painter(),
                &link.title,
                &FontId::proportional(10.5),
                slot.width() - 4.0,
            ),
            FontId::proportional(10.5),
            theme::mix(palette.text_muted, palette.text, hover),
        );
        if live
            && response
                .on_hover_cursor(CursorIcon::PointingHand)
                .on_hover_text(&link.title)
                .clicked()
        {
            actions.push(UiAction::SelectWorkspace(link.workspace));
            actions.push(UiAction::SelectTab(link.tab_id));
        }
    }
}

fn draw_top_sites(
    area: &mut Ui,
    chrome: &mut ChromeContext,
    body: Rect,
    live: bool,
    id: Id,
    actions: &mut Vec<UiAction>,
) {
    let palette = chrome.palette;
    let rows = ((body.height() / 30.0).floor() as usize).max(1);
    let sites: Vec<(String, String, usize)> = chrome
        .library
        .top_sites(rows)
        .iter()
        .map(|site| (site.host.clone(), site.url.clone(), site.visits))
        .collect();
    if sites.is_empty() {
        empty(
            area,
            &palette,
            body,
            "Nowhere yet — this fills as you browse.",
        );
        return;
    }
    for (index, (host, url, visits)) in sites.iter().enumerate() {
        let row = Rect::from_min_size(
            pos2(body.min.x, body.min.y + index as f32 * 30.0),
            vec2(body.width(), 28.0),
        );
        if row.max.y > body.max.y + 4.0 {
            break;
        }
        let response = list_row(area, &palette, row, id, index, host, host, live);
        area.painter().text(
            pos2(row.max.x - 4.0, row.center().y),
            Align2::RIGHT_CENTER,
            visits.to_string(),
            FontId::proportional(11.0),
            palette.text_muted,
        );
        if live && response.clicked() {
            actions.push(UiAction::Navigate(url.clone()));
        }
    }
}

fn draw_recent(
    area: &mut Ui,
    chrome: &mut ChromeContext,
    body: Rect,
    live: bool,
    id: Id,
    actions: &mut Vec<UiAction>,
) {
    let palette = chrome.palette;
    let rows = ((body.height() / 30.0).floor() as usize).max(1);
    let visits: Vec<(String, String)> = chrome
        .library
        .recent(rows)
        .iter()
        .map(|visit| {
            (
                crate::ui::display_name(&visit.title, &visit.url).to_owned(),
                visit.url.clone(),
            )
        })
        .collect();
    if visits.is_empty() {
        empty(area, &palette, body, "Nothing visited yet.");
        return;
    }
    for (index, (title, url)) in visits.iter().enumerate() {
        let row = Rect::from_min_size(
            pos2(body.min.x, body.min.y + index as f32 * 30.0),
            vec2(body.width(), 28.0),
        );
        if row.max.y > body.max.y + 4.0 {
            break;
        }
        let response = list_row(area, &palette, row, id, index, title, url, live);
        if live && response.clicked() {
            actions.push(UiAction::Navigate(url.clone()));
        }
    }
}

fn draw_favourites(
    area: &mut Ui,
    chrome: &mut ChromeContext,
    body: Rect,
    live: bool,
    id: Id,
    actions: &mut Vec<UiAction>,
) {
    let palette = chrome.palette;
    let rows = ((body.height() / 30.0).floor() as usize).max(1);
    let saved: Vec<(String, String)> = chrome
        .library
        .favourites
        .iter()
        .rev()
        .take(rows)
        .map(|entry| (entry.title.clone(), entry.url.clone()))
        .collect();
    if saved.is_empty() {
        empty(
            area,
            &palette,
            body,
            "Nothing saved yet — the star adds a page.",
        );
        return;
    }
    for (index, (title, url)) in saved.iter().enumerate() {
        let row = Rect::from_min_size(
            pos2(body.min.x, body.min.y + index as f32 * 30.0),
            vec2(body.width(), 28.0),
        );
        if row.max.y > body.max.y + 4.0 {
            break;
        }
        let response = list_row(area, &palette, row, id, index, title, url, live);
        if live && response.clicked() {
            actions.push(UiAction::Navigate(url.clone()));
        }
    }
}

fn draw_downloads(
    area: &mut Ui,
    chrome: &mut ChromeContext,
    body: Rect,
    live: bool,
    id: Id,
    actions: &mut Vec<UiAction>,
) {
    let palette = chrome.palette;
    let rows = ((body.height() / 34.0).floor() as usize).max(1);
    let items: Vec<(u64, String, Option<f32>, bool)> = chrome
        .downloads
        .items
        .iter()
        .rev()
        .take(rows)
        .map(|item| {
            (
                item.id,
                item.filename.clone(),
                item.fraction(),
                item.state == crate::downloads::DownloadState::Running,
            )
        })
        .collect();
    if items.is_empty() {
        empty(area, &palette, body, "Nothing downloaded yet.");
        return;
    }
    for (index, (download_id, name, fraction, running)) in items.iter().enumerate() {
        let row = Rect::from_min_size(
            pos2(body.min.x, body.min.y + index as f32 * 34.0),
            vec2(body.width(), 32.0),
        );
        if row.max.y > body.max.y + 4.0 {
            break;
        }
        let response = area.interact(
            row,
            id.with(("download", download_id)),
            if live { Sense::click() } else { Sense::hover() },
        );
        let hover = glass::ease_out(area.ctx().animate_bool_with_time(
            response.id.with("hover"),
            live && response.hovered(),
            0.12,
        ));
        if hover > 0.0 {
            area.painter().rect_filled(
                row,
                CornerRadius::same(8),
                palette.surface_hover.gamma_multiply(hover),
            );
        }
        icons::draw_icon(
            area.painter(),
            Rect::from_center_size(pos2(row.min.x + 14.0, row.center().y), vec2(14.0, 14.0)),
            if *running {
                Icon::Download
            } else {
                Icon::Check
            },
            if *running {
                palette.accent
            } else {
                palette.text_muted
            },
        );
        area.painter().text(
            pos2(row.min.x + 28.0, row.min.y + 11.0),
            Align2::LEFT_CENTER,
            fit(
                area.painter(),
                name,
                &FontId::proportional(12.5),
                row.width() - 36.0,
            ),
            FontId::proportional(12.5),
            theme::mix(palette.text_muted, palette.text, hover.max(0.4)),
        );
        if let Some(fraction) = fraction
            && *running
        {
            let track = Rect::from_min_size(
                pos2(row.min.x + 28.0, row.max.y - 8.0),
                vec2(row.width() - 36.0, 3.0),
            );
            area.painter()
                .rect_filled(track, CornerRadius::same(2), palette.border);
            area.painter().rect_filled(
                Rect::from_min_size(track.min, vec2(track.width() * fraction, track.height())),
                CornerRadius::same(2),
                palette.accent,
            );
        }
        if live && response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
            actions.push(UiAction::OpenDownloads);
        }
    }
}

fn draw_now_playing(
    area: &mut Ui,
    chrome: &mut ChromeContext,
    body: Rect,
    live: bool,
    id: Id,
    actions: &mut Vec<UiAction>,
) {
    let palette = chrome.palette;
    let media = chrome.media;
    if media.is_idle() {
        empty(area, &palette, body, "Nothing playing.");
        return;
    }
    let controls = 108.0_f32.min(body.width() * 0.5);
    let text_width = (body.width() - controls - 8.0).max(20.0);
    area.painter().text(
        pos2(body.min.x, body.min.y + 9.0),
        Align2::LEFT_CENTER,
        fit(
            area.painter(),
            &media.title,
            &FontId::proportional(13.0),
            text_width,
        ),
        FontId::proportional(13.0),
        palette.text,
    );
    area.painter().text(
        pos2(body.min.x, body.min.y + 25.0),
        Align2::LEFT_CENTER,
        fit(
            area.painter(),
            &media.artist,
            &FontId::proportional(11.5),
            text_width,
        ),
        FontId::proportional(11.5),
        palette.text_muted,
    );
    if media.duration > 0.0 {
        let track =
            Rect::from_min_size(pos2(body.min.x, body.max.y - 5.0), vec2(body.width(), 3.0));
        area.painter()
            .rect_filled(track, CornerRadius::same(2), palette.border);
        let played = (media.position / media.duration).clamp(0.0, 1.0) as f32;
        area.painter().rect_filled(
            Rect::from_min_size(track.min, vec2(track.width() * played, track.height())),
            CornerRadius::same(2),
            palette.accent,
        );
    }

    use servo::MediaSessionActionType;
    let buttons = [
        (Icon::SkipBack, MediaSessionActionType::PreviousTrack),
        (
            if media.playing {
                Icon::Pause
            } else {
                Icon::Play
            },
            if media.playing {
                MediaSessionActionType::Pause
            } else {
                MediaSessionActionType::Play
            },
        ),
        (Icon::SkipForward, MediaSessionActionType::NextTrack),
    ];
    for (index, (icon, action)) in buttons.into_iter().enumerate() {
        let centre = pos2(
            body.max.x - controls + controls * (0.18 + 0.32 * index as f32),
            body.min.y + 17.0,
        );
        let hit = Rect::from_center_size(centre, vec2(28.0, 28.0));
        let response = area.interact(
            hit,
            id.with(("transport", index)),
            if live { Sense::click() } else { Sense::hover() },
        );
        if live && response.hovered() {
            area.painter()
                .rect_filled(hit, CornerRadius::same(8), palette.surface_hover);
        }
        icons::draw_icon(
            area.painter(),
            Rect::from_center_size(centre, vec2(15.0, 15.0)),
            icon,
            palette.text,
        );
        if live && response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
            actions.push(UiAction::MediaAction(action));
        }
    }
}

fn draw_notes(area: &mut Ui, chrome: &mut ChromeContext, body: Rect, live: bool, id: Id) {
    let palette = chrome.palette;
    if !live {
        let note = chrome.library.note_mut().clone();
        let shown = if note.trim().is_empty() {
            "A place to put something down.".to_owned()
        } else {
            note
        };
        area.painter().text(
            body.min + vec2(0.0, 2.0),
            Align2::LEFT_TOP,
            crate::ui::ellipsize(&shown, 200),
            FontId::proportional(12.5),
            palette.text_muted,
        );
        return;
    }
    let mut inner = area.new_child(egui::UiBuilder::new().max_rect(body).id_salt(id));
    let editor = TextEdit::multiline(chrome.library.note_mut())
        .frame(Frame::NONE)
        .font(FontId::proportional(12.5))
        .text_color(palette.text)
        .hint_text(RichText::new("A place to put something down.").color(palette.text_muted))
        .desired_width(body.width())
        .desired_rows(((body.height() / 16.0) as usize).max(1));
    if inner.add(editor).changed() {
        chrome.library.changed();
    }
}

fn draw_tasks(area: &mut Ui, chrome: &mut ChromeContext, body: Rect, live: bool, id: Id) {
    let palette = chrome.palette;
    const ROW: f32 = 26.0;
    let field = 28.0;
    let list = Rect::from_min_max(
        body.min,
        pos2(body.max.x, (body.max.y - field).max(body.min.y)),
    );
    let capacity = ((list.height() / ROW).floor() as usize).max(1);

    let tasks: Vec<(String, bool)> = chrome
        .library
        .tasks
        .iter()
        .take(capacity)
        .map(|task| (task.text.clone(), task.done))
        .collect();
    if tasks.is_empty() {
        area.painter().text(
            pos2(list.min.x, list.min.y + 10.0),
            Align2::LEFT_CENTER,
            "Nothing to do. Enviable.",
            FontId::proportional(11.5),
            palette.text_muted,
        );
    }

    let mut toggled = None;
    let mut removed = None;
    for (index, (text, done)) in tasks.iter().enumerate() {
        let row = Rect::from_min_size(
            pos2(list.min.x, list.min.y + index as f32 * ROW),
            vec2(list.width(), ROW - 2.0),
        );
        let response = area.interact(
            row,
            id.with(("task", index)),
            if live { Sense::click() } else { Sense::hover() },
        );
        let hover = glass::ease_out(area.ctx().animate_bool_with_time(
            response.id.with("hover"),
            live && response.hovered(),
            0.12,
        ));
        if hover > 0.0 {
            area.painter().rect_filled(
                row,
                CornerRadius::same(7),
                palette.surface_hover.gamma_multiply(hover),
            );
        }
        let box_rect =
            Rect::from_center_size(pos2(row.min.x + 9.0, row.center().y), vec2(14.0, 14.0));
        if *done {
            area.painter()
                .rect_filled(box_rect, CornerRadius::same(4), palette.accent);
            icons::draw_icon(
                area.painter(),
                box_rect.shrink(2.5),
                Icon::Check,
                Color32::WHITE,
            );
        } else {
            area.painter().rect_stroke(
                box_rect,
                CornerRadius::same(4),
                Stroke::new(1.0_f32, palette.border),
                StrokeKind::Inside,
            );
        }
        area.painter().text(
            pos2(row.min.x + 24.0, row.center().y),
            Align2::LEFT_CENTER,
            fit(
                area.painter(),
                text,
                &FontId::proportional(12.5),
                row.width() - 46.0,
            ),
            FontId::proportional(12.5),
            if *done {
                palette.text_muted
            } else {
                palette.text
            },
        );
        // A done line still says so; a struck-through one says it and stops
        // asking to be read.
        if *done {
            let width = fit(
                area.painter(),
                text,
                &FontId::proportional(12.5),
                row.width() - 46.0,
            );
            let extent = area
                .painter()
                .layout_no_wrap(width, FontId::proportional(12.5), Color32::WHITE)
                .rect
                .width();
            area.painter().line_segment(
                [
                    pos2(row.min.x + 24.0, row.center().y),
                    pos2(row.min.x + 24.0 + extent, row.center().y),
                ],
                Stroke::new(1.0_f32, palette.text_muted.gamma_multiply(0.8)),
            );
        }
        if live && hover > 0.0 {
            let bin =
                Rect::from_center_size(pos2(row.max.x - 10.0, row.center().y), vec2(16.0, 16.0));
            icons::draw_icon(
                area.painter(),
                bin.shrink(3.0),
                Icon::Close,
                palette.text_muted,
            );
            if area
                .interact(bin, id.with(("task_remove", index)), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand)
                .clicked()
            {
                removed = Some(index);
            }
        }
        if live && response.on_hover_cursor(CursorIcon::PointingHand).clicked() && removed.is_none()
        {
            toggled = Some(index);
        }
    }

    if live {
        let entry = Rect::from_min_size(
            pos2(body.min.x, body.max.y - field + 4.0),
            vec2(body.width(), field - 6.0),
        );
        let mut inner = area.new_child(
            egui::UiBuilder::new()
                .max_rect(entry)
                .id_salt(id)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let editor = TextEdit::singleline(&mut chrome.browser.newtab_task_draft)
            .frame(Frame::NONE)
            .font(FontId::proportional(12.5))
            .text_color(palette.text)
            .vertical_align(egui::Align::Center)
            .hint_text(RichText::new("Add something…").color(palette.text_muted))
            .desired_width(entry.width());
        let response = inner.add(editor);
        area.painter().line_segment(
            [
                pos2(entry.min.x, entry.min.y - 2.0),
                pos2(entry.max.x, entry.min.y - 2.0),
            ],
            Stroke::new(1.0_f32, palette.border),
        );
        if response.lost_focus() && inner.input(|input| input.key_pressed(Key::Enter)) {
            let draft = std::mem::take(&mut chrome.browser.newtab_task_draft);
            chrome.library.add_task(&draft);
            response.request_focus();
        }
    }

    if let Some(index) = removed {
        chrome.library.remove_task(index);
    } else if let Some(index) = toggled {
        chrome.library.toggle_task(index);
    }
}

fn draw_workspaces(
    area: &mut Ui,
    chrome: &mut ChromeContext,
    body: Rect,
    live: bool,
    id: Id,
    actions: &mut Vec<UiAction>,
) {
    let palette = chrome.palette;
    let active = chrome.browser.active_workspace;
    let spaces: Vec<(String, usize)> = chrome
        .browser
        .workspaces
        .iter()
        .map(|space| (space.name.clone(), space.tabs.len()))
        .collect();
    for (index, (name, count)) in spaces.iter().enumerate() {
        let row = Rect::from_min_size(
            pos2(body.min.x, body.min.y + index as f32 * 28.0),
            vec2(body.width(), 26.0),
        );
        if row.max.y > body.max.y + 4.0 {
            break;
        }
        let response = area.interact(
            row,
            id.with(("workspace", index)),
            if live { Sense::click() } else { Sense::hover() },
        );
        let hover = glass::ease_out(area.ctx().animate_bool_with_time(
            response.id.with("hover"),
            live && response.hovered(),
            0.12,
        ));
        if hover > 0.0 || index == active {
            area.painter().rect_filled(
                row,
                CornerRadius::same(7),
                if index == active {
                    palette.active
                } else {
                    palette.surface_hover.gamma_multiply(hover)
                },
            );
        }
        area.painter().circle_filled(
            pos2(row.min.x + 10.0, row.center().y),
            4.0,
            theme::workspace_color(index),
        );
        area.painter().text(
            pos2(row.min.x + 22.0, row.center().y),
            Align2::LEFT_CENTER,
            fit(
                area.painter(),
                name,
                &FontId::proportional(12.5),
                row.width() - 52.0,
            ),
            FontId::proportional(12.5),
            palette.text,
        );
        area.painter().text(
            pos2(row.max.x - 8.0, row.center().y),
            Align2::RIGHT_CENTER,
            count.to_string(),
            FontId::proportional(11.0),
            palette.text_muted,
        );
        if live && response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
            actions.push(UiAction::SelectWorkspace(index));
        }
    }
}

fn draw_mark(area: &mut Ui, palette: &Palette, rect: Rect, ink: &Ink) {
    let height = (rect.height() * 0.44).clamp(20.0, 76.0);
    let color = if ink.photo {
        Color32::from_white_alpha(210)
    } else if palette.dark {
        Color32::from_white_alpha(200)
    } else {
        theme::mix(palette.text, palette.accent, 0.35)
    };
    crate::ui::draw_zervo_mark(area.painter(), rect.center(), height, color);
}

// ── Small pieces every card uses ───────────────────────────────────────────

/// One row of a list card: a letter badge, a title, and a hover fill.
#[allow(clippy::too_many_arguments)]
fn list_row(
    area: &mut Ui,
    palette: &Palette,
    row: Rect,
    id: Id,
    index: usize,
    title: &str,
    url: &str,
    live: bool,
) -> egui::Response {
    let response = area.interact(
        row,
        id.with(("row", index)),
        if live { Sense::click() } else { Sense::hover() },
    );
    let hover = glass::ease_out(area.ctx().animate_bool_with_time(
        response.id.with("hover"),
        live && response.hovered(),
        0.12,
    ));
    if hover > 0.0 {
        area.painter().rect_filled(
            row,
            CornerRadius::same(8),
            palette.surface_hover.gamma_multiply(hover),
        );
    }
    let badge = Rect::from_center_size(pos2(row.min.x + 14.0, row.center().y), vec2(20.0, 20.0));
    area.painter().rect_filled(
        badge,
        CornerRadius::same(6),
        palette.surface_hover.gamma_multiply(0.9),
    );
    area.painter().text(
        badge.center(),
        Align2::CENTER_CENTER,
        crate::ui::initial(title, url),
        FontId::proportional(11.0),
        palette.text_muted,
    );
    area.painter().text(
        pos2(row.min.x + 30.0, row.center().y),
        Align2::LEFT_CENTER,
        fit(
            area.painter(),
            title,
            &FontId::proportional(12.5),
            row.width() - 66.0,
        ),
        FontId::proportional(12.5),
        theme::mix(palette.text_muted, palette.text, hover.max(0.55)),
    );
    if live {
        return response
            .on_hover_cursor(CursorIcon::PointingHand)
            .on_hover_text(url);
    }
    response
}

/// What a card says when it has nothing to show. Every card has one: a blank
/// rectangle looks broken, and "there is nothing here yet" does not.
fn empty(area: &mut Ui, palette: &Palette, body: Rect, message: &str) {
    area.painter().text(
        pos2(body.min.x, body.min.y + 10.0),
        Align2::LEFT_CENTER,
        fit(
            area.painter(),
            message,
            &FontId::proportional(11.5),
            body.width(),
        ),
        FontId::proportional(11.5),
        palette.text_muted,
    );
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
