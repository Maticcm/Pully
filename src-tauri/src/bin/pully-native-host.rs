//! Chrome/Edge/Brave native-messaging host for the Pully browser extension.
//!
//! One instance of this process runs per `chrome.runtime.connectNative(...)`
//! call from the extension's background service worker, for the lifetime of
//! that connection. It is a thin, mostly-dumb relay in both directions:
//!
//! - reads Chromium's length-prefixed JSON frames on stdin (the extension's
//!   `Envelope` messages — see `pully_lib::browser_integration::messages`),
//!   answers `Ping` locally, and forwards everything else to Pully over a
//!   local named pipe;
//! - reads whatever Pully sends back down that same pipe (currently just
//!   `RequestState`, generated here the instant the pipe (re)connects, and
//!   `ThemeUpdate`, pushed by Pully on an interval) and forwards it verbatim
//!   to the extension as a native-messaging frame on stdout;
//! - keeps a background thread continuously retrying the pipe connection
//!   (rather than only reconnecting opportunistically when a message needs
//!   forwarding) so both directions catch up within a fraction of a second
//!   of Pully becoming reachable, not on the next incidental tab event;
//! - never itself downloads, analyzes, or executes anything.
//!
//! Per Chromium's native-messaging contract, **stdout is the protocol** —
//! nothing but framed JSON may ever be written there. All diagnostics go to
//! stderr instead.

use pully_lib::browser_integration::{
    framing::{read_frame, write_frame},
    ipc::PIPE_NAME,
    messages::{Envelope, PROTOCOL_VERSION},
};
use std::{
    fs::{File, OpenOptions},
    io::{self, Stdin, Stdout},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::Duration,
};

type SharedPipe = Arc<Mutex<Option<File>>>;

/// How often the background thread checks whether it needs to (re)connect.
/// Short enough that Pully starting up feels instant from the browser's
/// side; long enough not to matter for CPU/IO usage of an idle process.
const RECONNECT_POLL_INTERVAL: Duration = Duration::from_millis(300);

fn connect_to_pully() -> Option<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(PIPE_NAME)
        .ok()
}

/// Runs for the lifetime of the process. Keeps `pipe` populated as soon as
/// Pully's pipe server is reachable, independent of whether any browser
/// message happens to arrive during a particular window. Each time it
/// establishes a *new* connection, it hands a cloned, independently
/// readable handle to `on_reconnect` so the caller can both notify the
/// extension (`RequestState`) and start relaying whatever Pully sends down
/// that connection.
fn reconnect_loop(pipe: SharedPipe, on_reconnect: mpsc::Sender<File>, shutdown: Arc<AtomicBool>) {
    while !shutdown.load(Ordering::Relaxed) {
        let needs_connect = pipe.lock().map(|guard| guard.is_none()).unwrap_or(false);
        if needs_connect {
            if let Some(mut file) = connect_to_pully() {
                // The very first frame on a new connection must be Pully's
                // same-user auth token (see pully_lib::browser_integration::pipe_auth) —
                // Pully closes the connection immediately without it.
                let authenticated = pully_lib::browser_integration::pipe_auth::read_token()
                    .is_some_and(|token| write_frame(&mut file, token.as_bytes()).is_ok());
                if authenticated {
                    match file.try_clone() {
                        Ok(clone) => {
                            let _ = on_reconnect.send(clone);
                        }
                        Err(error) => {
                            eprintln!("[pully-native-host] could not clone pipe handle: {error}");
                        }
                    }
                    if let Ok(mut guard) = pipe.lock() {
                        *guard = Some(file);
                    }
                } else {
                    eprintln!(
                        "[pully-native-host] could not read Pully's pipe auth token; \
                         is Pully installed and has it run at least once?"
                    );
                }
            }
        }
        thread::sleep(RECONNECT_POLL_INTERVAL);
    }
}

fn respond(stdout: &Arc<Mutex<Stdout>>, envelope: &Envelope) {
    if let Ok(json) = serde_json::to_vec(envelope) {
        if let Ok(mut out) = stdout.lock() {
            let _ = write_frame(&mut *out, &json);
        }
    }
}

/// Relays every frame Pully sends down `pipe` straight to stdout, verbatim
/// (it's already valid `Envelope` JSON from Pully's side — no need to parse
/// and re-serialize). Runs until this particular connection's read fails,
/// at which point the reconnect loop above will eventually supply a new one.
fn relay_from_pully(mut pipe: File, stdout: &Arc<Mutex<Stdout>>) {
    while let Ok(Some(bytes)) = read_frame(&mut pipe) {
        if let Ok(mut out) = stdout.lock() {
            let _ = write_frame(&mut *out, &bytes);
        }
    }
}

/// Forwards one message to Pully. On a broken pipe, clears the shared handle
/// so the background reconnect thread notices and takes over — this call
/// itself never blocks retrying.
fn forward_to_pully(pipe: &SharedPipe, envelope: &Envelope) {
    let Ok(json) = serde_json::to_vec(envelope) else {
        return;
    };
    let mut guard = match pipe.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    if let Some(handle) = guard.as_mut() {
        if write_frame(handle, &json).is_ok() {
            return;
        }
        *guard = None;
    }
    // Not connected right now — dropped. The background thread will have a
    // fresh connection ready well before the next message typically arrives.
}

fn handle_message(stdin_frame: &[u8], stdout: &Arc<Mutex<Stdout>>, pipe: &SharedPipe) {
    let envelope = match serde_json::from_slice::<Envelope>(stdin_frame) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("[pully-native-host] dropping malformed message: {error}");
            return;
        }
    };
    if envelope.protocol_version() != PROTOCOL_VERSION {
        respond(
            stdout,
            &Envelope::ProtocolMismatch {
                protocol_version: PROTOCOL_VERSION,
                message: "Your Pully browser extension needs to be updated.".into(),
            },
        );
        return;
    }
    if matches!(envelope, Envelope::Ping { .. }) {
        respond(
            stdout,
            &Envelope::Pong {
                protocol_version: PROTOCOL_VERSION,
            },
        );
        return;
    }
    if let Envelope::OpenInPully { url, quick, .. } = &envelope {
        forward_to_pully(pipe, &envelope);
        let reached_pully = pipe.lock().map(|guard| guard.is_some()).unwrap_or(false);
        if !reached_pully {
            // An explicit user action (popup button or context-menu click),
            // not a passive tab update — launching Pully here is
            // intentional, matching what a double-click would do.
            pully_lib::browser_integration::launcher::launch(url, *quick);
        }
        return;
    }
    forward_to_pully(pipe, &envelope);
}

fn main() {
    let stdin: Stdin = io::stdin();
    let stdout = Arc::new(Mutex::new(io::stdout()));
    let mut stdin_lock = stdin.lock();

    let pipe: SharedPipe = Arc::new(Mutex::new(None));
    let shutdown = Arc::new(AtomicBool::new(false));
    let (reconnect_tx, reconnect_rx) = mpsc::channel::<File>();

    {
        let pipe = pipe.clone();
        let shutdown = shutdown.clone();
        thread::spawn(move || reconnect_loop(pipe, reconnect_tx, shutdown));
    }
    {
        let stdout = stdout.clone();
        thread::spawn(move || {
            // One connection's worth of Pully -> extension traffic at a
            // time; a fresh `File` arrives here each time the reconnect
            // loop establishes a new pipe connection.
            for file in reconnect_rx {
                respond(
                    &stdout,
                    &Envelope::RequestState {
                        protocol_version: PROTOCOL_VERSION,
                    },
                );
                relay_from_pully(file, &stdout);
            }
        });
    }

    loop {
        match read_frame(&mut stdin_lock) {
            Ok(Some(frame)) => handle_message(&frame, &stdout, &pipe),
            Ok(None) => break, // The extension closed the native-messaging port.
            Err(error) => {
                eprintln!("[pully-native-host] stdin read error: {error}");
                break;
            }
        }
    }
    shutdown.store(true, Ordering::Relaxed);
}
