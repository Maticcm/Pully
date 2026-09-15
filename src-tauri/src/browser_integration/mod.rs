//! Browser Integration: lets Pully see supported pages currently open in a
//! browser (with the Pully extension installed and the feature turned on),
//! so a page shows up under "Detected in your browser" without the user
//! copying a link at all. See docs/BROWSER_INTEGRATION.md for the full
//! architecture, privacy posture, and setup/testing steps.
//!
//! Everything here only ever produces a URL, which then goes through
//! Pully's *existing* `extractors::route` / analysis / Quick Mode / download
//! pipeline — this module does not duplicate or bypass any of that.

pub mod framing;
pub mod ipc;
pub mod launcher;
pub mod messages;
pub mod native_messaging;
pub mod pipe_auth;
pub mod state;
pub mod validation;
