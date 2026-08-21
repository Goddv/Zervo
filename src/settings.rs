//! User settings, persisted as JSON in the platform config directory
//! (`~/Library/Application Support/Zervo/settings.json` on macOS).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::theme::{AccentColor, ThemeMode};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchEngine {
    DuckDuckGo,
    Google,
    Bing,
    Startpage,
}

impl SearchEngine {
    pub const ALL: [SearchEngine; 4] = [
        SearchEngine::DuckDuckGo,
        SearchEngine::Google,
        SearchEngine::Bing,
        SearchEngine::Startpage,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            SearchEngine::DuckDuckGo => "DuckDuckGo",
            SearchEngine::Google => "Google",
            SearchEngine::Bing => "Bing",
            SearchEngine::Startpage => "Startpage",
        }
    }

    /// Build a search URL from an already-percent-escaped query.
    pub fn query_url(&self, escaped_query: &str) -> String {
        match self {
            SearchEngine::DuckDuckGo => {
                format!("https://duckduckgo.com/?q={escaped_query}")
            },
            SearchEngine::Google => {
                format!("https://www.google.com/search?q={escaped_query}")
            },
            SearchEngine::Bing => format!("https://www.bing.com/search?q={escaped_query}"),
            SearchEngine::Startpage => {
                format!("https://www.startpage.com/sp/search?query={escaped_query}")
            },
        }
    }
}

/// Which app icon Zervo wears in the Dock.
///
/// Both variants are composed from the same layered artwork authored in
/// Apple's Icon Composer (`assets/icon/Zervo.icon`); the transparent variant
/// is that document's translucency setting made visible.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppIcon {
    Default,
    Transparent,
}

impl AppIcon {
    pub const ALL: [AppIcon; 2] = [AppIcon::Default, AppIcon::Transparent];

    pub fn label(&self) -> &'static str {
        match self {
            AppIcon::Default => "Default",
            AppIcon::Transparent => "Transparent",
        }
    }
}

/// What a freshly opened tab shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NewTabPage {
    /// The internal zervo://newtab page.
    #[serde(alias = "Blank")]
    ZervoPage,
    Homepage,
}

/// Background treatment of the zervo://newtab page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NewTabBackground {
    /// A photograph, from whichever source `wallpaper_source` names.
    Photo,
    /// Slow ambient drift of accent-tinted light blobs.
    Aurora,
    /// Flowing sine bands.
    Waves,
    /// Drifting points of light.
    Particles,
    /// Static multi-point colour wash.
    Mesh,
    /// Faint geometric grid.
    Grid,
    /// Simple vertical fade.
    Gradient,
    Plain,
}

impl NewTabBackground {
    pub const ALL: [NewTabBackground; 8] = [
        NewTabBackground::Photo,
        NewTabBackground::Aurora,
        NewTabBackground::Waves,
        NewTabBackground::Particles,
        NewTabBackground::Mesh,
        NewTabBackground::Grid,
        NewTabBackground::Gradient,
        NewTabBackground::Plain,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            NewTabBackground::Photo => "Photo",
            NewTabBackground::Aurora => "Aurora",
            NewTabBackground::Waves => "Waves",
            NewTabBackground::Particles => "Particles",
            NewTabBackground::Mesh => "Mesh",
            NewTabBackground::Grid => "Grid",
            NewTabBackground::Gradient => "Gradient",
            NewTabBackground::Plain => "Plain",
        }
    }

    /// True for backgrounds that animate and therefore need timed repaints.
    pub fn animated(&self) -> bool {
        matches!(
            self,
            NewTabBackground::Aurora | NewTabBackground::Waves | NewTabBackground::Particles
        )
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemeMode,
    pub accent: AccentColor,
    pub homepage: String,
    pub search_engine: SearchEngine,
    pub new_tab_page: NewTabPage,
    pub new_tab_background: NewTabBackground,
    /// Show tab close buttons always, instead of only on hover.
    pub always_show_tab_close: bool,
    pub show_forward_button: bool,
    pub show_reload_button: bool,
    /// Show the pinned-tab essentials grid.
    pub show_essentials: bool,
    /// Show tab counts next to workspace names.
    pub show_tab_counts: bool,
    /// Denser sidebar rows.
    pub compact_sidebar: bool,
    /// Reveal a collapsed sidebar when the pointer nears the window edge.
    pub sidebar_autohide: bool,
    /// Sidebar width in points, remembered across restarts when resized.
    pub sidebar_width: f32,
    /// Show favourites as tiles rather than a list — shorter, and it copes
    /// with a lot of them.
    pub favourites_grid: bool,
    /// Width of the centred address pill in the navigation bar, in points.
    pub address_pill_width: f32,
    /// What sits left and right of the address pill, in order.
    pub navbar_left: Vec<crate::ui::NavItem>,
    pub navbar_right: Vec<crate::ui::NavItem>,
    /// Widgets placed in the navigation bar's shelf, in order.
    pub navbar_widgets: Vec<crate::dashboard::Placed>,
    /// Height of the navigation bar, in points. Anything above the row the
    /// controls need is free space, kept for widgets that will live there.
    pub navbar_height: f32,
    /// Present as plain Firefox rather than as Servo. Servo's own user agent
    /// already claims Firefox 140, but keeps a `Servo/x.y` token and omits the
    /// `Gecko/20100101` one, and enough sites match on those to matter.
    pub user_agent_compat: bool,
    /// Save files without asking where.
    pub downloads_auto: bool,
    /// Strength of the glow strip across the top of the window, 0.0..=1.0.
    /// Zero turns it off entirely.
    pub top_glow: f32,
    /// Accent-tinted outline around the web content card.
    pub content_border: bool,
    /// Opacity of the chrome (sidebar and surrounds), 0.35..=1.0. Below 1.0
    /// the window is translucent; the web content itself stays opaque.
    pub chrome_opacity: f32,
    /// How much comes through every surface the material draws — the cards,
    /// the menus, the shelf, the new tab page. One setting for the whole
    /// application rather than one per group of things.
    pub translucency: crate::theme::Translucency,
    /// How far a material blurs what is behind it, for materials that blur at
    /// all. Ignored by any that does not.
    pub blur: crate::theme::Blur,
    /// Opacity of the chrome's card surfaces — the address pill, tab rows,
    /// settings sections, shelf widgets — 0.0..=1.0. Independent of
    /// `chrome_opacity`, and unlike it this one reaches zero: at zero the
    /// cards are gone and only their text and icons remain. Surfaces that
    /// float over a web page or a photograph ignore it, because a modal you
    /// cannot see is a fault rather than a look.
    /// Trackpad swipes bound to chrome actions.
    pub gestures: crate::gestures::Gestures,

    // ── The new tab page.
    /// The cards on the page, and where each one sits. Arranged by dragging
    /// them rather than set here, which is why there is no list of toggles.
    pub newtab_tiles: Vec<crate::newtab::Tile>,
    /// The cities on the world-clocks card.
    pub newtab_world_clocks: Vec<crate::newtab::Zone>,
    /// Overrides the time-of-day greeting when non-empty.
    pub newtab_message: String,
    /// Where the page's photograph comes from.
    pub wallpaper_source: crate::wallpaper::Source,
    /// How often a new one is fetched.
    pub wallpaper_cadence: crate::wallpaper::Cadence,
    /// How far the photograph is veiled so the cards read on it, 0..=1. Never
    /// zero: a card on a bright photograph with no veil under it cannot be
    /// read, whatever the photograph.
    pub wallpaper_dim: f32,

    /// Dock icon variant.
    pub app_icon: AppIcon,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeMode::Auto,
            accent: AccentColor::Lavender,
            homepage: "https://servo.org".to_owned(),
            search_engine: SearchEngine::DuckDuckGo,
            new_tab_page: NewTabPage::ZervoPage,
            new_tab_background: NewTabBackground::Aurora,
            always_show_tab_close: false,
            show_forward_button: true,
            show_reload_button: true,
            show_essentials: true,
            show_tab_counts: true,
            compact_sidebar: false,
            sidebar_autohide: true,
            sidebar_width: crate::ui::SIDEBAR_DEFAULT_WIDTH,
            favourites_grid: false,
            address_pill_width: crate::ui::ADDRESS_PILL_DEFAULT_WIDTH,
            navbar_height: crate::ui::NAVBAR_DEFAULT_HEIGHT,
            navbar_left: crate::ui::NavItem::default_left(),
            navbar_right: crate::ui::NavItem::default_right(),
            navbar_widgets: crate::dashboard::Placed::defaults(),
            user_agent_compat: true,
            downloads_auto: true,
            top_glow: 1.0,
            content_border: true,
            chrome_opacity: 0.85,
            translucency: crate::theme::Translucency::Frosted,
            blur: crate::theme::Blur::Medium,
            gestures: crate::gestures::Gestures::default(),
            newtab_tiles: crate::newtab::Tile::defaults(),
            newtab_world_clocks: crate::newtab::Zone::defaults(),
            newtab_message: String::new(),
            wallpaper_source: crate::wallpaper::Source::Commons,
            wallpaper_cadence: crate::wallpaper::Cadence::Daily,
            wallpaper_dim: 0.55,
            app_icon: AppIcon::Default,
        }
    }
}

/// Where Zervo keeps its own files. Servo is pointed here too, for its cookie
/// jar, auth cache and HSTS list.
pub fn data_dir() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("Zervo"))
}

fn settings_path() -> Option<PathBuf> {
    Some(data_dir()?.join("settings.json"))
}

/// Pre-rename location, still read once so existing preferences carry over.
fn legacy_settings_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("Zervo").join("settings.json"))
}

pub fn load() -> Settings {
    let read = |path: Option<PathBuf>| -> Option<Settings> {
        let contents = std::fs::read_to_string(path?).ok()?;
        serde_json::from_str(&contents).ok()
    };
    read(settings_path())
        .or_else(|| read(legacy_settings_path()))
        .unwrap_or_default()
}

impl Settings {
    pub fn save(&self) {
        let Some(path) = settings_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(error) = std::fs::write(&path, json) {
                    log::warn!("Failed to save settings: {error}");
                }
            },
            Err(error) => log::warn!("Failed to serialize settings: {error}"),
        }
    }
}
