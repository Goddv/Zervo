//! UI-side state for the browser: workspaces containing tabs, plus the
//! transient state of the chrome (address bar text, which tab is active).
//! Engine state (the `WebView` for each tab) is attached by `main.rs`.

use servo::WebView;

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
}

pub struct Tab {
    pub id: TabId,
    pub kind: TabKind,
    /// Pinned tabs render as essential tiles in the sidebar grid.
    pub pinned: bool,
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
            title: "Settings".to_owned(),
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
            title: "History".to_owned(),
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
            title: "Downloads".to_owned(),
            url: "zervo://downloads".to_owned(),
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
            title: "New Tab".to_owned(),
            url: "zervo://newtab".to_owned(),
            webview: None,
            loading: false,
            can_go_back: false,
            can_go_forward: false,
        }
    }
}

/// Categories in the settings page's left-hand navigation.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SettingsSection {
    Appearance,
    General,
    Customize,
    Passwords,
    About,
}

impl SettingsSection {
    pub const ALL: [SettingsSection; 5] = [
        SettingsSection::Appearance,
        SettingsSection::General,
        SettingsSection::Customize,
        SettingsSection::Passwords,
        SettingsSection::About,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            SettingsSection::Appearance => "Appearance",
            SettingsSection::General => "General",
            SettingsSection::Customize => "Customize",
            SettingsSection::Passwords => "Passwords",
            SettingsSection::About => "About",
        }
    }
}

pub struct Workspace {
    pub name: String,
    pub tabs: Vec<Tab>,
}

pub struct BrowserState {
    pub workspaces: Vec<Workspace>,
    pub active_workspace: usize,
    pub active_tab: Option<TabId>,
    pub address_bar: String,
    /// True while the address bar has focus and the user is editing it, so we
    /// don't clobber their typing with engine-driven URL updates.
    pub editing_address: bool,
    /// Search box on the history page.
    pub history_query: String,
    /// The half-typed login on the passwords page: site, username, password.
    pub password_draft: (String, String, String),
    /// What the passwords page last did, shown under the form.
    pub password_notice: String,
    /// One-shot request for the address bar to grab keyboard focus (⌘L).
    pub focus_address: bool,
    /// Text in the zervo://newtab page's centered search box.
    pub newtab_query: String,
    /// Which settings category the settings page is showing.
    pub settings_section: SettingsSection,
    /// Sidebar hidden (session state, toggled from the chrome).
    pub sidebar_collapsed: bool,
    next_tab_id: TabId,
}

impl BrowserState {
    pub fn new(initial_url: &str) -> Self {
        let mut state = Self {
            workspaces: vec![Workspace {
                name: "Home".to_owned(),
                tabs: Vec::new(),
            }],
            active_workspace: 0,
            active_tab: None,
            address_bar: initial_url.to_owned(),
            editing_address: false,
            history_query: String::new(),
            password_draft: (String::new(), String::new(), String::new()),
            password_notice: String::new(),
            focus_address: false,
            newtab_query: String::new(),
            settings_section: SettingsSection::Appearance,
            sidebar_collapsed: false,
            next_tab_id: 0,
        };
        let id = state.add_tab(0, initial_url);
        state.active_tab = Some(id);
        state
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

    pub fn add_workspace(&mut self, name: impl Into<String>) -> usize {
        self.workspaces.push(Workspace {
            name: name.into(),
            tabs: Vec::new(),
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
