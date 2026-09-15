//! In-memory, non-persisted state of tabs Pully currently knows about from
//! connected browser extensions. Nothing here ever touches disk — when a
//! browser session disconnects (or Pully exits), its tabs are simply gone,
//! per the feature's privacy requirements.

use super::messages::{Browser, Envelope, TabRecord};
use serde::Serialize;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedTab {
    /// `{browserInstanceId}:{tabId}` — tab ids are only unique within one
    /// browser session, so callers must not assume `tabId` alone is unique.
    pub key: String,
    pub browser_instance_id: String,
    pub browser: Browser,
    pub tab_id: String,
    pub window_id: Option<String>,
    pub url: String,
    pub title: String,
    pub active: bool,
    pub fav_icon_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserStatus {
    pub browser: Browser,
    pub connected: bool,
}

struct BrowserSession {
    browser: Browser,
    last_seen: Instant,
    tabs: HashMap<String, TabRecord>,
}

#[derive(Default)]
pub struct BrowserTabStore {
    sessions: HashMap<String, BrowserSession>,
}

impl BrowserTabStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies an already-validated envelope. Returns `true` when the
    /// visible tab set may have changed (so the caller knows whether it's
    /// worth pushing a fresh snapshot to the frontend).
    pub fn apply(&mut self, envelope: &Envelope) -> bool {
        match envelope {
            Envelope::Hello {
                browser,
                browser_instance_id,
                ..
            } => {
                self.touch(browser_instance_id, *browser);
                false
            }
            Envelope::BrowserState {
                browser_instance_id,
                browser,
                tabs,
                ..
            } => {
                let session = self.session_mut(browser_instance_id, *browser);
                session.tabs = tabs
                    .iter()
                    .cloned()
                    .map(|t| (t.tab_id.clone(), t))
                    .collect();
                true
            }
            Envelope::TabAdded {
                browser_instance_id,
                browser,
                tab,
                ..
            }
            | Envelope::TabUpdated {
                browser_instance_id,
                browser,
                tab,
                ..
            } => {
                let session = self.session_mut(browser_instance_id, *browser);
                session.tabs.insert(tab.tab_id.clone(), tab.clone());
                true
            }
            Envelope::TabRemoved {
                browser_instance_id,
                tab_id,
                ..
            } => {
                if let Some(session) = self.sessions.get_mut(browser_instance_id) {
                    session.tabs.remove(tab_id);
                }
                true
            }
            Envelope::ActiveTabChanged {
                browser_instance_id,
                tab_id,
                ..
            } => {
                if let Some(session) = self.sessions.get_mut(browser_instance_id) {
                    for (id, tab) in session.tabs.iter_mut() {
                        tab.active = id == tab_id;
                    }
                }
                true
            }
            Envelope::Ping { .. }
            | Envelope::Pong { .. }
            | Envelope::RequestState { .. }
            | Envelope::OpenInPully { .. }
            | Envelope::ProtocolMismatch { .. }
            | Envelope::ThemeUpdate { .. } => false,
        }
    }

    fn session_mut(&mut self, id: &str, browser: Browser) -> &mut BrowserSession {
        self.touch(id, browser);
        self.sessions.get_mut(id).expect("just inserted")
    }

    fn touch(&mut self, id: &str, browser: Browser) {
        let session = self
            .sessions
            .entry(id.to_string())
            .or_insert_with(|| BrowserSession {
                browser,
                last_seen: Instant::now(),
                tabs: HashMap::new(),
            });
        session.browser = browser;
        session.last_seen = Instant::now();
    }

    /// Drops an entire browser session's tabs — called when its
    /// native-messaging connection (and therefore its pipe connection) closes.
    pub fn remove_session(&mut self, id: &str) -> bool {
        self.sessions.remove(id).is_some()
    }

    pub fn snapshot(&self) -> Vec<DetectedTab> {
        self.sessions
            .iter()
            .flat_map(|(instance_id, session)| {
                session.tabs.values().map(move |tab| DetectedTab {
                    key: format!("{instance_id}:{}", tab.tab_id),
                    browser_instance_id: instance_id.clone(),
                    browser: session.browser,
                    tab_id: tab.tab_id.clone(),
                    window_id: tab.window_id.clone(),
                    url: tab.url.clone(),
                    title: tab.title.clone(),
                    active: tab.active,
                    fav_icon_url: tab.fav_icon_url.clone(),
                })
            })
            .collect()
    }

    /// One entry per browser family Pully has ever heard from this run,
    /// `true` if a session for it has been seen within `fresh_within`. A
    /// family Pully has never connected to simply doesn't appear — status is
    /// never guessed for a browser we have no evidence of.
    pub fn connection_status(&self, fresh_within: Duration) -> Vec<BrowserStatus> {
        let now = Instant::now();
        let mut seen: HashMap<Browser, bool> = HashMap::new();
        for session in self.sessions.values() {
            let fresh = now.duration_since(session.last_seen) <= fresh_within;
            let entry = seen.entry(session.browser).or_insert(false);
            *entry = *entry || fresh;
        }
        let mut rows: Vec<_> = seen
            .into_iter()
            .map(|(browser, connected)| BrowserStatus { browser, connected })
            .collect();
        rows.sort_by_key(|status| format!("{:?}", status.browser));
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser_integration::messages::PROTOCOL_VERSION;

    fn tab(id: &str, url: &str, active: bool) -> TabRecord {
        TabRecord {
            tab_id: id.into(),
            window_id: Some("1".into()),
            url: url.into(),
            title: "Title".into(),
            active,
            fav_icon_url: None,
        }
    }

    #[test]
    fn tracks_tabs_scoped_by_browser_instance() {
        let mut store = BrowserTabStore::new();
        store.apply(&Envelope::TabAdded {
            protocol_version: PROTOCOL_VERSION,
            browser_instance_id: "chrome-1".into(),
            browser: Browser::Chrome,
            tab: tab("5", "https://a.example", true),
        });
        store.apply(&Envelope::TabAdded {
            protocol_version: PROTOCOL_VERSION,
            browser_instance_id: "brave-1".into(),
            browser: Browser::Brave,
            tab: tab("5", "https://b.example", true),
        });
        let snapshot = store.snapshot();
        assert_eq!(snapshot.len(), 2);
        assert!(snapshot
            .iter()
            .any(|t| t.key == "chrome-1:5" && t.url.contains("a.example")));
        assert!(snapshot
            .iter()
            .any(|t| t.key == "brave-1:5" && t.url.contains("b.example")));
    }

    #[test]
    fn removing_a_tab_only_affects_its_own_browser_instance() {
        let mut store = BrowserTabStore::new();
        for instance in ["chrome-1", "brave-1"] {
            store.apply(&Envelope::TabAdded {
                protocol_version: PROTOCOL_VERSION,
                browser_instance_id: instance.into(),
                browser: Browser::Chrome,
                tab: tab("5", "https://example.com", true),
            });
        }
        store.apply(&Envelope::TabRemoved {
            protocol_version: PROTOCOL_VERSION,
            browser_instance_id: "chrome-1".into(),
            tab_id: "5".into(),
        });
        let snapshot = store.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].browser_instance_id, "brave-1");
    }

    #[test]
    fn disconnecting_a_session_drops_all_its_tabs() {
        let mut store = BrowserTabStore::new();
        store.apply(&Envelope::BrowserState {
            protocol_version: PROTOCOL_VERSION,
            browser_instance_id: "chrome-1".into(),
            browser: Browser::Chrome,
            tabs: vec![
                tab("1", "https://a.example", true),
                tab("2", "https://b.example", false),
            ],
        });
        assert_eq!(store.snapshot().len(), 2);
        assert!(store.remove_session("chrome-1"));
        assert!(store.snapshot().is_empty());
    }

    #[test]
    fn active_tab_changed_updates_active_flags_within_one_session() {
        let mut store = BrowserTabStore::new();
        store.apply(&Envelope::BrowserState {
            protocol_version: PROTOCOL_VERSION,
            browser_instance_id: "chrome-1".into(),
            browser: Browser::Chrome,
            tabs: vec![
                tab("1", "https://a.example", true),
                tab("2", "https://b.example", false),
            ],
        });
        store.apply(&Envelope::ActiveTabChanged {
            protocol_version: PROTOCOL_VERSION,
            browser_instance_id: "chrome-1".into(),
            tab_id: "2".into(),
        });
        let snapshot = store.snapshot();
        let active: Vec<_> = snapshot
            .iter()
            .filter(|t| t.active)
            .map(|t| t.tab_id.as_str())
            .collect();
        assert_eq!(active, vec!["2"]);
    }

    #[test]
    fn connection_status_only_lists_browsers_actually_seen() {
        let mut store = BrowserTabStore::new();
        store.apply(&Envelope::Hello {
            protocol_version: PROTOCOL_VERSION,
            browser: Browser::Chrome,
            browser_instance_id: "chrome-1".into(),
            extension_version: "1.0.0".into(),
        });
        let status = store.connection_status(Duration::from_secs(10));
        assert_eq!(
            status,
            vec![BrowserStatus {
                browser: Browser::Chrome,
                connected: true
            }]
        );
    }
}
