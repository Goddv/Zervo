//! UI-side state for the browser: workspaces containing tabs, plus the
//! transient state of the chrome (address bar text, which tab is active).
//! Engine state (the `WebView` for each tab) is attached by `main.rs`.

use crate::l10n::text as tr;
use serde::{Deserialize, Serialize};
use servo::WebView;
use url::Url;

pub type TabId = u64;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TabKind {
    /// A normal web page backed by an engine webview.
    Web,
    /// The internal settings page (zervo://settings) — no webview.
    Settings,
    /// The internal new-tab page (zervo://newtab) — no webview until the user
    /// navigates, which converts it to a web tab in place.
    NewTab,
    /// The internal history page (zervo://history).
    History,
    /// The internal downloads page (zervo://downloads).
    Downloads,
    /// One of the four pages a page that will not load lands on
    /// (`zervo://unsupported` and its three siblings). See [`crate::trouble`].
    ///
    /// The kind carries which of the four it is, so two different troubles are
    /// two different tabs rather than one that keeps changing its mind —
    /// `find_or_create_internal` compares kinds, and the payload is part of
    /// that comparison.
    Trouble(crate::trouble::Trouble),
}

impl TabKind {
    fn internal_title(self) -> Option<&'static str> {
        match self {
            Self::Settings => Some(tr("common-settings")),
            Self::History => Some(tr("common-history")),
            Self::Downloads => Some(tr("common-downloads")),
            Self::NewTab => Some(tr("common-new-tab")),
            Self::Trouble(trouble) => Some(trouble.title()),
            Self::Web => None,
        }
    }
}

pub struct Tab {
    pub id: TabId,
    pub kind: TabKind,
    /// Pinned tabs stay above the open tabs in their space.
    pub pinned: bool,
    pub pin_anchor: Option<crate::library::Favourite>,
    pub title: String,
    pub url: String,
    /// The engine view backing this tab. `None` until the tab is first activated
    /// (lazy creation keeps startup and background workspaces cheap).
    pub webview: Option<WebView>,
    pub loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
}

impl Tab {
    pub fn new(id: TabId, url: impl Into<String>) -> Self {
        let url = url.into();
        Self {
            id,
            kind: TabKind::Web,
            pinned: false,
            pin_anchor: None,
            title: url.clone(),
            url,
            webview: None,
            loading: false,
            can_go_back: false,
            can_go_forward: false,
        }
    }

    pub fn new_settings(id: TabId) -> Self {
        Self {
            id,
            kind: TabKind::Settings,
            pinned: false,
            pin_anchor: None,
            title: TabKind::Settings
                .internal_title()
                .unwrap_or_default()
                .to_owned(),
            url: "zervo://settings".to_owned(),
            webview: None,
            loading: false,
            can_go_back: false,
            can_go_forward: false,
        }
    }

    pub fn new_history(id: TabId) -> Self {
        Self {
            id,
            kind: TabKind::History,
            pinned: false,
            pin_anchor: None,
            title: TabKind::History
                .internal_title()
                .unwrap_or_default()
                .to_owned(),
            url: "zervo://history".to_owned(),
            webview: None,
            loading: false,
            can_go_back: false,
            can_go_forward: false,
        }
    }

    pub fn new_downloads(id: TabId) -> Self {
        Self {
            id,
            kind: TabKind::Downloads,
            pinned: false,
            pin_anchor: None,
            title: TabKind::Downloads
                .internal_title()
                .unwrap_or_default()
                .to_owned(),
            url: "zervo://downloads".to_owned(),
            webview: None,
            loading: false,
            can_go_back: false,
            can_go_forward: false,
        }
    }

    /// A page that will not load. `url` carries what is known about the
    /// failure in its query string, so the address bar and the page agree and
    /// either can be typed back in.
    pub fn new_trouble(id: TabId, trouble: crate::trouble::Trouble, url: String) -> Self {
        Self {
            id,
            kind: TabKind::Trouble(trouble),
            pinned: false,
            pin_anchor: None,
            title: trouble.title().to_owned(),
            url,
            webview: None,
            loading: false,
            can_go_back: false,
            can_go_forward: false,
        }
    }

    pub fn new_zervo_page(id: TabId) -> Self {
        Self {
            id,
            kind: TabKind::NewTab,
            pinned: false,
            pin_anchor: None,
            title: TabKind::NewTab
                .internal_title()
                .unwrap_or_default()
                .to_owned(),
            url: "zervo://newtab".to_owned(),
            webview: None,
            loading: false,
            can_go_back: false,
            can_go_forward: false,
        }
    }
}

/// Categories in the settings page's left-hand navigation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SettingsSection {
    Appearance,
    General,
    Layout,
    Passwords,
    About,
}

impl SettingsSection {
    pub const ALL: [SettingsSection; 5] = [
        SettingsSection::Appearance,
        SettingsSection::General,
        SettingsSection::Layout,
        SettingsSection::Passwords,
        SettingsSection::About,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            SettingsSection::Appearance => tr("common-appearance"),
            SettingsSection::General => tr("common-general"),
            SettingsSection::Layout => tr("common-layout"),
            SettingsSection::Passwords => tr("common-passwords"),
            SettingsSection::About => tr("common-about"),
        }
    }
}

pub struct Workspace {
    pub name: String,
    pub tabs: Vec<Tab>,
    last_active: Option<TabId>,
}

pub struct BrowserState {
    pub workspaces: Vec<Workspace>,
    pub active_workspace: usize,
    pub active_tab: Option<TabId>,
    pub address_bar: String,
    /// The favourite being renamed, and the name so far.
    pub favourite_edit: Option<(String, String)>,
    /// The workspace being named, and the name so far. Set the moment one tab
    /// is dropped on another, so the group opens asking what it is.
    pub workspace_edit: Option<(usize, String)>,
    /// Search box on the history page.
    pub history_query: String,
    /// The half-typed login on the passwords page: site, username, password.
    pub password_draft: (String, String, String),
    /// What the passwords page last did, shown under the form.
    pub password_notice: String,
    pub command_bar: Option<crate::ui::CommandBar>,
    /// Text in the zervo://newtab page's search box.
    pub newtab_query: String,
    /// The half-typed line on the new tab page's to-do card.
    pub newtab_task_draft: String,
    /// The new tab page is being arranged. Session state on purpose: nobody
    /// wants to open a browser and find it still in edit mode from yesterday.
    pub newtab_editing: bool,
    /// Which settings category the settings page is showing.
    pub settings_section: SettingsSection,

    next_tab_id: TabId,
}

impl BrowserState {
    pub fn refresh_internal_titles(&mut self) {
        for tab in self
            .workspaces
            .iter_mut()
            .flat_map(|workspace| &mut workspace.tabs)
        {
            if let Some(title) = tab.kind.internal_title() {
                tab.title = title.to_owned();
            }
        }
    }

    pub fn new(initial_url: &str) -> Self {
        let mut state = Self {
            workspaces: vec![Workspace {
                name: tr("common-home").to_owned(),
                tabs: Vec::new(),
                last_active: None,
            }],
            active_workspace: 0,
            active_tab: None,
            address_bar: initial_url.to_owned(),
            favourite_edit: None,
            workspace_edit: None,
            history_query: String::new(),
            password_draft: (String::new(), String::new(), String::new()),
            password_notice: String::new(),
            command_bar: None,
            newtab_query: String::new(),
            newtab_task_draft: String::new(),
            newtab_editing: false,
            settings_section: SettingsSection::Appearance,
            next_tab_id: 0,
        };
        let id = state.add_tab(0, initial_url);
        state.select_tab(id);
        state
    }

    pub fn restore(homepage: &str, first_name: &str, explicit_url: bool) -> Self {
        let saved: SavedSpaces = crate::store::load_or_default(spaces_path());
        Self::from_saved(saved, homepage, first_name, explicit_url)
    }

    fn from_saved(
        saved: SavedSpaces,
        homepage: &str,
        first_name: &str,
        explicit_url: bool,
    ) -> Self {
        let mut state = Self::new(homepage);
        if saved.spaces.is_empty() {
            if !first_name.trim().is_empty() {
                state.workspaces[0].name = first_name.trim().to_owned();
            }
            return state;
        }

        state.workspaces.clear();
        for space in saved.spaces {
            let index = state.add_workspace(space.name);
            for pin in space.pins {
                if Url::parse(&pin.url).is_ok_and(|url| url.scheme() != "zervo") {
                    let tab = state.add_tab(index, &pin.url);
                    if let Some(tab) = state.tab_mut(tab) {
                        tab.title = pin.title.clone();
                        tab.pinned = true;
                        tab.pin_anchor = Some(pin);
                    }
                }
            }
        }
        state.active_workspace = saved.active.min(state.workspaces.len() - 1);
        let index = state.active_workspace;
        let tab = match (explicit_url, state.tab_for_workspace(index)) {
            (false, Some(tab)) => tab,
            _ => state.add_tab(index, homepage),
        };
        state.select_tab(tab);
        if let Some(tab) = state.tab(tab) {
            state.address_bar = tab.url.clone();
        }
        state
    }

    fn saved_spaces(&self) -> SavedSpaces {
        SavedSpaces {
            active: self.active_workspace,
            spaces: self
                .workspaces
                .iter()
                .map(|space| SavedSpace {
                    name: space.name.clone(),
                    pins: space
                        .tabs
                        .iter()
                        .filter_map(|tab| tab.pin_anchor.clone())
                        .collect(),
                })
                .collect(),
        }
    }

    pub fn save_spaces(&self) {
        let _ = crate::store::save(spaces_path(), &self.saved_spaces());
    }

    pub fn tab_for_workspace(&self, index: usize) -> Option<TabId> {
        let workspace = self.workspaces.get(index)?;
        workspace
            .last_active
            .filter(|id| workspace.tabs.iter().any(|tab| tab.id == *id))
            .or_else(|| workspace.tabs.last().map(|tab| tab.id))
    }

    pub fn select_tab(&mut self, id: TabId) -> bool {
        let Some(index) = self
            .workspaces
            .iter()
            .position(|space| space.tabs.iter().any(|tab| tab.id == id))
        else {
            return false;
        };
        self.active_workspace = index;
        self.active_tab = Some(id);
        self.workspaces[index].last_active = Some(id);
        true
    }

    pub fn can_remove_workspace(&self, index: usize) -> bool {
        self.workspaces.len() > 1
            && self.workspaces.get(index).is_some_and(|space| {
                space.tabs.is_empty()
                    || matches!(space.tabs.as_slice(), [tab] if tab.kind == TabKind::NewTab)
            })
    }

    pub fn remove_workspace(&mut self, index: usize) -> bool {
        if !self.can_remove_workspace(index) {
            return false;
        }
        self.workspaces.remove(index);
        if self.active_workspace == index {
            self.active_tab = None;
        } else if self.active_workspace > index {
            self.active_workspace -= 1;
        }
        self.active_workspace = self.active_workspace.min(self.workspaces.len() - 1);
        self.workspace_edit = None;
        true
    }

    pub fn add_tab(&mut self, workspace: usize, url: impl Into<String>) -> TabId {
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        self.workspaces[workspace].tabs.push(Tab::new(id, url));
        id
    }

    /// Add an internal zervo://newtab page tab to a workspace.
    pub fn add_zervo_page(&mut self, workspace: usize) -> TabId {
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        self.workspaces[workspace]
            .tabs
            .push(Tab::new_zervo_page(id));
        id
    }

    /// Find an existing internal tab of `kind`, or create one in the active
    /// workspace. Returns its id.
    fn find_or_create_internal(&mut self, kind: TabKind) -> TabId {
        if let Some(tab) = self
            .workspaces
            .iter()
            .flat_map(|workspace| workspace.tabs.iter())
            .find(|tab| tab.kind == kind)
        {
            return tab.id;
        }
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        let workspace = self.active_workspace;
        let tab = match kind {
            TabKind::Downloads => Tab::new_downloads(id),
            TabKind::History => Tab::new_history(id),
            TabKind::Trouble(trouble) => Tab::new_trouble(id, trouble, trouble.url()),
            _ => Tab::new_settings(id),
        };
        self.workspaces[workspace].tabs.push(tab);
        id
    }

    pub fn find_or_create_settings_tab(&mut self) -> TabId {
        self.find_or_create_internal(TabKind::Settings)
    }

    pub fn find_or_create_history_tab(&mut self) -> TabId {
        self.find_or_create_internal(TabKind::History)
    }

    pub fn find_or_create_downloads_tab(&mut self) -> TabId {
        self.find_or_create_internal(TabKind::Downloads)
    }

    /// The tab for one of the four trouble pages, at `url` — which carries
    /// what is known about this particular failure and so has to be written
    /// even when the tab already existed.
    pub fn find_or_create_trouble_tab(
        &mut self,
        trouble: crate::trouble::Trouble,
        url: String,
    ) -> TabId {
        let id = self.find_or_create_internal(TabKind::Trouble(trouble));
        if let Some(tab) = self.tab_mut(id) {
            tab.url = url;
        }
        id
    }

    pub fn add_workspace(&mut self, name: impl Into<String>) -> usize {
        self.workspaces.push(Workspace {
            name: name.into(),
            tabs: Vec::new(),
            last_active: None,
        });
        self.workspaces.len() - 1
    }

    pub fn tab(&self, id: TabId) -> Option<&Tab> {
        self.workspaces
            .iter()
            .flat_map(|workspace| workspace.tabs.iter())
            .find(|tab| tab.id == id)
    }

    pub fn tab_mut(&mut self, id: TabId) -> Option<&mut Tab> {
        self.workspaces
            .iter_mut()
            .flat_map(|workspace| workspace.tabs.iter_mut())
            .find(|tab| tab.id == id)
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.tab(self.active_tab?)
    }

    /// Find the tab owning the given webview, by the engine's webview id.
    pub fn tab_for_webview_mut(&mut self, webview: &WebView) -> Option<&mut Tab> {
        let id = webview.id();
        self.workspaces
            .iter_mut()
            .flat_map(|workspace| workspace.tabs.iter_mut())
            .find(|tab| tab.webview.as_ref().is_some_and(|wv| wv.id() == id))
    }

    /// Move a tab to `index` in `workspace`, keeping the tab itself — and with
    /// it the live page — intact. False if either the tab or the workspace has
    /// gone.
    ///
    /// Deliberately not `remove_tab` followed by `add_tab`: `remove_tab` hands
    /// back only the `WebView` and drops everything else about the tab, and
    /// `add_tab` builds a fresh one. Round-tripping through them would close
    /// the page and reopen it, losing scroll position, form state and history.
    pub fn move_tab(&mut self, id: TabId, workspace: usize, index: usize) -> bool {
        if workspace >= self.workspaces.len() {
            return false;
        }
        let Some(from) = self
            .workspaces
            .iter()
            .position(|space| space.tabs.iter().any(|tab| tab.id == id))
        else {
            return false;
        };
        let Some(at) = self.workspaces[from]
            .tabs
            .iter()
            .position(|tab| tab.id == id)
        else {
            return false;
        };
        let tab = self.workspaces[from].tabs.remove(at);
        // Within one workspace the caller's index was worked out against the
        // list as it looked *before* the removal, so everything after the tab
        // has since shifted down one. Correct for that first and clamp
        // second: clamping to the post-removal length already accounts for
        // the shift, so doing it the other way round subtracts it twice and
        // a tab dragged to the end of its own list lands one slot short.
        let mut index = index;
        if from == workspace && at < index {
            index -= 1;
        }
        let index = index.min(self.workspaces[workspace].tabs.len());
        self.workspaces[workspace].tabs.insert(index, tab);
        true
    }

    /// Put two tabs in a workspace of their own and return its index. The tab
    /// that was dropped on keeps its place at the front.
    pub fn group_tabs(&mut self, name: impl Into<String>, onto: TabId, dragged: TabId) -> usize {
        let workspace = self.add_workspace(name);
        self.move_tab(onto, workspace, 0);
        self.move_tab(dragged, workspace, 1);
        workspace
    }

    /// Remove a tab, returning its webview (if any) so the caller can drop it
    /// after releasing any engine-side references.
    pub fn remove_tab(&mut self, id: TabId) -> Option<WebView> {
        for workspace in &mut self.workspaces {
            if let Some(index) = workspace.tabs.iter().position(|tab| tab.id == id) {
                let tab = workspace.tabs.remove(index);
                if self.active_tab == Some(id) {
                    self.active_tab = workspace
                        .tabs
                        .get(index.min(workspace.tabs.len().saturating_sub(1)))
                        .map(|tab| tab.id);
                }
                return tab.webview;
            }
        }
        None
    }
}

#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
struct SavedSpaces {
    active: usize,
    spaces: Vec<SavedSpace>,
}

#[derive(Deserialize, Serialize)]
struct SavedSpace {
    name: String,
    pins: Vec<crate::library::Favourite>,
}

fn spaces_path() -> Option<std::path::PathBuf> {
    Some(crate::settings::data_dir()?.join("spaces.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restores_spaces_and_pins_without_discarding_an_explicit_url() {
        let mut state = BrowserState::new("https://example.com");
        let first = state.active_tab.unwrap();
        let second = state.add_workspace("Research");
        let pinned = state.add_tab(second, "https://example.org/visited");
        let tab = state.tab_mut(pinned).unwrap();
        tab.pinned = true;
        tab.pin_anchor = Some(crate::library::Favourite {
            title: "Notes".to_owned(),
            url: "https://example.org/notes".to_owned(),
        });
        assert!(state.select_tab(pinned));
        assert_eq!(state.active_workspace, second);
        assert_eq!(state.tab_for_workspace(0), Some(first));

        let saved = serde_json::to_string(&state.saved_spaces()).unwrap();
        let restored = BrowserState::from_saved(
            serde_json::from_str(&saved).unwrap(),
            "https://example.com",
            "",
            false,
        );
        assert_eq!(restored.active_workspace, second);
        let tab = restored.active_tab().unwrap();
        assert!(tab.pinned);
        assert_eq!(tab.title, "Notes");
        assert_eq!(tab.url, "https://example.org/notes");

        let opened = BrowserState::from_saved(
            serde_json::from_str(&saved).unwrap(),
            "https://example.net",
            "",
            true,
        );
        assert_eq!(opened.active_workspace, second);
        let tab = opened.active_tab().unwrap();
        assert!(!tab.pinned);
        assert_eq!(tab.url, "https://example.net");
        assert_eq!(opened.workspaces[second].tabs.len(), 2);
    }

    #[test]
    fn moving_a_tab_updates_the_space_and_invalidates_the_old_selection() {
        let mut state = BrowserState::new("https://example.com");
        let initial = state.active_tab.unwrap();
        let replacement = state.add_zervo_page(0);
        let second = state.add_workspace("Work");
        assert!(state.move_tab(initial, second, 0));
        assert!(state.select_tab(initial));
        assert_eq!(state.active_workspace, second);
        assert_eq!(state.tab_for_workspace(0), Some(replacement));
        assert!(state.remove_workspace(0));
        assert_eq!(state.active_workspace, 0);
        assert_eq!(state.active_tab, Some(initial));
    }
}
