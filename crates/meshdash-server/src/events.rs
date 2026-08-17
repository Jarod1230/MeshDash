//! The live event stream, as a WebSocket.
//!
//! Pushes everything on the [`EventBus`] to whoever is connected, so a browser
//! sees a node appear or disappear without polling.
//!
//! # Why it authenticates differently
//!
//! A browser cannot set an `Authorization` header on a WebSocket connection —
//! the JavaScript API offers no way to. ADR-0006 left this open to be decided
//! here, and the choice is: **the first message after the upgrade carries the
//! token.**
//!
//! The obvious alternative, a token in the query string, was rejected. Query
//! strings end up in server logs, proxy logs and browser history, so the secret
//! would be written down in several places nobody thinks about. A message is
//! read once and kept nowhere.
//!
//! Until that message arrives, the connection is open but receives nothing, and
//! it is closed if the message does not come in time.

use std::time::Duration;

use axum::{
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::Response,
};
use meshdash_core::{config::AuthConfig, event::EventBus, module::AppContext};
use tokio::sync::broadcast::error::RecvError;

/// How long a client has to send its token before the connection is closed.
///
/// Long enough for a slow link, short enough that unauthenticated connections
/// cannot be left hanging to tie up resources.
const AUTH_TIMEOUT: Duration = Duration::from_secs(10);

/// Close code for a client that did not authenticate.
///
/// 1008 is "policy violation" — the closest thing the WebSocket protocol has to
/// "unauthorised".
const CLOSE_POLICY_VIOLATION: u16 = 1008;

/// Accepts the upgrade and hands the socket to the stream loop.
pub async fn handle_upgrade(
    upgrade: WebSocketUpgrade,
    State(context): State<AppContext>,
    auth: AuthConfig,
) -> Response {
    upgrade.on_upgrade(move |socket| stream_events(socket, context.events, auth))
}

/// Authenticates, then forwards events until the client goes away.
async fn stream_events(mut socket: WebSocket, events: EventBus, auth: AuthConfig) {
    if !authenticate(&mut socket, &auth).await {
        return;
    }

    // Subscribe only after authenticating — but note this means events that
    // arrived during the handshake are not delivered. The stream is a live
    // view, not a log; history comes from the modules' tables.
    let mut incoming = events.subscribe();

    loop {
        match incoming.recv().await {
            Ok(event) => {
                let Ok(json) = serde_json::to_string(&event) else {
                    tracing::error!("could not serialise an event, dropping it");
                    continue;
                };

                if socket.send(Message::Text(json.into())).await.is_err() {
                    // The client is gone; that is ordinary, not an error.
                    break;
                }
            }
            // A slow client missed events. Telling it is better than pretending
            // the stream was complete.
            Err(RecvError::Lagged(missed)) => {
                tracing::warn!(missed, "event subscriber fell behind");
            }
            Err(RecvError::Closed) => break,
        }
    }
}

/// Waits for the first message and checks the token in it.
///
/// Returns `true` when the connection may proceed. With no token configured,
/// nothing is expected and the stream starts right away.
async fn authenticate(socket: &mut WebSocket, auth: &AuthConfig) -> bool {
    let Some(expected) = auth.configured_token() else {
        return true;
    };

    let first = match tokio::time::timeout(AUTH_TIMEOUT, socket.recv()).await {
        Ok(Some(Ok(Message::Text(text)))) => text,
        // Timed out, closed, or something that is not a text message.
        _ => {
            close_unauthorised(socket, "expected a token as the first message").await;
            return false;
        }
    };

    // Accept the bare token as well as the `Bearer …` spelling, so a caller can
    // reuse whatever it sends on ordinary requests.
    let presented = first.trim();
    let presented = presented.strip_prefix("Bearer ").unwrap_or(presented);

    if !constant_time_eq(presented.as_bytes(), expected.as_bytes()) {
        // As with HTTP: never log what was presented.
        tracing::warn!("rejected an unauthenticated event stream");
        close_unauthorised(socket, "invalid token").await;
        return false;
    }

    true
}

/// Compares in constant time, per ADR-0006.
fn constant_time_eq(presented: &[u8], expected: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    presented.ct_eq(expected).into()
}

/// Closes the socket with a reason the client can show.
async fn close_unauthorised(socket: &mut WebSocket, reason: &'static str) {
    let _ = socket
        .send(Message::Close(Some(axum::extract::ws::CloseFrame {
            code: CLOSE_POLICY_VIOLATION,
            reason: reason.into(),
        })))
        .await;
}
