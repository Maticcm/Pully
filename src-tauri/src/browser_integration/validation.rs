//! Everything arriving from a browser extension is untrusted input. This
//! module is the single place that decides whether an inbound `Envelope` (or
//! a `TabRecord` inside one) is safe to accept, independent of whichever
//! transport carried it (native messaging framing or the local pipe).

use super::messages::{limits, Envelope, TabRecord};

#[derive(Debug, PartialEq, Eq)]
pub enum Rejection {
    UnsupportedUrlScheme,
    UrlTooLong,
    TitleTooLong,
    IdentifierInvalid,
    ProtocolMismatch,
}

/// Only `http`/`https` may ever reach Pully's analysis pipeline from a
/// browser tab — the same rule `extractors::validate_url` enforces for
/// manually typed links, applied here before a browser URL is even stored.
pub fn validate_url(url: &str) -> Result<(), Rejection> {
    if url.len() > limits::MAX_URL_LEN {
        return Err(Rejection::UrlTooLong);
    }
    let Ok(parsed) = url::Url::parse(url) else {
        return Err(Rejection::UnsupportedUrlScheme);
    };
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(Rejection::UnsupportedUrlScheme);
    }
    Ok(())
}

fn validate_identifier(value: &str) -> Result<(), Rejection> {
    if value.is_empty() || value.len() > limits::MAX_ID_LEN || value.chars().any(char::is_control) {
        return Err(Rejection::IdentifierInvalid);
    }
    Ok(())
}

/// Validates a tab record in place, truncating an over-long title rather
/// than rejecting the whole tab for it (titles are display-only and
/// attacker-controlled length there is a nuisance, not a safety issue).
pub fn validate_tab(tab: &mut TabRecord) -> Result<(), Rejection> {
    validate_identifier(&tab.tab_id)?;
    if let Some(window_id) = &tab.window_id {
        validate_identifier(window_id)?;
    }
    validate_url(&tab.url)?;
    if tab.title.len() > limits::MAX_TITLE_LEN {
        tab.title.truncate(limits::MAX_TITLE_LEN);
    }
    if let Some(icon) = &tab.fav_icon_url {
        if icon.len() > limits::MAX_URL_LEN
            || !(icon.starts_with("http://")
                || icon.starts_with("https://")
                || icon.starts_with("data:image/"))
        {
            tab.fav_icon_url = None;
        }
    }
    Ok(())
}

/// Validates an envelope's protocol version and, for variants that carry
/// tabs/URLs/identifiers, their contents. Returns the envelope back
/// (with tab titles possibly truncated) so the caller doesn't need a
/// separate mutable pass.
pub fn validate_envelope(
    mut envelope: Envelope,
    expected_version: u32,
) -> Result<Envelope, Rejection> {
    if envelope.protocol_version() != expected_version {
        return Err(Rejection::ProtocolMismatch);
    }
    match &mut envelope {
        Envelope::Hello {
            browser_instance_id,
            ..
        } => validate_identifier(browser_instance_id)?,
        Envelope::BrowserState {
            browser_instance_id,
            tabs,
            ..
        } => {
            validate_identifier(browser_instance_id)?;
            if tabs.len() > limits::MAX_TABS_PER_BROWSER {
                tabs.truncate(limits::MAX_TABS_PER_BROWSER);
            }
            for tab in tabs.iter_mut() {
                validate_tab(tab)?;
            }
        }
        Envelope::TabAdded {
            browser_instance_id,
            tab,
            ..
        }
        | Envelope::TabUpdated {
            browser_instance_id,
            tab,
            ..
        } => {
            validate_identifier(browser_instance_id)?;
            validate_tab(tab)?;
        }
        Envelope::TabRemoved {
            browser_instance_id,
            tab_id,
            ..
        } => {
            validate_identifier(browser_instance_id)?;
            validate_identifier(tab_id)?;
        }
        Envelope::ActiveTabChanged {
            browser_instance_id,
            tab_id,
            ..
        } => {
            validate_identifier(browser_instance_id)?;
            validate_identifier(tab_id)?;
        }
        Envelope::OpenInPully { url, .. } => validate_url(url)?,
        Envelope::Ping { .. }
        | Envelope::Pong { .. }
        | Envelope::RequestState { .. }
        | Envelope::ProtocolMismatch { .. }
        | Envelope::ThemeUpdate { .. } => {}
    }
    Ok(envelope)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser_integration::messages::{Browser, PROTOCOL_VERSION};

    fn tab(url: &str) -> TabRecord {
        TabRecord {
            tab_id: "1".into(),
            window_id: Some("1".into()),
            url: url.into(),
            title: "Title".into(),
            active: true,
            fav_icon_url: None,
        }
    }

    #[test]
    fn accepts_http_and_https() {
        assert!(validate_url("https://example.com/x").is_ok());
        assert!(validate_url("http://example.com/x").is_ok());
    }

    #[test]
    fn rejects_non_http_schemes() {
        assert_eq!(
            validate_url("chrome://settings"),
            Err(Rejection::UnsupportedUrlScheme)
        );
        assert_eq!(
            validate_url("file:///secret"),
            Err(Rejection::UnsupportedUrlScheme)
        );
        assert_eq!(
            validate_url("javascript:alert(1)"),
            Err(Rejection::UnsupportedUrlScheme)
        );
    }

    #[test]
    fn rejects_oversized_urls() {
        let huge = format!("https://example.com/{}", "a".repeat(5000));
        assert_eq!(validate_url(&huge), Err(Rejection::UrlTooLong));
    }

    #[test]
    fn truncates_rather_than_rejects_long_titles() {
        let mut record = tab("https://example.com");
        record.title = "x".repeat(10_000);
        validate_tab(&mut record).unwrap();
        assert_eq!(record.title.len(), limits::MAX_TITLE_LEN);
    }

    #[test]
    fn drops_a_suspicious_favicon_instead_of_failing_the_whole_tab() {
        let mut record = tab("https://example.com");
        record.fav_icon_url = Some("javascript:alert(1)".into());
        validate_tab(&mut record).unwrap();
        assert_eq!(record.fav_icon_url, None);
    }

    #[test]
    fn caps_tabs_per_browser_state_snapshot() {
        let tabs: Vec<TabRecord> = (0..(limits::MAX_TABS_PER_BROWSER + 50))
            .map(|i| TabRecord {
                tab_id: i.to_string(),
                ..tab("https://example.com")
            })
            .collect();
        let envelope = Envelope::BrowserState {
            protocol_version: PROTOCOL_VERSION,
            browser_instance_id: "chrome-1".into(),
            browser: Browser::Chrome,
            tabs,
        };
        let validated = validate_envelope(envelope, PROTOCOL_VERSION).unwrap();
        let Envelope::BrowserState { tabs, .. } = validated else {
            unreachable!()
        };
        assert_eq!(tabs.len(), limits::MAX_TABS_PER_BROWSER);
    }

    #[test]
    fn rejects_a_protocol_version_mismatch() {
        let envelope = Envelope::Ping {
            protocol_version: 999,
        };
        assert_eq!(
            validate_envelope(envelope, PROTOCOL_VERSION).unwrap_err(),
            Rejection::ProtocolMismatch
        );
    }
}
