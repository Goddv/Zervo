//! Icon rendering. All chrome iconography comes from the Phosphor icon set
//! (https://phosphoricons.com, MIT), vendored as a font at
//! `assets/fonts/Phosphor.ttf` and registered with egui as its own font
//! family, so icons are drawn as text and stay crisp at any DPI.

use egui::{
    Align2, Color32, CornerRadius, CursorIcon, FontFamily, FontId, Painter, Rect, Response, Sense,
    Ui, vec2,
};

use crate::phosphor::glyph;
use crate::theme::Palette;

/// The egui font family the Phosphor face is registered under.
pub const PHOSPHOR_FAMILY: &str = "phosphor";

pub fn phosphor_font(size: f32) -> FontId {
    // `FontFamily::Name` holds an `Arc<str>`, so `.into()` on a `&str`
    // allocates — once per icon, per frame, across some sixty call sites in the
    // chrome. Cloning a shared one is a refcount bump instead.
    static FAMILY: std::sync::LazyLock<FontFamily> =
        std::sync::LazyLock::new(|| FontFamily::Name(PHOSPHOR_FAMILY.into()));
    FontId::new(size, FAMILY.clone())
}

#[derive(Clone, Copy, PartialEq)]
#[expect(dead_code)] // The full pack vocabulary; not every glyph is placed yet.
pub enum Icon {
    Back,
    Forward,
    Reload,
    Plus,
    Close,
    Gear,
    Globe,
    Search,
    Home,
    Download,
    Pin,
    Trash,
    Clock,
    Folder,
    FolderOpen,
    Check,
    CaretDown,
    Pause,
    Play,
    XCircle,
    CheckCircle,
    Palette,
    Sliders,
    Info,
    Browser,
    FileArrowDown,
    ExternalLink,
    Warning,
    Sidebar,
    Star,
    Lock,
    History,
    Extensions,
    Arrange,
    // ── The new tab page's vocabulary.
    Apps,
    Note,
    Todo,
    CheckSquare,
    Square,
    Image,
    Mountains,
    Shuffle,
    World,
    Music,
    Stack,
    Pencil,
    DragHandle,
    Resize,
    Calendar,
    TrendUp,
    Recent,
    Reset,
    Minus,
    Link,
    SkipBack,
    SkipForward,
}

impl Icon {
    pub fn glyph(self) -> &'static str {
        match self {
            Icon::Star => glyph::STAR,
            Icon::Lock => glyph::LOCK,
            Icon::History => glyph::CLOCK,
            Icon::Extensions => glyph::PUZZLE,
            Icon::Arrange => glyph::MAGICWAND,
            Icon::Back => glyph::ARROWLEFT,
            Icon::Forward => glyph::ARROWRIGHT,
            Icon::Reload => glyph::RELOAD,
            Icon::Plus => glyph::PLUS,
            Icon::Close => glyph::CLOSE,
            Icon::Gear => glyph::GEAR,
            Icon::Globe => glyph::GLOBE,
            Icon::Search => glyph::SEARCH,
            Icon::Home => glyph::HOUSE,
            Icon::Download => glyph::DOWNLOAD,
            Icon::Pin => glyph::PIN,
            Icon::Trash => glyph::TRASH,
            Icon::Clock => glyph::CLOCK,
            Icon::Folder => glyph::FOLDER,
            Icon::FolderOpen => glyph::FOLDEROPEN,
            Icon::Check => glyph::CHECK,
            Icon::CaretDown => glyph::CARETDOWN,
            Icon::Pause => glyph::PAUSE,
            Icon::Play => glyph::PLAY,
            Icon::XCircle => glyph::XCIRCLE,
            Icon::CheckCircle => glyph::CHECKCIRCLE,
            Icon::Palette => glyph::PALETTE,
            Icon::Sliders => glyph::SLIDERS,
            Icon::Info => glyph::INFO,
            Icon::Browser => glyph::BROWSER,
            Icon::FileArrowDown => glyph::FILEARROWDOWN,
            Icon::ExternalLink => glyph::EXTERNALLINK,
            Icon::Warning => glyph::WARNING,
            Icon::Sidebar => glyph::SIDEBAR,
            Icon::Apps => glyph::SQUARESFOUR,
            Icon::Note => glyph::NOTEPENCIL,
            Icon::Todo => glyph::LISTCHECKS,
            Icon::CheckSquare => glyph::CHECKSQUARE,
            Icon::Square => glyph::SQUARE,
            Icon::Image => glyph::IMAGE,
            Icon::Mountains => glyph::MOUNTAINS,
            Icon::Shuffle => glyph::SHUFFLE,
            Icon::World => glyph::GLOBEHEMISPHERE,
            Icon::Music => glyph::MUSICNOTES,
            Icon::Stack => glyph::STACK,
            Icon::Pencil => glyph::PENCIL,
            Icon::DragHandle => glyph::DOTSSIX,
            Icon::Resize => glyph::CORNERSOUT,
            Icon::Calendar => glyph::CALENDAR,
            Icon::TrendUp => glyph::TRENDUP,
            Icon::Recent => glyph::CLOCKCOUNTER,
            Icon::Reset => glyph::ARROWCOUNTER,
            Icon::Minus => glyph::MINUS,
            Icon::Link => glyph::LINK,
            Icon::SkipBack => glyph::SKIPBACK,
            Icon::SkipForward => glyph::SKIPFORWARD,
        }
    }
}

/// Draw an icon centred in `rect`, sized to the rect's height.
pub fn draw_icon(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        icon.glyph(),
        phosphor_font(rect.height()),
        color,
    );
}

/// A ghost icon button: rounded hover fill, palette-derived glyph color,
/// pointer cursor, dimmed when disabled, with a press micro-interaction.
pub fn icon_button(
    ui: &mut Ui,
    icon: Icon,
    size: f32,
    palette: &Palette,
    enabled: bool,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        vec2(size + 12.0, size + 8.0),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );

    let hover_t = crate::glass::ease_out(ui.ctx().animate_bool_with_time(
        response.id.with("hover"),
        enabled && response.hovered(),
        0.12,
    ));
    // Presses feel immediate; only the release settles.
    let is_down = enabled && response.is_pointer_button_down_on();
    let press_t = crate::glass::ease_out(ui.ctx().animate_bool_with_time(
        response.id.with("press"),
        is_down,
        if is_down { 0.03 } else { 0.12 },
    ));

    let painter = ui.painter();
    if hover_t > 0.0 {
        painter.rect_filled(
            rect,
            CornerRadius::same(8),
            palette.surface_hover.gamma_multiply(hover_t),
        );
    }
    let color = if !enabled {
        palette.text_muted.gamma_multiply(0.35)
    } else {
        crate::theme::mix(palette.text_muted, palette.text, hover_t)
    };

    // Press squish: the glyph scales down slightly while held.
    let scale = 1.0 - 0.10 * press_t;
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        icon.glyph(),
        phosphor_font(size * scale),
        color,
    );

    if enabled {
        response.clone().on_hover_cursor(CursorIcon::PointingHand);
    }
    response
}
