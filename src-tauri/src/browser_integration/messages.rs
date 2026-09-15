//! Wire protocol shared, conceptually, between the Pully browser extension,
//! the native-messaging host (`pully-native-host`), and the Pully app. The
//! extension and the native host both speak this exact envelope over
//! Chromium's length-prefixed JSON native-messaging framing; the native host
//! re-frames the same envelope over the local named pipe to reach Pully, so
//! there is exactly one message schema end to end instead of two that could
//! drift apart. The TypeScript mirror lives at
//! `browser-extension/src/messaging/protocol.ts` — keep the two in sync by
//! hand; there is no code generator for this yet (see docs/BROWSER_INTEGRATION.md).

use serde::{Deserialize, Serialize};

/// Bumped whenever the wire shape changes in a way older peers can't parse.
pub const PROTOCOL_VERSION: u32 = 1;

/// Hard caps applied before/while validating any inbound message. These are
/// deliberately generous for legitimate use and stingy against abuse.
pub mod limits {
    pub const MAX_FRAME_BYTES: usize = 1024 * 1024; // 1 MiB per native-messaging frame
    pub const MAX_URL_LEN: usize = 4096;
    pub const MAX_TITLE_LEN: usize = 300;
    pub const MAX_ID_LEN: usize = 128;
    pub const MAX_TABS_PER_BROWSER: usize = 500;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Browser {
    Chrome,
    Brave,
    Edge,
    /// Any other Chromium-family browser the extension couldn't name more
    /// specifically — still fully supported, just shown generically.
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TabRecord {
    pub tab_id: String,
    pub window_id: Option<String>,
    pub url: String,
    pub title: String,
    pub active: bool,
    pub fav_icon_url: Option<String>,
}

/// One message envelope, tagged by `type`. This is the exact JSON shape sent
/// over native messaging (extension -> host) and over the local pipe
/// (host -> Pully). `protocol_version` rides on every message so either side
/// can detect and report a mismatch instead of misparsing silently.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Envelope {
    Hello {
        protocol_version: u32,
        browser: Browser,
        browser_instance_id: String,
        extension_version: String,
    },
    Ping {
        protocol_version: u32,
    },
    Pong {
        protocol_version: u32,
    },
    /// Sent from `pully-native-host` down to the extension (never forwarded
    /// to Pully) the moment its pipe connection to Pully comes up — including
    /// right after Pully starts, or reconnects following a restart. Prompts
    /// an immediate `BrowserState` resync instead of waiting on the next
    /// incidental tab event or the periodic safety-net resync, so detection
    /// catches up within about half a second of Pully becoming reachable
    /// rather than whenever the browser happens to do something next.
    RequestState {
        protocol_version: u32,
    },
    /// A full snapshot, sent right after `Hello` and whenever the extension
    /// wants to resync (e.g. after waking from an idle service worker).
    BrowserState {
        protocol_version: u32,
        browser_instance_id: String,
        browser: Browser,
        tabs: Vec<TabRecord>,
    },
    TabAdded {
        protocol_version: u32,
        browser_instance_id: String,
        browser: Browser,
        tab: TabRecord,
    },
    TabUpdated {
        protocol_version: u32,
        browser_instance_id: String,
        browser: Browser,
        tab: TabRecord,
    },
    TabRemoved {
        protocol_version: u32,
        browser_instance_id: String,
        tab_id: String,
    },
    ActiveTabChanged {
        protocol_version: u32,
        browser_instance_id: String,
        tab_id: String,
    },
    /// An explicit user action from the popup or a context menu — the only
    /// browser-originated message allowed to bring Pully's window forward.
    OpenInPully {
        protocol_version: u32,
        url: String,
        quick: bool,
    },
    /// Sent by Pully back down the pipe/native-messaging connection when it
    /// rejects a message, so the extension can surface something useful
    /// ("update your extension") instead of failing silently.
    ProtocolMismatch {
        protocol_version: u32,
        message: String,
    },
    /// Pully -> extension only (never the reverse). A snapshot of Pully's
    /// *currently applied* theme, already fully resolved by the frontend
    /// (accent/canvas/surface as "r g b" triplets, the concrete font-family
    /// stack, and whether dark mode is active) so neither the native host
    /// nor the extension need to know anything about how Pully derives
    /// those from its settings (accent color picker, base color, font
    /// choice, light/dark/system). Pushed on an interval from the frontend
    /// (see `App.tsx`), not computed or cached on the Rust side.
    ThemeUpdate {
        protocol_version: u32,
        accent_color: String,
        accent_foreground: String,
        canvas: String,
        surface: String,
        dark: bool,
        font_family: String,
    },
}

impl Envelope {
    pub fn protocol_version(&self) -> u32 {
        match self {
            Envelope::Hello {
                protocol_version, ..
            }
            | Envelope::Ping { protocol_version }
            | Envelope::Pong { protocol_version }
            | Envelope::RequestState { protocol_version }
            | Envelope::BrowserState {
                protocol_version, ..
            }
            | Envelope::TabAdded {
                protocol_version, ..
            }
            | Envelope::TabUpdated {
                protocol_version, ..
            }
            | Envelope::TabRemoved {
                protocol_version, ..
            }
            | Envelope::ActiveTabChanged {
                protocol_version, ..
            }
            | Envelope::OpenInPully {
                protocol_version, ..
            }
            | Envelope::ProtocolMismatch {
                protocol_version, ..
            }
            | Envelope::ThemeUpdate {
                protocol_version, ..
            } => *protocol_version,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_tab_added_envelope() {
        let envelope = Envelope::TabAdded {
            protocol_version: PROTOCOL_VERSION,
            browser_instance_id: "chrome-abc123".into(),
            browser: Browser::Chrome,
            tab: TabRecord {
                tab_id: "42".into(),
                window_id: Some("1".into()),
                url: "https://example.com/watch".into(),
                title: "Example".into(),
                active: true,
                fav_icon_url: None,
            },
        };
        let json = serde_json::to_string(&envelope).unwrap();
        assert!(json.contains("\"type\":\"tabAdded\""));
        let parsed: Envelope = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.protocol_version(), PROTOCOL_VERSION);
    }
}
