//! Controls for the site data exposed by Servo's embedder.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Selection {
    #[default]
    CookiesAndStorage,
    Cookies,
    SessionCookies,
}

impl Selection {
    pub const ALL: [Self; 3] = [Self::CookiesAndStorage, Self::Cookies, Self::SessionCookies];

    pub fn label(self) -> &'static str {
        crate::l10n::text(match self {
            Self::CookiesAndStorage => "browsing-data-cookies-and-storage",
            Self::Cookies => "browsing-data-all-cookies",
            Self::SessionCookies => "browsing-data-session-cookies",
        })
    }

    pub fn clear(self, manager: &servo::SiteDataManager) {
        use servo::StorageType;
        match self {
            Self::Cookies => manager.clear_cookies(None),
            Self::SessionCookies => manager.clear_session_cookies(None),
            Self::CookiesAndStorage => {
                let storage = StorageType::Local | StorageType::Session;
                let sites: Vec<_> = manager
                    .site_data(storage)
                    .iter()
                    .map(servo::SiteData::name)
                    .collect();
                let names: Vec<_> = sites.iter().map(String::as_str).collect();
                if !names.is_empty() {
                    manager.clear_site_data(&names, storage);
                }
                manager.clear_cookies(None);
            },
        }
    }
}
