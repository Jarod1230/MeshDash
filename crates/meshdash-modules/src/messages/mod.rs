//! Direct messages the node received.
//!
//! The node only announces that something is waiting; the messages are then
//! fetched one at a time until it says there are no more. This module does that
//! and keeps what arrives, so a history exists even though the node's own queue
//! is emptied by reading it.
//!
//! # The sender is a prefix, not a contact
//!
//! A message names its sender with six bytes of their public key. Six bytes can
//! collide, so this module stores the prefix and does **not** decide which
//! contact it belongs to. Doing so would also mean reading another module's
//! tables, which the module rules forbid — `nodes` owns the contacts. Whoever
//! wants a name matches the prefix and treats the result as probable.

use std::sync::Arc;

use async_trait::async_trait;
use axum::{Json, Router, extract::State, routing::get};
use chrono::{DateTime, Utc};
use meshdash_core::{
    db::Migration,
    event::AppEvent,
    module::{AppContext, Module},
};
use meshdash_proto::{
    message::Message,
    opcode::{self, Command, Push, Response},
};
use serde::Serialize;

/// Schema of this module. Versions count from 1, per module.
const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    description: "received direct messages",
    sql: "
        CREATE TABLE messages_received (
            id             INTEGER PRIMARY KEY AUTOINCREMENT,
            sender_prefix  TEXT    NOT NULL,
            text           TEXT    NOT NULL,
            text_type      INTEGER NOT NULL,
            snr            REAL,
            path_len       INTEGER,
            sent_at        INTEGER NOT NULL,
            received_at    TEXT    NOT NULL
        );

        CREATE INDEX messages_received_sent_at ON messages_received (sent_at);
    ",
}];

/// Collects direct messages from the node.
#[derive(Debug, Default)]
pub struct MessagesModule;

/// One stored message, as the API reports it.
#[derive(Debug, Serialize, PartialEq)]
pub struct StoredMessage {
    /// Running number, ascending with arrival.
    pub id: i64,
    /// First six bytes of the sender's key, lowercase hex.
    ///
    /// A prefix, not an identity: two contacts can share one.
    pub sender_prefix: String,
    /// The message text.
    pub text: String,
    /// Firmware's text type, passed through as a number.
    pub text_type: u8,
    /// Signal-to-noise ratio in dB, if the node reported it.
    pub snr: Option<f32>,
    /// Hops the packet flooded over, or `None` if it did not flood.
    pub path_len: Option<u8>,
    /// When the sender stamped it, in seconds since the epoch.
    pub sent_at: u32,
    /// When MeshDash stored it.
    pub received_at: DateTime<Utc>,
}

#[async_trait]
impl Module for MessagesModule {
    fn name(&self) -> &'static str {
        "messages"
    }

    fn migrations(&self) -> &'static [Migration] {
        MIGRATIONS
    }

    fn routes(&self) -> Option<Router<AppContext>> {
        // A named path rather than "/": mounting a bare root inside a nested
        // router does not match, and "received" leaves room for a "sent"
        // counterpart once sending exists.
        Some(Router::new().route("/received", get(list_messages)))
    }

    async fn start(&self, context: &AppContext) -> Result<(), String> {
        let context = Arc::new(context.clone());
        let mut events = context.events.subscribe();

        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(AppEvent::Push { payload }) => {
                        // The node only rings the bell; the messages are
                        // fetched separately.
                        if is_message_waiting(&payload) {
                            if let Err(error) = drain_messages(&context).await {
                                tracing::warn!(error, "could not fetch waiting messages");
                            }
                        }
                    }
                    // Messages may have piled up while we were away.
                    Ok(AppEvent::NodeConnected) => {
                        if let Err(error) = drain_messages(&context).await {
                            tracing::warn!(error, "could not fetch messages after connecting");
                        }
                    }
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(missed)) => {
                        tracing::warn!(missed, "messages module missed events");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        Ok(())
    }
}

/// Whether a push announces waiting messages.
fn is_message_waiting(payload: &[u8]) -> bool {
    payload
        .first()
        .is_some_and(|&opcode| opcode::is_push(opcode) && Push::from(opcode) == Push::MsgWaiting)
}

/// Answers with the stored messages, newest first.
async fn list_messages(
    State(context): State<AppContext>,
) -> Result<Json<Vec<StoredMessage>>, ListError> {
    read_messages(&context).await.map(Json).map_err(ListError)
}

/// How many messages one drain fetches at most.
///
/// A node that keeps handing over valid messages would otherwise hold the
/// drain open indefinitely. Whatever is left waits for the next push.
pub const MAX_MESSAGES_PER_DRAIN: usize = 500;

/// Fetches messages until the node says there are none left.
///
/// Returns how many were stored.
pub async fn drain_messages(context: &AppContext) -> Result<usize, String> {
    let mut stored = 0;

    // One request per message, as the protocol prescribes: the node hands over
    // exactly one and says when the queue is empty.
    //
    // Bounded rather than a plain loop: the exits below all depend on the node
    // answering sensibly, and a node is exactly the thing that might not.
    for _ in 0..MAX_MESSAGES_PER_DRAIN {
        let answer = context
            .link
            .request(vec![u8::from(Command::SyncNextMessage)])
            .await
            .map_err(|error| error.to_string())?;

        match answer.first().map(|&opcode| Response::from(opcode)) {
            Some(Response::NoMoreMessages) => break,
            Some(Response::ContactMsgRecv | Response::ContactMsgRecvV3) => {}
            // Anything else is not an answer to this question. Asking again
            // would produce the same non-answer, and again after that: the
            // node ends up flooded with requests while nothing progresses.
            Some(other) => {
                tracing::warn!(
                    ?other,
                    "node answered the sync command with something else; stopping"
                );
                break;
            }
            None => break,
        }

        match Message::parse(&answer) {
            Ok(message) => {
                store_message(context, &message)
                    .await
                    .map_err(|error| error.to_string())?;
                stored += 1;
            }
            // One unreadable message must not stop the queue from draining;
            // the node would otherwise offer the same frame forever.
            Err(error) => tracing::warn!(%error, "skipping a message that could not be read"),
        }
    }

    if stored == MAX_MESSAGES_PER_DRAIN {
        tracing::warn!(
            stored,
            "stopped at the per-drain limit; the rest waits for the next push"
        );
    } else if stored > 0 {
        tracing::info!(stored, "stored waiting messages");
    }
    Ok(stored)
}

/// Stores one message.
pub async fn store_message(context: &AppContext, message: &Message) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO messages_received
            (sender_prefix, text, text_type, snr, path_len, sent_at, received_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(to_hex(&message.sender_prefix))
    .bind(&message.text)
    .bind(i64::from(text_type_as_byte(message.text_type)))
    .bind(message.snr.map(f64::from))
    .bind(message.path_len.map(i64::from))
    .bind(i64::from(message.sent_at))
    .bind(Utc::now().to_rfc3339())
    .execute(context.db.pool())
    .await?;

    Ok(())
}

/// The wire value of a text type, so an unknown one survives storage.
fn text_type_as_byte(text_type: meshdash_proto::message::TextType) -> u8 {
    use meshdash_proto::message::TextType;
    match text_type {
        TextType::Plain => 0,
        TextType::CliData => 1,
        TextType::SignedPlain => 2,
        TextType::Unknown(value) => value,
    }
}

/// One row of `messages_received`, in the order the query asks for it.
type MessageRow = (
    i64,
    String,
    String,
    i64,
    Option<f64>,
    Option<i64>,
    i64,
    String,
);

/// Reads stored messages, newest first.
pub async fn read_messages(context: &AppContext) -> Result<Vec<StoredMessage>, sqlx::Error> {
    let rows: Vec<MessageRow> = sqlx::query_as(
        "SELECT id, sender_prefix, text, text_type, snr, path_len, sent_at, received_at
         FROM messages_received ORDER BY id DESC",
    )
    .fetch_all(context.db.pool())
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            Some(StoredMessage {
                id: row.0,
                sender_prefix: row.1,
                text: row.2,
                text_type: row.3 as u8,
                snr: row.4.map(|value| value as f32),
                path_len: row.5.map(|value| value as u8),
                sent_at: row.6 as u32,
                received_at: match DateTime::parse_from_rfc3339(&row.7) {
                    Ok(time) => time.with_timezone(&Utc),
                    Err(error) => {
                        tracing::error!(%error, "stored timestamp is not RFC 3339");
                        return None;
                    }
                },
            })
        })
        .collect())
}

/// Turns bytes into lowercase hex.
fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}

/// Turns a storage failure into an API error.
#[derive(Debug)]
pub struct ListError(sqlx::Error);

impl axum::response::IntoResponse for ListError {
    fn into_response(self) -> axum::response::Response {
        tracing::error!(error = %self.0, "could not read the messages");

        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": { "code": "storage_failed", "message": "could not read the messages" }
            })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests;
