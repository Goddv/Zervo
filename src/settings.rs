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
            HaloTint::Accent => "Accent",
            HaloTint::Chrome => "Chrome",
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
            AppIcon::Default => "Default",
            AppIcon::Transparent => "Transparent",
        }
    }
}

/// Where the chrome lives.
///
/// Three, not two. All three are always reachable — ⌘S walks them — so what
/// first run asks for, and what this setting holds, is only where somebody
/// starts rather than what they are stuck with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Layout {
    /// Everything in one column down the side.
    Sidebar,
    /// The address pill centred across the top; tabs hidden.
    Bar,
    /// Nothing but the page. The chrome is what the edge reveals.
    FullPage,
}

impl Layout {
    pub const ALL: [Layout; 3] = [Layout::Sidebar, Layout::Bar, Layout::FullPage];

    pub fn label(self) -> &'static str {
        match self {
            Layout::Sidebar => "Sidebar",
            Layout::Bar => "Top bar",
            Layout::FullPage => "Full page",
        }
    }

    pub fn note(self) -> &'static str {
        match self {
            Layout::Sidebar => "Everything in one column.",
            Layout::Bar => "Address pill centred, tabs hidden.",
            Layout::FullPage => "Nothing but the page; hover the edge.",
        }
    }

    /// Whether the sidebar is the docked chrome. False for both of the others,
    /// which is what makes the left edge hot.
    pub fn is_sidebar(self) -> bool {
        self == Layout::Sidebar
    }

    /// The next one along, for the shortcut that walks all three.
    pub fn next(self) -> Layout {
        match self {
            Layout::Sidebar => Layout::Bar,
            Layout::Bar => Layout::FullPage,
            Layout::FullPage => Layout::Sidebar,
        }
    }

    /// The other docked one, for the sidebar glyph — which has always been a
    /// two-way switch and should stay one.
    pub fn toggled(self) -> Layout {
        match self {
            Layout::Sidebar => Layout::Bar,
            Layout::Bar | Layout::FullPage => Layout::Sidebar,
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
            NewTabHome::Composed => "Composed",
            NewTabHome::Board => "Board",
        }
    }

    pub fn note(self) -> &'static str {
        match self {
            NewTabHome::Composed => {
                "The clock, the field, and where you left off. The board is one press away."
            },
            NewTabHome::Board => "Every card on the grid, to arrange as you like.",
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
    /// Show the pinned-tab essentials grid.
    pub show_essentials: bool,
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
    /// Reveal a collapsed sidebar when the pointer nears the window edge.
    ///
    /// Full-page mode ignores this and always reveals: there is no other
    /// chrome, so an edge that did nothing would be a browser with no way back.
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
    /// Height of the sidebar's own widget shelf, in points. Zero is shut.
    ///
    /// Its own number rather than the bar's: the two shelves hold the same
    /// widgets at the same cell, but one is twelve columns across the top of
    /// the window and the other is one column down the side, so how far open
    /// each is left is not the same question.
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

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeMode::Auto,
            accent: AccentColor::Lavender,
            homepage: "https://servo.org".to_owned(),
            search_engine: SearchEngine::DuckDuckGo,
            new_tab_page: NewTabPage::ZervoPage,
            new_tab_background: NewTabBackground::Aurora,
            newtab_home: NewTabHome::Composed,
            always_show_tab_close: false,
            show_forward_button: true,
            show_reload_button: true,
            show_essentials: true,
            show_tab_counts: true,
            compact_sidebar: false,
            layout: Layout::Sidebar,
            seen_setup: true,
            first_space: String::new(),
            space_colour: 0,
            saved: Vec::new(),
            sidebar_autohide: true,
            sidebar_width: crate::ui::SIDEBAR_DEFAULT_WIDTH,
            favourites_grid: false,
            address_pill_width: crate::ui::ADDRESS_PILL_DEFAULT_WIDTH,
            navbar_height: crate::ui::NAVBAR_DEFAULT_HEIGHT,
            sidebar_shelf_height: 0.0,
            navbar_left: crate::ui::NavItem::default_left(),
            navbar_right: crate::ui::NavItem::default_right(),
            navbar_widgets: crate::dashboard::Placed::defaults(),
            user_agent_compat: true,
            downloads_auto: true,
            top_glow: 1.0,
            content_border: true,
            content_shadow: false,
            content_halo: false,
            content_halo_tint: HaloTint::Accent,
            content_shadow_amount: 1.0,
            content_halo_amount: 1.0,
            appearance: crate::theme::Appearance::default(),
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
    if fresh {
        // No settings file, so nobody has been here before. The only honest
        // signal for it: there is no account, and nothing to ask.
        settings.seen_setup = false;
    }
    settings
}

/// Carry across the one field that changed address.
///
/// Everything else in [`crate::theme::Appearance`] is new, so `serde(default)`
/// is enough for it. This one already had a value somebody chose, and losing
/// it would silently frost a window they had asked to be solid.
fn migrate(settings: &mut Settings) {
    let Some(translucency) = settings.legacy_translucency.take() else {
        return;
    };
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

    /// A fresh profile opens on Zervo's own material, and the arrangement the
    /// study argues for is one press away.
    ///
    /// The default was `Candy` for a while and that was wrong twice over: the
    /// artboards the rest of this design is drawn against use the shipped
    /// configuration, so nothing could be compared against them; and a browser
    /// should not open on an argument.
    #[test]
    fn the_default_is_zervos_own_material() {
        assert_eq!(Settings::default().appearance.preset, Some(Preset::Zervo));
        assert_eq!(
            crate::theme::Appearance::classic(),
            Settings::default().appearance,
            "the fallback and the default have to be the same arrangement"
        );
        assert!(Preset::ALL.contains(&Preset::Candy));
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
