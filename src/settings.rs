//! User settings, persisted as JSON in the platform config directory
//! (`~/Library/Application Support/Zervo/settings.json` on macOS).

use crate::l10n::text as tr;
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
/// What the halo around the content card is coloured with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HaloTint {
    /// The accent, so the card reads as lit by it.
    Accent,
    /// The chrome's own colour, so the halo is depth rather than colour.
    Chrome,
}

impl HaloTint {
    pub const ALL: [HaloTint; 2] = [HaloTint::Accent, HaloTint::Chrome];

    pub fn label(self) -> &'static str {
        match self {
            HaloTint::Accent => tr("options-accent"),
            HaloTint::Chrome => tr("options-chrome"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppIcon {
    Default,
    Transparent,
}

impl AppIcon {
    pub const ALL: [AppIcon; 2] = [AppIcon::Default, AppIcon::Transparent];

    pub fn label(&self) -> &'static str {
        match self {
            AppIcon::Default => tr("options-default"),
            AppIcon::Transparent => tr("options-transparent"),
        }
    }
}

/// Where the sidebar lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Layout {
    /// The sidebar is docked alongside the page.
    #[serde(alias = "Bar")]
    Sidebar,
    /// The sidebar is revealed from the edge.
    FullPage,
}

impl Layout {
    pub const ALL: [Layout; 2] = [Layout::Sidebar, Layout::FullPage];

    pub fn label(self) -> &'static str {
        match self {
            Layout::Sidebar => tr("common-sidebar"),
            Layout::FullPage => tr("options-full-page"),
        }
    }

    pub fn note(self) -> &'static str {
        match self {
            Layout::Sidebar => tr("options-everything-in-one-column"),
            Layout::FullPage => tr("options-nothing-but-the-page-hover-the-edge"),
        }
    }

    pub fn is_sidebar(self) -> bool {
        self == Layout::Sidebar
    }

    pub fn toggled(self) -> Layout {
        match self {
            Layout::Sidebar => Layout::FullPage,
            Layout::FullPage => Layout::Sidebar,
        }
    }
}

/// Sections in the scrollable part of the sidebar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SidebarSection {
    Favourites,
    Pinned,
    Tabs,
}

impl SidebarSection {
    pub const ALL: [Self; 3] = [Self::Favourites, Self::Pinned, Self::Tabs];

    pub fn label(self) -> &'static str {
        match self {
            Self::Favourites => tr("common-favourites"),
            Self::Pinned => tr("sidebar-pinned-tabs"),
            Self::Tabs => tr("sidebar-open-tabs"),
        }
    }

    pub fn visible(self, settings: &Settings) -> bool {
        match self {
            Self::Favourites => settings.show_essentials,
            Self::Pinned => settings.show_pinned_section,
            Self::Tabs => settings.show_open_tabs,
        }
    }

    pub fn set_visible(self, settings: &mut Settings, visible: bool) {
        match self {
            Self::Favourites => settings.show_essentials = visible,
            Self::Pinned => settings.show_pinned_section = visible,
            Self::Tabs => settings.show_open_tabs = visible,
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

/// Which shape the new tab page opens in.
///
/// Turn 7a's own summary of the change: "Nothing was deleted. World clocks,
/// the note, the to-do list, downloads, now playing, the mark — all thirteen
/// still exist, still on the same grid, still draggable and resizable. The
/// change is which of them greets you."
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NewTabHome {
    /// The clock, one field, the session you actually left, and the three
    /// sites you use. A new tab is opened to go somewhere.
    Composed,
    /// The board of cards — `Tile::defaults()` and anything added to it —
    /// arrangeable and resizable. What the page has always been, one press
    /// away.
    Board,
}

impl NewTabHome {
    pub const ALL: [NewTabHome; 2] = [NewTabHome::Composed, NewTabHome::Board];

    pub fn label(self) -> &'static str {
        match self {
            NewTabHome::Composed => tr("options-composed"),
            NewTabHome::Board => tr("options-board"),
        }
    }

    pub fn note(self) -> &'static str {
        match self {
            NewTabHome::Composed => tr("sidebar-new-tab-options"),
            NewTabHome::Board => tr("options-every-card-on-the-grid-to-arrange"),
        }
    }
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
            NewTabBackground::Photo => tr("options-photo"),
            NewTabBackground::Aurora => tr("options-aurora"),
            NewTabBackground::Waves => tr("options-waves"),
            NewTabBackground::Particles => tr("options-particles"),
            NewTabBackground::Mesh => tr("options-mesh"),
            NewTabBackground::Grid => tr("options-grid"),
            NewTabBackground::Gradient => tr("options-gradient"),
            NewTabBackground::Plain => tr("options-plain"),
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

/// An arrangement somebody kept, under a name of their own.
///
/// The five presets are constants and always will be — they are the ones the
/// study argues for. This is the other half of the answer to "a theme is a
/// Rust constant": not only is every value settable, an arrangement can be put
/// somewhere and come back.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Saved {
    pub name: String,
    pub appearance: crate::theme::Appearance,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// BCP 47 locale code. An empty value follows the OS language.
    pub language: String,
    pub theme: ThemeMode,
    pub accent: AccentColor,
    pub homepage: String,
    pub search_engine: SearchEngine,
    pub new_tab_page: NewTabPage,
    pub new_tab_background: NewTabBackground,
    pub newtab_home: NewTabHome,
    /// Show tab close buttons always, instead of only on hover.
    pub always_show_tab_close: bool,
    pub show_forward_button: bool,
    pub show_reload_button: bool,
    /// Show global favourites in the sidebar.
    pub show_essentials: bool,
    pub sidebar_sections: [SidebarSection; 3],
    pub show_pinned_section: bool,
    pub show_open_tabs: bool,
    pub show_sidebar_tools: bool,
    pub show_sidebar_widgets: bool,
    /// Show tab counts next to workspace names.
    pub show_tab_counts: bool,
    /// Denser sidebar rows.
    pub compact_sidebar: bool,
    /// Where the chrome lives. Persisted, unlike the session-scoped flag it
    /// replaced: first run asks for it, so it has to outlive first run.
    pub layout: Layout,
    /// Whether the setup has been through once.
    ///
    /// Defaults to *true*, which reads backwards until you see what asks the
    /// question. A settings file that exists is a profile somebody has already
    /// configured, and showing them a setup wizard because a new field
    /// appeared in an update would be the update interrogating them. Only a
    /// profile with no settings file at all is a first run, and `load` is what
    /// knows that.
    pub seen_setup: bool,
    /// What the first workspace is called, and which of `WORKSPACE_COLORS` it
    /// takes.
    ///
    /// Workspaces themselves are session state: `BrowserState` builds one at
    /// startup and nothing writes it out. Asking somebody to name their first
    /// space and then forgetting it on the next launch would be worse than not
    /// asking, so the answer lives here and is applied to workspace zero when
    /// the browser starts.
    pub first_space: String,
    pub space_colour: usize,
    /// Arrangements the reader saved, in the order they saved them.
    pub saved: Vec<Saved>,
    /// Sidebar width in points, remembered across restarts when resized.
    pub sidebar_width: f32,
    /// Show favourites as tiles rather than a list — shorter, and it copes
    /// with a lot of them.
    pub favourites_grid: bool,
    /// Widgets in the sidebar shelf. Older versions shared this list with the top bar.
    #[serde(alias = "navbar_widgets")]
    pub sidebar_widgets: Vec<crate::dashboard::Placed>,
    /// Height of the sidebar widget shelf, in points. Zero is shut.
    pub sidebar_shelf_height: f32,
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
    /// A soft shadow around the content card.
    ///
    /// Off by default. The card fills nearly the whole window, so its shadow
    /// falls on the few points of chrome around it and reads as a dark seam
    /// tracing the edge rather than as depth — but that depends on how big the
    /// window is and what is behind it, so it is a choice rather than a rule.
    pub content_shadow: bool,
    /// A soft glow around the content card, and what colours it.
    pub content_halo: bool,
    pub content_halo_tint: HaloTint,
    /// How far each spreads, as a multiple of the shape they were drawn at
    /// before either was adjustable. One is what the shadow has always looked
    /// like, and the halo is tuned to match it rather than to its own scale.
    pub content_shadow_amount: f32,
    pub content_halo_amount: f32,
    /// Everything about how Zervo is built: which preset, where the seam
    /// falls, what a surface is made of, how it moves, how far the accent
    /// reaches. See [`crate::theme::Appearance`].
    ///
    /// The Solid/Frosted switch lives inside it — one setting for the whole
    /// application rather than one per group of things — and it is honoured by
    /// every arrangement, including the ones that ship opaque.
    pub appearance: crate::theme::Appearance,
    /// Where the Solid/Frosted switch used to live, on its own.
    ///
    /// Read once at load and folded into `appearance`, so a settings file
    /// written before every value moved into one arrangement does not quietly
    /// lose the reader's choice. Never written back out.
    #[serde(rename = "translucency", skip_serializing)]
    pub legacy_translucency: Option<crate::theme::Translucency>,
    /// Opacity of the chrome's card surfaces — the address pill, tab rows,
    /// settings sections, shelf widgets — 0.0..=1.0. Independent of
    /// the chrome's own opacity, and unlike it this one reaches zero: at zero the
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

fn sidebar_default_appearance() -> crate::theme::Appearance {
    use crate::theme::{Appearance, Preset, Seam, Spine};

    let native = cfg!(target_os = "macos");
    let preset = if native {
        Preset::LiquidGlass
    } else {
        Preset::Flat
    };
    Appearance {
        preset: native.then_some(preset),
        seam: Seam::Card,
        gap: 9.0,
        corners: if native {
            preset.appearance().corners
        } else {
            0.8
        },
        candy: 0.025,
        workspace_accent: true,
        spine: Spine::Nothing,
        ..preset.appearance()
    }
}

#[cfg(target_os = "macos")]
fn use_native_material(appearance: &mut crate::theme::Appearance) {
    use crate::theme::{Appearance, Preset, Seam};

    *appearance = Appearance {
        seam: if appearance.seam.closes_gap() {
            Seam::EdgeToEdge
        } else {
            Seam::Card
        },
        gap: appearance.gap,
        motion: appearance.motion,
        sweep: appearance.sweep,
        pill_progress: appearance.pill_progress,
        candy: appearance.candy,
        workspace_accent: appearance.workspace_accent,
        spine: appearance.spine,
        align_nav: appearance.align_nav,
        ..Preset::LiquidGlass.appearance()
    };
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: String::new(),
            theme: ThemeMode::Auto,
            accent: AccentColor::Lavender,
            homepage: "https://servo.org".to_owned(),
            search_engine: SearchEngine::DuckDuckGo,
            new_tab_page: NewTabPage::ZervoPage,
            new_tab_background: NewTabBackground::Grid,
            newtab_home: NewTabHome::Composed,
            always_show_tab_close: false,
            show_forward_button: true,
            show_reload_button: true,
            show_essentials: true,
            sidebar_sections: SidebarSection::ALL,
            show_pinned_section: true,
            show_open_tabs: true,
            show_sidebar_tools: true,
            show_sidebar_widgets: true,
            show_tab_counts: true,
            compact_sidebar: false,
            layout: Layout::Sidebar,
            seen_setup: true,
            first_space: String::new(),
            space_colour: 0,
            saved: Vec::new(),
            sidebar_width: crate::ui::SIDEBAR_DEFAULT_WIDTH,
            favourites_grid: false,
            sidebar_shelf_height: 0.0,
            sidebar_widgets: crate::dashboard::Placed::defaults(),
            user_agent_compat: true,
            downloads_auto: true,
            top_glow: 0.20,
            content_border: true,
            content_shadow: false,
            content_halo: false,
            content_halo_tint: HaloTint::Accent,
            content_shadow_amount: 1.0,
            content_halo_amount: 1.0,
            appearance: sidebar_default_appearance(),
            legacy_translucency: None,
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

impl Settings {
    pub fn reset_sidebar_sections(&mut self) {
        self.sidebar_sections = SidebarSection::ALL;
        for section in SidebarSection::ALL {
            section.set_visible(self, true);
        }
        self.show_sidebar_tools = true;
        self.show_sidebar_widgets = true;
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

// There was a `legacy_settings_path` here, read as a fallback so preferences
// written before a rename would carry over. It returned
// `config_dir()/Zervo/settings.json` — which is what `settings_path()` returns,
// because `data_dir()` above is `config_dir()?.join("Zervo")`. The two paths
// became the same the moment `data_dir()` was pointed at the config directory,
// and the fallback has been reading the file it had just failed to read ever
// since. Removed rather than repaired: whatever it once migrated from is not
// nameable from here any more.

pub fn load() -> Settings {
    let fresh = settings_path().is_none_or(|path| !path.exists());
    let mut settings: Settings = crate::store::load_or_default(settings_path());
    migrate(&mut settings);
    #[cfg(target_os = "macos")]
    use_native_material(&mut settings.appearance);
    if fresh {
        // No settings file, so nobody has been here before. The only honest
        // signal for it: there is no account, and nothing to ask.
        settings.seen_setup = false;
    }
    settings
}

/// Update built-in presets and carry forward older translucency settings.
fn migrate(settings: &mut Settings) {
    use crate::theme::{Preset, Translucency};

    if !SidebarSection::ALL
        .iter()
        .all(|section| settings.sidebar_sections.contains(section))
    {
        settings.sidebar_sections = SidebarSection::ALL;
    }

    if settings.appearance.preset == Some(Preset::LiquidGlass)
        && settings.appearance.translucency == Translucency::Frosted
        && settings.appearance.blur == 0.0
    {
        settings.appearance = Preset::LiquidGlass.appearance();
    }

    crate::dashboard::normalize_layout(&mut settings.sidebar_widgets);

    let Some(translucency) = settings.legacy_translucency.take() else {
        return;
    };
    // Profiles with the legacy top-level field predate this branch's
    // material. Interpret it against the preset those profiles started with.
    if settings.appearance == sidebar_default_appearance() {
        settings.appearance = Preset::Zervo.appearance();
    }
    if translucency != settings.appearance.translucency {
        settings.appearance.translucency = translucency;
        settings.appearance.customised();
    }
}

impl Settings {
    pub fn save(&self) {
        let _ = crate::store::save(settings_path(), self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Preset, Translucency};

    /// The one field that changed address. A settings file written before
    /// every value about how Zervo is built moved into one arrangement still
    /// carries `translucency` at the top level, and the reader's choice there
    /// has to survive the move.
    #[test]
    fn an_old_translucency_choice_is_carried_across() {
        for (chose, expected, custom) in [
            (Translucency::Solid, Translucency::Solid, true),
            // Already what the default arrangement uses, so it is not a
            // deviation from the preset and must not relabel it "Custom".
            (Translucency::Frosted, Translucency::Frosted, false),
        ] {
            let old =
                format!(r#"{{"theme":"Auto","accent":"Lavender","translucency":"{chose:?}"}}"#);
            let mut settings: Settings =
                serde_json::from_str(&old).expect("an old settings file still parses");
            assert_eq!(settings.legacy_translucency, Some(chose));
            migrate(&mut settings);
            assert_eq!(settings.appearance.translucency, expected);
            assert_eq!(settings.legacy_translucency, None, "read once, then gone");
            assert_eq!(
                settings.appearance.preset.is_none(),
                custom,
                "{chose:?}: whether this counts as the reader's own arrangement"
            );
        }
    }

    #[test]
    fn only_the_builtin_legacy_glass_preset_migrates() {
        for (preset, expected) in [
            (r#""LiquidGlass""#, Translucency::LiquidGlass),
            ("null", Translucency::Frosted),
        ] {
            let old = format!(
                r#"{{"appearance":{{"preset":{preset},"translucency":"Frosted","blur":0}}}}"#
            );
            let mut settings: Settings =
                serde_json::from_str(&old).expect("legacy glass settings parse");
            migrate(&mut settings);
            assert_eq!(settings.appearance.translucency, expected);
            assert_eq!(
                settings.appearance.blur,
                if expected == Translucency::LiquidGlass {
                    Preset::LiquidGlass.appearance().blur
                } else {
                    0.0
                }
            );
        }
    }

    /// And it is never written back out, or the next version would keep
    /// migrating a value that has already moved.
    #[test]
    fn the_old_field_is_not_written_back() {
        let written: serde_json::Value =
            serde_json::to_value(Settings::default()).expect("settings serialise");
        let top = written.as_object().expect("settings are an object");
        assert!(
            !top.contains_key("translucency"),
            "the old top-level field came back"
        );
        // It lives one level down now, which is the whole point of the move.
        assert!(top["appearance"]["translucency"].is_string());
    }

    #[test]
    fn the_sidebar_uses_the_platform_material_by_default() {
        let appearance = Settings::default().appearance;
        assert_eq!(appearance.seam, crate::theme::Seam::Card);
        assert_eq!(
            appearance.translucency,
            if cfg!(target_os = "macos") {
                Translucency::LiquidGlass
            } else {
                Translucency::Solid
            }
        );
        assert!(appearance.workspace_accent);
        assert!(Preset::ALL.contains(&Preset::Zervo));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_material_applies_to_existing_appearances() {
        let mut appearance = Preset::Flat.appearance();
        appearance.seam = crate::theme::Seam::OneSurface;
        appearance.gap = 11.0;
        appearance.workspace_accent = true;
        use_native_material(&mut appearance);
        assert_eq!(appearance.translucency, Translucency::LiquidGlass);
        assert_eq!(appearance.corners, Preset::LiquidGlass.appearance().corners);
        assert_eq!(appearance.seam, crate::theme::Seam::EdgeToEdge);
        assert_eq!(appearance.gap, 11.0);
        assert!(appearance.workspace_accent);
    }

    #[test]
    fn legacy_bar_profiles_use_sidebar_and_keep_widget_settings() {
        let mut settings: Settings = serde_json::from_str(r#"{
            "layout": "Bar",
            "sidebar_width": 280.0,
            "appearance": { "shelf": "Bar", "liquid": true },
            "navbar_widgets": [
                { "kind": "Clock", "size": { "w": { "Cells": 2 }, "h": { "Cells": 1 } }, "col": 1 },
                { "kind": "Transport", "size": { "w": { "Cells": 1 }, "h": { "Cells": 1 } }, "col": 0 }
            ]
        }"#).unwrap();
        migrate(&mut settings);
        assert_eq!(settings.layout, Layout::Sidebar);
        assert_eq!(settings.sidebar_width, 280.0);
        assert_eq!(settings.sidebar_widgets.len(), 2);
        assert_eq!(settings.sidebar_widgets[0].row, 1);
        assert_eq!(settings.sidebar_widgets[1].row, 0);
        let written = serde_json::to_value(settings).unwrap();
        assert!(written.get("navbar_widgets").is_none());
        assert!(written["appearance"].get("shelf").is_none());
        assert!(written["appearance"].get("liquid").is_none());
    }

    #[test]
    fn sidebar_layout_repairs_duplicate_sections_and_keeps_legacy_visibility() {
        let mut settings: Settings = serde_json::from_str(
            r#"{"show_essentials":false,"sidebar_sections":["Pinned","Pinned","Tabs"]}"#,
        )
        .unwrap();
        migrate(&mut settings);
        assert_eq!(settings.sidebar_sections, SidebarSection::ALL);
        assert!(!SidebarSection::Favourites.visible(&settings));
        settings.reset_sidebar_sections();
        assert!(SidebarSection::Favourites.visible(&settings));
    }
}

#[cfg(test)]
mod first_run_tests {
    use super::*;

    /// An update that adds a field must not turn into an interrogation.
    ///
    /// `seen_setup` defaults to true precisely so that a settings file which
    /// predates the setup — every existing profile — is left alone. Only the
    /// absence of a file is a first run, and that is decided in `load` rather
    /// than by the default.
    #[test]
    fn an_existing_profile_is_not_a_first_run() {
        let old = r#"{"theme":"Auto","accent":"Lavender"}"#;
        let settings: Settings = serde_json::from_str(old).expect("an old file still parses");
        assert!(
            settings.seen_setup,
            "a profile that predates the setup would be shown it"
        );
    }

    /// And the answers it collects are ones that survive being closed. The
    /// space name in particular: workspaces are session state, so if this were
    /// not persisted the wizard would ask for a name and forget it.
    #[test]
    fn the_setup_writes_only_fields_that_persist() {
        let written: serde_json::Value =
            serde_json::to_value(Settings::default()).expect("settings serialise");
        let top = written.as_object().expect("settings are an object");
        for key in [
            "appearance",
            "layout",
            "first_space",
            "space_colour",
            "seen_setup",
        ] {
            assert!(top.contains_key(key), "{key} is not written out");
        }
    }
}
