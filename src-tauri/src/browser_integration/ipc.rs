//! Local IPC between the native-messaging host process (`pully-native-host`,
//! one instance per connected browser extension) and this running Pully
//! instance. Deliberately narrow: the only thing that ever crosses this
//! boundary is the browser-integration `Envelope` protocol (see
//! `messages.rs`) — never arbitrary Tauri commands, shell commands, or
//! filesystem paths. Every message is re-validated on this side too
//! (`validation.rs`), independent of whatever the native host already
//! checked, since a compromised or out-of-date host binary is still
//! untrusted input as far as Pully itself is concerned.
//!
//! Windows-first: implemented with a native named pipe, which is local-only
//! (no network socket, nothing bindable from outside the machine) and needs
//! no extra dependency beyond tokio's own Windows named-pipe support.
//! macOS/Linux (a Unix domain socket carrying the same `Envelope` protocol
//! and the same `BrowserTabStore`) is not implemented yet — see
//! docs/BROWSER_INTEGRATION.md for that limitation and how to add it.

use super::{
    messages::{Envelope, PROTOCOL_VERSION},
    state::BrowserTabStore,
    validation::validate_envelope,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, OnceLock,
};
use tauri::AppHandle;
#[cfg(windows)]
use tauri::Manager;
use tokio::sync::Mutex;

/// Local pipe name, namespaced with the app identifier to avoid colliding
/// with anything else on the machine.
pub const PIPE_NAME: &str = r"\\.\pipe\app.pully.desktop.browser-bridge";

/// Shared, non-persisted state: the current set of tabs Pully knows about
/// from connected browser extensions, plus which browser families have been
/// heard from recently enough to call "connected".
pub struct BrowserIntegration {
    pub store: Arc<Mutex<BrowserTabStore>>,
    /// Strict opt-in: mirrors the user's "Enable browser integration"
    /// setting. While `false`, inbound messages are read (so the pipe
    /// connection doesn't stall) but never applied to `store` or emitted to
    /// the frontend — nothing is tracked unless the user has turned this on.
    pub enabled: AtomicBool,
    /// The token `pully-native-host` must present as the very first frame
    /// on a new pipe connection (see `pipe_auth`). Set once at startup,
    /// before the pipe server starts accepting connections — never `None`
    /// by the time a connection could arrive, unless the token couldn't be
    /// written at all, in which case connections are refused outright
    /// rather than accepted unauthenticated.
    pub expected_token: OnceLock<String>,
}

impl Default for BrowserIntegration {
    fn default() -> Self {
        Self {
            store: Arc::new(Mutex::new(BrowserTabStore::new())),
            enabled: AtomicBool::new(false),
            expected_token: OnceLock::new(),
        }
    }
}

#[cfg(windows)]
pub fn start(app: AppHandle) {
    windows_impl::start(app);
}

#[cfg(not(windows))]
pub fn start(_app: AppHandle) {
    eprintln!(
        "[browser-integration] no local IPC transport implemented for this platform yet; \
         browser tab detection will stay empty. See docs/BROWSER_INTEGRATION.md."
    );
}

/// Sends `envelope` (in practice, always a `ThemeUpdate`) to every currently
/// connected `pully-native-host` session. A no-op where there's no peer
/// registry to broadcast through (nothing connected, or not Windows).
#[cfg(windows)]
pub async fn broadcast(app: &AppHandle, envelope: Envelope) {
    if let Some(peers) = app.try_state::<windows_impl::PeerRegistry>() {
        peers.broadcast(&envelope).await;
    }
}

#[cfg(not(windows))]
pub async fn broadcast(_app: &AppHandle, _envelope: Envelope) {}

#[cfg(windows)]
mod windows_impl {
    use super::*;
    use crate::browser_integration::messages::limits::MAX_FRAME_BYTES;
    use std::{collections::HashMap, sync::atomic::AtomicU64};
    use tauri::{Emitter, Manager};
    use tokio::{
        io::{split, AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf},
        net::windows::named_pipe::{NamedPipeServer, ServerOptions},
    };

    /// The write half of every currently-connected `pully-native-host`
    /// pipe session, so `push_theme` (a Tauri command, see `commands.rs`)
    /// can broadcast a `ThemeUpdate` to all of them without needing to know
    /// anything about individual browser sessions.
    #[derive(Default)]
    pub struct PeerRegistry {
        next_id: AtomicU64,
        writers: Mutex<HashMap<u64, WriteHalf<NamedPipeServer>>>,
    }

    impl PeerRegistry {
        async fn insert(&self, writer: WriteHalf<NamedPipeServer>) -> u64 {
            let id = self.next_id.fetch_add(1, Ordering::Relaxed);
            self.writers.lock().await.insert(id, writer);
            id
        }

        async fn remove(&self, id: u64) {
            self.writers.lock().await.remove(&id);
        }

        /// Sends `envelope` to every connected peer, dropping any whose
        /// write fails (the read side of that same connection will notice
        /// independently and clean up via `remove`).
        pub async fn broadcast(&self, envelope: &Envelope) {
            let Ok(payload) = serde_json::to_vec(envelope) else {
                return;
            };
            let mut writers = self.writers.lock().await;
            let mut dead = Vec::new();
            for (id, writer) in writers.iter_mut() {
                if write_frame(writer, &payload).await.is_err() {
                    dead.push(*id);
                }
            }
            for id in dead {
                writers.remove(&id);
            }
        }
    }

    pub fn start(app: AppHandle) {
        app.manage(PeerRegistry::default());
        tauri::async_runtime::spawn(async move {
            loop {
                let server = match ServerOptions::new()
                    .reject_remote_clients(true)
                    .in_buffer_size(64 * 1024)
                    .out_buffer_size(64 * 1024)
                    .create(PIPE_NAME)
                {
                    Ok(server) => server,
                    Err(error) => {
                        eprintln!("[browser-integration] failed to create named pipe: {error}");
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        continue;
                    }
                };
                if let Err(error) = server.connect().await {
                    eprintln!("[browser-integration] pipe connect failed: {error}");
                    continue;
                }
                let app = app.clone();
                tauri::async_runtime::spawn(handle_connection(app, server));
            }
        });
    }

    async fn handle_connection(app: AppHandle, pipe: NamedPipeServer) {
        let integration = app.state::<BrowserIntegration>();
        let (mut reader, writer) = split(pipe);

        // The very first frame on every connection must be the same-user
        // auth token (see `pipe_auth`) — read and checked before this
        // connection is registered as a peer or any Envelope is parsed, so
        // an unauthenticated caller gets nothing but a closed connection.
        let expected = integration.expected_token.get();
        let presented = read_frame(&mut reader).await.ok().flatten();
        let authenticated = match (expected, &presented) {
            (Some(expected), Some(token)) => token.as_slice() == expected.as_bytes(),
            _ => false,
        };
        if !authenticated {
            eprintln!("[browser-integration] rejected a pipe connection with a missing/invalid auth token");
            return;
        }

        let peers = app.state::<PeerRegistry>();
        let store = integration.store.clone();
        let peer_id = peers.insert(writer).await;
        let mut session_id: Option<String> = None;
        loop {
            let frame = match read_frame(&mut reader).await {
                Ok(Some(bytes)) => bytes,
                Ok(None) => break,
                Err(error) => {
                    eprintln!("[browser-integration] pipe read error: {error}");
                    break;
                }
            };
            let Ok(raw) = serde_json::from_slice::<Envelope>(&frame) else {
                continue;
            };
            let Ok(envelope) = validate_envelope(raw, PROTOCOL_VERSION) else {
                continue;
            };
            if !integration.enabled.load(Ordering::Relaxed) {
                // Strict opt-in: keep draining the connection so it doesn't
                // stall, but never track or surface anything while disabled.
                continue;
            }
            if let Envelope::Hello {
                browser_instance_id,
                ..
            } = &envelope
            {
                session_id = Some(browser_instance_id.clone());
            }
            if let Envelope::OpenInPully { url, quick, .. } = &envelope {
                let _ = app.emit(
                    "browser-open-request",
                    serde_json::json!({ "url": url, "quick": quick }),
                );
                // "Open in Pully" is the one browser-originated action
                // allowed to bring the window forward — everything else
                // (tab open/close/update) stays silent in the background.
                // Quick Download deliberately does *not* do this: it must
                // keep working with the window hidden in the tray (that's
                // the whole point of tray mode), so it leaves visibility
                // exactly as it already was.
                if !quick {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.unminimize();
                        let _ = window.set_focus();
                    }
                }
                continue;
            }
            let changed = store.lock().await.apply(&envelope);
            if changed {
                let snapshot = store.lock().await.snapshot();
                let _ = app.emit("browser-tabs-changed", snapshot);
            }
        }
        peers.remove(peer_id).await;
        if let Some(id) = session_id {
            let changed = store.lock().await.remove_session(&id);
            if changed {
                let snapshot = store.lock().await.snapshot();
                let _ = app.emit("browser-tabs-changed", snapshot);
            }
        }
    }

    async fn read_frame(
        reader: &mut ReadHalf<NamedPipeServer>,
    ) -> std::io::Result<Option<Vec<u8>>> {
        let mut len_bytes = [0u8; 4];
        if let Err(error) = reader.read_exact(&mut len_bytes).await {
            return if error.kind() == std::io::ErrorKind::UnexpectedEof {
                Ok(None)
            } else {
                Err(error)
            };
        }
        let len = u32::from_ne_bytes(len_bytes) as usize;
        if len > MAX_FRAME_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "frame too large",
            ));
        }
        let mut buffer = vec![0u8; len];
        reader.read_exact(&mut buffer).await?;
        Ok(Some(buffer))
    }

    async fn write_frame(
        writer: &mut WriteHalf<NamedPipeServer>,
        payload: &[u8],
    ) -> std::io::Result<()> {
        if payload.len() > MAX_FRAME_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "frame too large",
            ));
        }
        writer
            .write_all(&(payload.len() as u32).to_ne_bytes())
            .await?;
        writer.write_all(payload).await?;
        writer.flush().await
    }
}
