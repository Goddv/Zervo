//! UI-side state for the browser: workspaces containing tabs, plus the
//! transient state of the chrome (address bar text, which tab is active).
//! Engine state (the `WebView` for each tab) is attached by `main.rs`.

use crate::l10n::text as tr;
use serde::{Deserialize, Serialize};
use servo::WebView;
use url::Url;

pub type TabId = u64;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FolderId(u64);

pub struct TabFolder {
    pub id: FolderId,
    pub name: String,
    pub collapsed: bool,
}

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
    /// The internal release notes and updater status page.
    Updates,
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
            Self::Updates => Some(tr("updates-title")),
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
    pub folder: Option<FolderId>,
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
            folder: None,
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
            folder: None,
            title: TabKind::Settings
                .internal_title()
                .unwrap_or_default()
                .to_owned(),
            url: "heion://settings".to_owned(),
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
            folder: None,
            title: TabKind::History
                .internal_title()
                .unwrap_or_default()
                .to_owned(),
            url: "heion://history".to_owned(),
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
            folder: None,
            title: TabKind::Downloads
                .internal_title()
                .unwrap_or_default()
                .to_owned(),
            url: "heion://downloads".to_owned(),
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
        let url = match url.strip_prefix("zervo://") {
            Some(rest) => format!("heion://{rest}"),
            None => url,
        };
        Self {
            id,
            kind: TabKind::Trouble(trouble),
            pinned: false,
            pin_anchor: None,
            folder: None,
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
            folder: None,
            title: TabKind::NewTab
                .internal_title()
                .unwrap_or_default()
                .to_owned(),
            url: "heion://newtab".to_owned(),
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
    pub folders: Vec<TabFolder>,
    last_active: Option<TabId>,
}

pub struct BrowserState {
    pub workspaces: Vec<Workspace>,
    pub active_workspace: usize,
    pub active_tab: Option<TabId>,
    pub address_bar: String,
    /// The favourite being renamed, and the name so far.
    pub favourite_edit: Option<(String, String)>,
    /// The workspace or folder being named, and the draft name.
    pub workspace_edit: Option<(usize, String)>,
    pub folder_edit: Option<(FolderId, String)>,
    /// Search box on the history page.
    pub history_query: String,
    pub browsing_data_cleared: bool,
    /// The half-typed login on the passwords page: site, username, password.
    pub password_draft: (String, String, String),
    /// What the passwords page last did, shown under the form.
    pub password_notice: String,
    pub command_bar: Option<crate::ui::CommandBar>,
    /// Which settings category the settings page is showing.
    pub settings_section: SettingsSection,
    pub settings_query: String,

    session_dirty: bool,
    next_tab_id: TabId,
    next_folder_id: u64,
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
                folders: Vec::new(),
                last_active: None,
            }],
            active_workspace: 0,
            active_tab: None,
            address_bar: initial_url.to_owned(),
            favourite_edit: None,
            workspace_edit: None,
            folder_edit: None,
            history_query: String::new(),
            browsing_data_cleared: false,
            password_draft: (String::new(), String::new(), String::new()),
            password_notice: String::new(),
            command_bar: None,
            settings_section: SettingsSection::Appearance,
            settings_query: String::new(),
            session_dirty: true,
            next_tab_id: 0,
            next_folder_id: 0,
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
        state.active_tab = None;
        for space in saved.spaces {
            let index = state.add_workspace(space.name);
            let mut folders = Vec::with_capacity(space.folders.len());
            for folder in &space.folders {
                let id = state.add_folder(index, folder.name.clone());
                state.folder_mut(id).unwrap().collapsed = folder.collapsed;
                folders.push(id);
            }
            if let Some(tabs) = space.tabs {
                let mut selected = None;
                for (position, saved) in tabs.into_iter().enumerate() {
                    let folder = saved.folder.and_then(|index| folders.get(index).copied());
                    if let Some(id) = state.restore_tab(index, folder, saved) {
                        if Some(position) == space.selected {
                            selected = Some(id);
                        }
                    }
                }
                state.workspaces[index].last_active = selected;
            } else {
                // Old profiles saved pinned and foldered tabs only.
                for pin in space.pins {
                    state.restore_tab(index, None, SavedTab::legacy(pin, true));
                }
                for (id, folder) in folders.into_iter().zip(space.folders) {
                    for tab in folder.tabs {
                        state.restore_tab(index, Some(id), SavedTab::legacy(tab.site, tab.pinned));
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

    fn restore_tab(
        &mut self,
        workspace: usize,
        folder: Option<FolderId>,
        saved: SavedTab,
    ) -> Option<TabId> {
        let url = Url::parse(&saved.url).ok()?;
        let id = self.next_tab_id;
        let mut tab = match url.scheme() {
            "heion" | "zervo" => match url.host_str()? {
                "newtab" => Tab::new_zervo_page(id),
                "settings" => Tab::new_settings(id),
                "history" => Tab::new_history(id),
                "downloads" => Tab::new_downloads(id),
                "updates" => Tab {
                    kind: TabKind::Updates,
                    title: tr("updates-title").into(),
                    ..Tab::new(id, "heion://updates")
                },
                _ => Tab::new_trouble(
                    id,
                    crate::trouble::Trouble::from_url(&saved.url)?,
                    saved.url,
                ),
            },
            // An internal action or a JavaScript URL must not run at launch.
            "javascript" | "blob" => return None,
            _ => {
                let mut tab = Tab::new(id, saved.url);
                if !saved.title.is_empty() {
                    tab.title = saved.title;
                }
                tab
            },
        };
        tab.folder = folder;
        tab.pinned = saved.pinned;
        tab.pin_anchor = saved.pin_anchor;
        self.next_tab_id += 1;
        self.workspaces[workspace].tabs.push(tab);
        Some(id)
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
                        .filter(|tab| tab.folder.is_none())
                        .filter_map(|tab| tab.pin_anchor.clone())
                        .collect(),
                    folders: space
                        .folders
                        .iter()
                        .map(|folder| SavedFolder {
                            name: folder.name.clone(),
                            collapsed: folder.collapsed,
                            tabs: space
                                .tabs
                                .iter()
                                .filter(|tab| {
                                    tab.folder == Some(folder.id) && tab.kind == TabKind::Web
                                })
                                .map(|tab| SavedFolderTab {
                                    site: tab.pin_anchor.clone().unwrap_or_else(|| {
                                        crate::library::Favourite {
                                            url: tab.url.clone(),
                                            title: tab.title.clone(),
                                        }
                                    }),
                                    pinned: tab.pinned,
                                })
                                .collect(),
                        })
                        .collect(),
                    selected: space
                        .last_active
                        .and_then(|id| space.tabs.iter().position(|tab| tab.id == id)),
                    tabs: Some(
                        space
                            .tabs
                            .iter()
                            .map(|tab| SavedTab {
                                url: tab.url.clone(),
                                title: tab.title.clone(),
                                pinned: tab.pinned,
                                pin_anchor: tab.pin_anchor.clone(),
                                folder: tab.folder.and_then(|id| {
                                    space.folders.iter().position(|folder| folder.id == id)
                                }),
                            })
                            .collect(),
                    ),
                })
                .collect(),
        }
    }

    pub fn mark_session_dirty(&mut self) {
        self.session_dirty = true;
    }

    pub fn save_session(&mut self) -> bool {
        if self.session_dirty && crate::store::save(spaces_path(), &self.saved_spaces()) {
            self.session_dirty = false;
        }
        !self.session_dirty
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
        if index != self.active_workspace {
            self.folder_edit = None;
            if self
                .workspace_edit
                .as_ref()
                .is_some_and(|(editing, _)| *editing != index)
            {
                self.workspace_edit = None;
            }
        }
        self.session_dirty |=
            self.active_tab != Some(id) || self.workspaces[index].last_active != Some(id);
        self.active_workspace = index;
        self.active_tab = Some(id);
        self.workspaces[index].last_active = Some(id);
        true
    }

    pub fn can_remove_workspace(&self, index: usize) -> bool {
        self.workspaces.len() > 1
            && self.workspaces.get(index).is_some_and(|space| {
                space.folders.is_empty()
                    && (space.tabs.is_empty()
                        || matches!(space.tabs.as_slice(), [tab] if tab.kind == TabKind::NewTab))
            })
    }

    pub fn remove_workspace(&mut self, index: usize) -> bool {
        if !self.can_remove_workspace(index) {
            return false;
        }
        self.session_dirty = true;
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
        self.session_dirty = true;
        self.workspaces[workspace].tabs.push(Tab::new(id, url));
        id
    }

    /// Add an internal zervo://newtab page tab to a workspace.
    pub fn add_zervo_page(&mut self, workspace: usize) -> TabId {
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        self.session_dirty = true;
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
        self.session_dirty = true;
        let tab = match kind {
            TabKind::Downloads => Tab::new_downloads(id),
            TabKind::History => Tab::new_history(id),
            TabKind::Updates => Tab {
                kind,
                title: tr("updates-title").into(),
                ..Tab::new(id, "heion://updates")
            },
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

    pub fn find_or_create_updates_tab(&mut self) -> TabId {
        self.find_or_create_internal(TabKind::Updates)
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
            self.session_dirty = true;
        }
        id
    }

    pub fn add_workspace(&mut self, name: impl Into<String>) -> usize {
        self.session_dirty = true;
        self.workspaces.push(Workspace {
            name: name.into(),
            tabs: Vec::new(),
            folders: Vec::new(),
            last_active: None,
        });
        self.workspaces.len() - 1
    }

    pub fn adjacent_workspace(&self, forward: bool) -> Option<usize> {
        let count = self.workspaces.len();
        if count < 2 {
            return None;
        }
        let offset = if forward { 1 } else { count - 1 };
        Some((self.active_workspace + offset) % count)
    }

    pub fn add_folder(&mut self, workspace: usize, name: String) -> FolderId {
        let id = FolderId(self.next_folder_id);
        self.next_folder_id += 1;
        self.session_dirty = true;
        self.workspaces[workspace].folders.push(TabFolder {
            id,
            name,
            collapsed: false,
        });
        id
    }

    pub fn folder_mut(&mut self, id: FolderId) -> Option<&mut TabFolder> {
        self.workspaces
            .iter_mut()
            .flat_map(|workspace| &mut workspace.folders)
            .find(|folder| folder.id == id)
    }

    pub fn remove_folder(&mut self, id: FolderId) -> bool {
        for workspace in &mut self.workspaces {
            if let Some(index) = workspace.folders.iter().position(|folder| folder.id == id) {
                self.session_dirty = true;
                workspace.folders.remove(index);
                for tab in &mut workspace.tabs {
                    if tab.folder == Some(id) {
                        tab.folder = None;
                    }
                }
                if self
                    .folder_edit
                    .as_ref()
                    .is_some_and(|(editing, _)| *editing == id)
                {
                    self.folder_edit = None;
                }
                return true;
            }
        }
        false
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
    pub fn move_tab(
        &mut self,
        id: TabId,
        workspace: usize,
        index: usize,
        folder: Option<FolderId>,
    ) -> bool {
        let Some(target) = self.workspaces.get(workspace) else {
            return false;
        };
        if folder.is_some_and(|id| target.folders.iter().all(|folder| folder.id != id)) {
            return false;
        }
        let Some((from, at)) = self.tab_location(id) else {
            return false;
        };

        self.session_dirty = true;
        let mut tab = self.workspaces[from].tabs.remove(at);
        if from != workspace && self.workspaces[from].last_active == Some(id) {
            self.workspaces[from].last_active = None;
        }
        tab.folder = folder;
        // Within one workspace the drop index was calculated before removal.
        let index = index - usize::from(from == workspace && at < index);
        let tabs = &mut self.workspaces[workspace].tabs;
        tabs.insert(index.min(tabs.len()), tab);
        true
    }

    fn tab_location(&self, id: TabId) -> Option<(usize, usize)> {
        self.workspaces
            .iter()
            .enumerate()
            .find_map(|(space, workspace)| {
                workspace
                    .tabs
                    .iter()
                    .position(|tab| tab.id == id)
                    .map(|index| (space, index))
            })
    }

    /// Dropping on an unfiled tab groups both in their existing workspace.
    /// Dropping on a folder member inserts immediately after that tab.
    pub fn group_tabs(&mut self, onto: TabId, dragged: TabId) -> Option<(FolderId, bool)> {
        let (workspace, index) = self.tab_location(onto)?;
        if onto == dragged || self.tab_location(dragged)?.0 != workspace {
            return None;
        }
        let folder = self.workspaces[workspace].tabs[index].folder;
        let created = folder.is_none();
        let folder = folder
            .unwrap_or_else(|| self.add_folder(workspace, tr("sidebar-new-folder").to_owned()));
        self.workspaces[workspace].tabs[index].folder = Some(folder);
        self.move_tab(dragged, workspace, index + 1, Some(folder));
        Some((folder, created))
    }

    /// Remove a tab, returning its webview (if any) so the caller can drop it
    /// after releasing any engine-side references.
    pub fn remove_tab(&mut self, id: TabId) -> Option<WebView> {
        for workspace in &mut self.workspaces {
            if let Some(index) = workspace.tabs.iter().position(|tab| tab.id == id) {
                self.session_dirty = true;
                let tab = workspace.tabs.remove(index);
                if workspace.last_active == Some(id) {
                    workspace.last_active = None;
                }
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
    #[serde(default)]
    pins: Vec<crate::library::Favourite>,
    #[serde(default)]
    selected: Option<usize>,
    #[serde(default)]
    tabs: Option<Vec<SavedTab>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    folders: Vec<SavedFolder>,
}

#[derive(Deserialize, Serialize)]
struct SavedFolder {
    name: String,
    #[serde(default)]
    collapsed: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tabs: Vec<SavedFolderTab>,
}

#[derive(Deserialize, Serialize)]
struct SavedFolderTab {
    site: crate::library::Favourite,
    #[serde(default)]
    pinned: bool,
}

#[derive(Deserialize, Serialize)]
struct SavedTab {
    url: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    pinned: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pin_anchor: Option<crate::library::Favourite>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    folder: Option<usize>,
}

impl SavedTab {
    fn legacy(site: crate::library::Favourite, pinned: bool) -> Self {
        Self {
            url: site.url.clone(),
            title: site.title.clone(),
            pinned,
            pin_anchor: pinned.then_some(site),
            folder: None,
        }
    }
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
        assert_eq!(tab.url, "https://example.org/visited");
        assert_eq!(
            tab.pin_anchor.as_ref().unwrap().url,
            "https://example.org/notes"
        );

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
    fn restores_tab_order_and_selected_tab_in_each_space() {
        let mut state = BrowserState::new("https://example.com/first");
        let history = state.find_or_create_history_tab();
        let last = state.add_tab(0, "https://example.com/last");
        assert!(state.select_tab(history));
        let work = state.add_workspace("Work");
        let first = state.add_tab(work, "https://example.org/first");
        state.add_zervo_page(work);
        assert!(state.select_tab(first));

        let restored =
            BrowserState::from_saved(state.saved_spaces(), "https://fallback.test/", "", false);
        assert_eq!(restored.active_workspace, work);
        assert_eq!(
            restored.active_tab().unwrap().url,
            "https://example.org/first"
        );
        assert_eq!(restored.workspaces[0].tabs.len(), 3);
        assert_eq!(restored.workspaces[0].tabs[1].url, "heion://history");
        assert_eq!(
            restored.workspaces[0].tabs[2].url,
            state.tab(last).unwrap().url
        );
        let selected = restored.tab_for_workspace(0).unwrap();
        assert!(restored.tab(selected).unwrap().kind == TabKind::History);
        assert!(
            restored
                .workspaces
                .iter()
                .flat_map(|space| &space.tabs)
                .all(|tab| tab.webview.is_none())
        );
    }

    #[test]
    fn grouping_creates_a_folder_in_the_same_workspace() {
        let mut state = BrowserState::new("https://example.com/a");
        let first = state.active_tab.unwrap();
        let second = state.add_tab(0, "https://example.com/b");
        let third = state.add_tab(0, "https://example.com/c");
        let unrelated = state.add_workspace("Research");
        let foreign = state.add_tab(unrelated, "https://example.org");
        let (folder, true) = state.group_tabs(second, first).unwrap() else {
            panic!("dropping on a loose tab should create a folder");
        };
        assert_eq!(state.group_tabs(second, third), Some((folder, false)));
        assert_eq!(state.group_tabs(second, foreign), None);
        assert_eq!(state.workspaces.len(), 2);
        let tabs = &state.workspaces[0].tabs;
        assert_eq!(
            tabs.iter().map(|tab| tab.id).collect::<Vec<_>>(),
            [second, third, first]
        );
        assert!(tabs.iter().all(|tab| tab.folder == Some(folder)));
        assert!(!state.move_tab(first, unrelated, 0, Some(folder)));
        assert!(state.move_tab(third, unrelated, 1, None));
        assert_eq!(state.tab(third).unwrap().folder, None);
        assert!(state.remove_folder(folder));
        assert_eq!(state.active_tab, Some(first));
        assert_eq!(state.workspaces[0].tabs.len(), 2);
        assert!(
            state.workspaces[0]
                .tabs
                .iter()
                .all(|tab| tab.folder.is_none())
        );
    }

    #[test]
    fn restores_folder_members_and_collapsed_state_alongside_legacy_pins() {
        let saved: SavedSpaces = serde_json::from_str(
            r#"{
            "spaces": [
                {"name": "Home", "pins": [{"url": "https://example.com", "title": "Home pin"}]},
                {"name": "Work", "pins": [], "folders": [
                    {"name": "Notes", "collapsed": true, "tabs": [
                        {"site": {"url": "https://example.org/a", "title": "A"}},
                        {"site": {"url": "https://example.org/b", "title": "B"}, "pinned": true}
                    ]}
                ]}
            ],
            "active": 1
        }"#,
        )
        .unwrap();
        let state = BrowserState::from_saved(saved, "https://example.net", "", false);
        let saved = state.saved_spaces();
        let restored = BrowserState::from_saved(saved, "https://example.net", "", false);
        assert_eq!(restored.active_workspace, 1);
        assert_eq!(restored.adjacent_workspace(true), Some(0));
        assert_eq!(restored.adjacent_workspace(false), Some(0));
        let workspace = &restored.workspaces[1];
        let folder = &workspace.folders[0];
        assert_eq!(folder.name, "Notes");
        assert!(folder.collapsed);
        assert_eq!(workspace.tabs.len(), 2);
        assert!(
            workspace
                .tabs
                .iter()
                .all(|tab| tab.folder == Some(folder.id))
        );
        assert_eq!(workspace.tabs[0].title, "A");
        assert!(!workspace.tabs[0].pinned);
        assert_eq!(workspace.tabs[1].pin_anchor.as_ref().unwrap().title, "B");
        assert!(restored.workspaces[0].tabs[0].pinned);
        assert!(restored.workspaces[0].folders.is_empty());
    }

    #[test]
    fn moving_a_tab_updates_the_space_and_invalidates_the_old_selection() {
        let mut state = BrowserState::new("https://example.com");
        let initial = state.active_tab.unwrap();
        let replacement = state.add_zervo_page(0);
        let second = state.add_workspace("Work");
        assert!(state.move_tab(initial, second, 0, None));
        assert!(state.select_tab(initial));
        assert_eq!(state.active_workspace, second);
        assert_eq!(state.tab_for_workspace(0), Some(replacement));
        assert!(state.remove_workspace(0));
        assert_eq!(state.active_workspace, 0);
        assert_eq!(state.active_tab, Some(initial));
    }
}
