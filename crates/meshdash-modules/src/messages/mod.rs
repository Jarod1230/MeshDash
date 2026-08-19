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
//!
//! # Channels arrive through the same queue
//!
//! A channel message is queued and announced exactly like a direct one and is
//! handed over in response to the same `CMD_SYNC_NEXT_MESSAGE`. Only its layout
//! differs, so the drain has to accept both — a drain that only knows direct
//! messages stops dead at the first channel message.
//!
//! A channel message has **no sender field**: the sending firmware writes the
//! node name into the text before broadcasting. Nothing here can attribute it.
//!
//! # What this module publishes
//!
//! Every message it stores, direct or channel, is published on the bus as
//! `AppEvent::Module` with module `messages` and kind `signal`:
//!
//! ```json
//! { "snr": -2.5, "path_len": 2, "source": "direct" }
//! ```
//!
//! `snr` may be absent, `path_len` may be absent. That is how `telemetry` gets
//! the reception quality without reading this module's tables — see
//! `docs/decisions/0007-modul-ereignisse.md`.

use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use meshdash_core::{
    db::Migration,
    event::AppEvent,
    module::{AppContext, Module},
};
use meshdash_proto::{
    channel::{ChannelInfo, ChannelMessage},
    message::{Message, TextType},
    opcode::{self, Command, Push, Response},
    send::{self, SendReceipt},
};
use serde::{Deserialize, Serialize};

/// Schema of this module. Versions count from 1, per module.
const MIGRATIONS: &[Migration] = &[
    Migration {
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
    },
    Migration {
        version: 2,
        description: "channel messages, known channels and a record of what was sent",
        sql: "
        CREATE TABLE messages_channel_received (
            id             INTEGER PRIMARY KEY AUTOINCREMENT,
            channel_index  INTEGER NOT NULL,
            text           TEXT    NOT NULL,
            text_type      INTEGER NOT NULL,
            snr            REAL,
            path_len       INTEGER,
            sent_at        INTEGER NOT NULL,
            received_at    TEXT    NOT NULL
        );

        CREATE INDEX messages_channel_received_sent_at
            ON messages_channel_received (sent_at);

        -- The shared key of a channel is deliberately absent: whoever holds it
        -- can read and write the channel, and a column that does not exist
        -- cannot leak through an API response or a backup.
        CREATE TABLE messages_channels (
            channel_index  INTEGER PRIMARY KEY,
            name           TEXT    NOT NULL,
            seen_at        TEXT    NOT NULL
        );

        CREATE TABLE messages_sent (
            id             INTEGER PRIMARY KEY AUTOINCREMENT,
            target         TEXT    NOT NULL,
            text           TEXT    NOT NULL,
            sent_at        TEXT    NOT NULL,
            flooded        INTEGER,
            expected_ack   TEXT
        );
    ",
    },
];

/// How many messages a listing returns unless it asks for fewer.
///
/// Both message tables grow with every message the mesh delivers and nothing
/// prunes them. An unbounded read would eventually try to serialise a year of
/// traffic into one response — see `docs/conventions.md`.
const DEFAULT_LIMIT: i64 = 500;

/// Largest number of messages a single request may ask for.
const MAX_LIMIT: i64 = 5_000;

/// How many messages to return.
#[derive(Debug, Deserialize, Default)]
pub struct ListQuery {
    /// Upper bound, capped at [`MAX_LIMIT`].
    limit: Option<i64>,
}

impl ListQuery {
    /// The number of rows to read, within the bounds this module allows.
    fn effective_limit(&self) -> i64 {
        self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
    }
}

/// Highest channel index the node can possibly have.
///
/// The index travels as a single byte, so 255 is not an assumption about the
/// firmware — it is the width of the field. Probing stops at the first index
/// the node does not know anyway.
const MAX_CHANNEL_INDEX: u8 = u8::MAX;

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

/// One stored channel message, as the API reports it.
#[derive(Debug, Serialize, PartialEq)]
pub struct StoredChannelMessage {
    /// Running number, ascending with arrival.
    pub id: i64,
    /// Which channel it came in on.
    pub channel_index: u8,
    /// The message text. Contains the sender's name, put there by their node.
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

/// One channel the node knows, without its key.
#[derive(Debug, Serialize, PartialEq)]
pub struct KnownChannel {
    /// Position in the node's channel table; how a channel is addressed.
    pub channel_index: u8,
    /// Display name.
    pub name: String,
    /// When MeshDash last read this channel from the node.
    pub seen_at: DateTime<Utc>,
}

/// A message to send to one contact.
#[derive(Debug, Deserialize)]
pub struct DirectRequest {
    /// First six bytes of the recipient's public key, lowercase hex.
    pub recipient_prefix: String,
    /// What to send.
    pub text: String,
}

/// A message to send to one channel.
#[derive(Debug, Deserialize)]
pub struct ChannelRequest {
    /// Which channel to send on.
    pub channel_index: u8,
    /// What to send.
    pub text: String,
}

/// What the node reported about a direct message it took.
#[derive(Debug, Serialize, PartialEq)]
pub struct SendResult {
    /// Whether it went out as a flood rather than along a known route.
    pub flooded: bool,
    /// The acknowledgement to expect back, lowercase hex, or `None`.
    pub expected_ack: Option<String>,
    /// The node's own estimate of the round trip, in milliseconds.
    pub estimated_timeout_ms: u32,
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
        Some(
            Router::new()
                .route("/received", get(list_messages))
                .route("/channel-received", get(list_channel_messages))
                .route("/channels", get(list_channels))
                .route("/send", post(send_direct))
                .route("/channel-send", post(send_to_channel)),
        )
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
                        // Which channel an index means is the node's answer to
                        // give, and it can change while we are away.
                        if let Err(error) = sync_channels(&context).await {
                            tracing::warn!(error, "could not read the channel list");
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
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<StoredMessage>>, ListError> {
    read_messages(&context, query.effective_limit())
        .await
        .map(Json)
        .map_err(ListError)
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

        // Both kinds come out of the same queue, so both have to be taken
        // here; anything else means the node is no longer answering the
        // question that was asked.
        match answer.first().map(|&opcode| Response::from(opcode)) {
            Some(Response::NoMoreMessages) => break,
            Some(Response::ContactMsgRecv | Response::ContactMsgRecvV3) => {
                match Message::parse(&answer) {
                    Ok(message) => {
                        store_message(context, &message)
                            .await
                            .map_err(|error| error.to_string())?;
                        publish_signal(context, "direct", message.snr, message.path_len);
                        stored += 1;
                    }
                    // One unreadable message must not stop the queue from
                    // draining; the node would otherwise offer the same frame
                    // forever.
                    Err(error) => {
                        tracing::warn!(%error, "skipping a message that could not be read");
                    }
                }
            }
            Some(Response::ChannelMsgRecv | Response::ChannelMsgRecvV3) => {
                match ChannelMessage::parse(&answer) {
                    Ok(message) => {
                        store_channel_message(context, &message)
                            .await
                            .map_err(|error| error.to_string())?;
                        publish_signal(context, "channel", message.snr, message.path_len);
                        stored += 1;
                    }
                    Err(error) => {
                        tracing::warn!(%error, "skipping a channel message that could not be read");
                    }
                }
            }
            // Asking again would produce the same non-answer, and again after
            // that: the node ends up flooded while nothing progresses.
            Some(other) => {
                tracing::warn!(
                    ?other,
                    "node answered the sync command with something else; stopping"
                );
                break;
            }
            None => break,
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
    .bind(i64::from(message.text_type.as_byte()))
    .bind(message.snr.map(f64::from))
    .bind(message.path_len.map(i64::from))
    .bind(i64::from(message.sent_at))
    .bind(Utc::now().to_rfc3339())
    .execute(context.db.pool())
    .await?;

    Ok(())
}

/// Announces the reception quality of one message on the bus.
///
/// Fire and forget: nobody may be listening, and this module must not care.
/// Publishing when the bus is empty is not an error.
fn publish_signal(context: &AppContext, source: &str, snr: Option<f32>, path_len: Option<u8>) {
    context.events.publish(AppEvent::Module {
        module: "messages".into(),
        kind: "signal".into(),
        data: serde_json::json!({
            "source": source,
            "snr": snr,
            "path_len": path_len,
        }),
    });
}

/// Stores one channel message.
pub async fn store_channel_message(
    context: &AppContext,
    message: &ChannelMessage,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO messages_channel_received
            (channel_index, text, text_type, snr, path_len, sent_at, received_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(i64::from(message.channel_index))
    .bind(&message.text)
    .bind(i64::from(message.text_type.as_byte()))
    .bind(message.snr.map(f64::from))
    .bind(message.path_len.map(i64::from))
    .bind(i64::from(message.sent_at))
    .bind(Utc::now().to_rfc3339())
    .execute(context.db.pool())
    .await?;

    Ok(())
}

/// Reads the node's channel list and records it.
///
/// Returns how many channels the node knows.
pub async fn sync_channels(context: &AppContext) -> Result<usize, String> {
    let mut found = 0;

    // There is no "list channels" command — only "describe this index". The
    // node answers ERR_CODE_NOT_FOUND past its last one, which is where the
    // list ends.
    for index in 0..=MAX_CHANNEL_INDEX {
        let answer = context
            .link
            .request(vec![u8::from(Command::GetChannel), index])
            .await
            .map_err(|error| error.to_string())?;

        let Ok(info) = ChannelInfo::parse(&answer) else {
            break;
        };

        store_channel(context, &info)
            .await
            .map_err(|error| error.to_string())?;
        found += 1;
    }

    tracing::info!(found, "read the channel list");
    Ok(found)
}

/// Stores one channel description. Never its key.
pub async fn store_channel(context: &AppContext, info: &ChannelInfo) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO messages_channels (channel_index, name, seen_at)
         VALUES (?, ?, ?)
         ON CONFLICT (channel_index) DO UPDATE SET
            name = excluded.name,
            seen_at = excluded.seen_at",
    )
    .bind(i64::from(info.index))
    .bind(&info.name)
    .bind(Utc::now().to_rfc3339())
    .execute(context.db.pool())
    .await?;

    Ok(())
}

/// Sends one message to a contact and records the attempt.
pub async fn send_message(
    context: &AppContext,
    recipient_prefix: [u8; 6],
    text: &str,
) -> Result<SendResult, SendFailure> {
    // The node stamps nothing for us: the timestamp in the frame is ours, and
    // the recipient checks it against replay protection.
    let timestamp = Utc::now().timestamp().clamp(0, i64::from(u32::MAX)) as u32;
    let frame = send::encode_direct(recipient_prefix, TextType::Plain, 0, timestamp, text)
        .map_err(|error| SendFailure::Rejected(error.to_string()))?;

    let answer = context
        .link
        .request(frame)
        .await
        .map_err(|error| SendFailure::NoNode(error.to_string()))?;

    let receipt = SendReceipt::parse(&answer).map_err(|_| node_refused(&answer))?;

    record_sent(context, &to_hex(&recipient_prefix), text, Some(&receipt))
        .await
        .map_err(|error| SendFailure::Storage(error.to_string()))?;

    Ok(SendResult {
        flooded: receipt.flooded,
        expected_ack: receipt.expected_ack.map(|ack| format!("{ack:08x}")),
        estimated_timeout_ms: receipt.estimated_timeout_ms,
    })
}

/// Sends one message to a channel and records the attempt.
///
/// There is no receipt: a broadcast is not acknowledged, so the node answers
/// with a plain OK and there is nothing to wait for.
pub async fn send_channel_message(
    context: &AppContext,
    channel_index: u8,
    text: &str,
) -> Result<(), SendFailure> {
    let timestamp = Utc::now().timestamp().clamp(0, i64::from(u32::MAX)) as u32;
    let frame = send::encode_channel(channel_index, timestamp, text)
        .map_err(|error| SendFailure::Rejected(error.to_string()))?;

    let answer = context
        .link
        .request(frame)
        .await
        .map_err(|error| SendFailure::NoNode(error.to_string()))?;

    match answer.first().map(|&opcode| Response::from(opcode)) {
        Some(Response::Ok) => {}
        _ => return Err(node_refused(&answer)),
    }

    record_sent(context, &format!("channel:{channel_index}"), text, None)
        .await
        .map_err(|error| SendFailure::Storage(error.to_string()))?;

    Ok(())
}

/// Turns a refusal from the node into a failure worth reporting.
///
/// The error code is passed through as a number: what each one means is the
/// firmware's business, and inventing names for them here would be guessing.
fn node_refused(answer: &[u8]) -> SendFailure {
    match answer.first().map(|&opcode| Response::from(opcode)) {
        Some(Response::Err) => SendFailure::NodeRefused {
            code: answer.get(1).copied(),
        },
        _ => SendFailure::NodeRefused { code: None },
    }
}

/// Keeps a record of what was handed to the node.
async fn record_sent(
    context: &AppContext,
    target: &str,
    text: &str,
    receipt: Option<&SendReceipt>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO messages_sent (target, text, sent_at, flooded, expected_ack)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(target)
    .bind(text)
    .bind(Utc::now().to_rfc3339())
    .bind(receipt.map(|receipt| i64::from(receipt.flooded)))
    .bind(
        receipt
            .and_then(|receipt| receipt.expected_ack)
            .map(|ack| format!("{ack:08x}")),
    )
    .execute(context.db.pool())
    .await?;

    Ok(())
}

/// Why a message could not be sent.
#[derive(Debug, thiserror::Error)]
pub enum SendFailure {
    /// The request itself is not sendable — empty text, too long.
    #[error("{0}")]
    Rejected(String),

    /// The node could not be reached.
    #[error("the node did not answer: {0}")]
    NoNode(String),

    /// The node answered, but refused.
    #[error("the node refused the message")]
    NodeRefused {
        /// The firmware's error code, if it sent one.
        code: Option<u8>,
    },

    /// The message went out but could not be recorded.
    #[error("could not record the message: {0}")]
    Storage(String),
}

impl IntoResponse for SendFailure {
    fn into_response(self) -> axum::response::Response {
        let (status, code) = match self {
            Self::Rejected(_) => (StatusCode::BAD_REQUEST, "invalid_message"),
            Self::NoNode(_) => (StatusCode::SERVICE_UNAVAILABLE, "node_unreachable"),
            Self::NodeRefused { .. } => (StatusCode::BAD_GATEWAY, "node_refused"),
            Self::Storage(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage_failed"),
        };

        if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(error = %self, "could not send a message");
        } else {
            tracing::warn!(error = %self, "could not send a message");
        }

        (
            status,
            Json(serde_json::json!({
                "error": { "code": code, "message": self.to_string() }
            })),
        )
            .into_response()
    }
}

/// Takes a message for one contact.
async fn send_direct(
    State(context): State<AppContext>,
    Json(request): Json<DirectRequest>,
) -> Result<Json<SendResult>, SendFailure> {
    let prefix = parse_prefix(&request.recipient_prefix).ok_or_else(|| {
        SendFailure::Rejected("recipient_prefix must be twelve hex digits".into())
    })?;

    send_message(&context, prefix, &request.text)
        .await
        .map(Json)
}

/// Takes a message for one channel.
async fn send_to_channel(
    State(context): State<AppContext>,
    Json(request): Json<ChannelRequest>,
) -> Result<StatusCode, SendFailure> {
    send_channel_message(&context, request.channel_index, &request.text).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Reads a six-byte prefix from hex.
fn parse_prefix(text: &str) -> Option<[u8; 6]> {
    if text.len() != 12 {
        return None;
    }

    let mut prefix = [0u8; 6];
    for (index, byte) in prefix.iter_mut().enumerate() {
        *byte = u8::from_str_radix(text.get(index * 2..index * 2 + 2)?, 16).ok()?;
    }

    Some(prefix)
}

/// Answers with the stored channel messages, newest first.
async fn list_channel_messages(
    State(context): State<AppContext>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<StoredChannelMessage>>, ListError> {
    read_channel_messages(&context, query.effective_limit())
        .await
        .map(Json)
        .map_err(ListError)
}

/// Answers with the channels the node knows.
async fn list_channels(
    State(context): State<AppContext>,
) -> Result<Json<Vec<KnownChannel>>, ListError> {
    read_channels(&context).await.map(Json).map_err(ListError)
}

/// One row of `messages_channel_received`.
type ChannelMessageRow = (i64, i64, String, i64, Option<f64>, Option<i64>, i64, String);

/// Reads stored channel messages, newest first.
pub async fn read_channel_messages(
    context: &AppContext,
    limit: i64,
) -> Result<Vec<StoredChannelMessage>, sqlx::Error> {
    let rows: Vec<ChannelMessageRow> = sqlx::query_as(
        "SELECT id, channel_index, text, text_type, snr, path_len, sent_at, received_at
         FROM messages_channel_received ORDER BY id DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(context.db.pool())
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            Some(StoredChannelMessage {
                id: row.0,
                channel_index: row.1 as u8,
                text: row.2,
                text_type: row.3 as u8,
                snr: row.4.map(|value| value as f32),
                path_len: row.5.map(|value| value as u8),
                sent_at: row.6 as u32,
                received_at: parse_time(&row.7)?,
            })
        })
        .collect())
}

/// Reads the known channels, by index.
pub async fn read_channels(context: &AppContext) -> Result<Vec<KnownChannel>, sqlx::Error> {
    let rows: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT channel_index, name, seen_at FROM messages_channels ORDER BY channel_index",
    )
    .fetch_all(context.db.pool())
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            Some(KnownChannel {
                channel_index: row.0 as u8,
                name: row.1,
                seen_at: parse_time(&row.2)?,
            })
        })
        .collect())
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
pub async fn read_messages(
    context: &AppContext,
    limit: i64,
) -> Result<Vec<StoredMessage>, sqlx::Error> {
    let rows: Vec<MessageRow> = sqlx::query_as(
        "SELECT id, sender_prefix, text, text_type, snr, path_len, sent_at, received_at
         FROM messages_received ORDER BY id DESC LIMIT ?",
    )
    .bind(limit)
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
                received_at: parse_time(&row.7)?,
            })
        })
        .collect())
}

/// Reads a stored timestamp back.
///
/// A row we cannot parse is left out rather than failing the whole listing.
fn parse_time(text: &str) -> Option<DateTime<Utc>> {
    match DateTime::parse_from_rfc3339(text) {
        Ok(time) => Some(time.with_timezone(&Utc)),
        Err(error) => {
            tracing::error!(%error, text, "stored timestamp is not RFC 3339");
            None
        }
    }
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
