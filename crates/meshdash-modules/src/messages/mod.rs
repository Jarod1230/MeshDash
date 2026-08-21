//! Direct messages the node received.
//!
//! The node only announces that something is waiting; the messages are then
//! fetched one at a time until it says there are no more. This module does that
//! and keeps what arrives, so a history exists even though the node's own queue
//! is emptied by reading it.
//!
//! # The sender is a prefix, and a prefix can belong to more than one node
//!
//! A message names its sender with six bytes of their public key. This module
//! keeps its own small list of which prefix goes with which name, fed by the
//! `nodes` module over the bus — it cannot read that module's tables, and it
//! does not need to.
//!
//! Six bytes can collide. When two known contacts share a prefix, **neither
//! name is shown**: the answer would be a coin toss dressed up as fact, and on
//! a mesh where messages carry instructions, attributing one to the wrong
//! person is worse than showing a hex prefix. The API says how many candidates
//! there were, so an interface can explain itself.
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
    command,
    message::{Message, TextType},
    opcode::Response,
    push::PushEvent,
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
    Migration {
        version: 3,
        description: "which sender prefix belongs to which contact",
        sql: "
        -- Fed by the nodes module over the event bus; this module never reads
        -- that module's tables. The prefix is the first six bytes of a public
        -- key and is *not* unique — several contacts can share one, which is
        -- why the primary key here is the full key.
        CREATE TABLE messages_senders (
            public_key  TEXT PRIMARY KEY,
            prefix      TEXT NOT NULL,
            name        TEXT NOT NULL,
            updated_at  TEXT NOT NULL
        );

        CREATE INDEX messages_senders_prefix ON messages_senders (prefix);
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
    /// The sender's name, when exactly one known contact has this prefix.
    ///
    /// `None` means either nobody known, or several — `sender_candidates`
    /// tells which.
    pub sender_name: Option<String>,
    /// How many known contacts share this prefix.
    pub sender_candidates: usize,
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
                .route("/channel-send", post(send_to_channel))
                .route("/conversations", get(list_conversations))
                .route("/conversation", get(show_conversation)),
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
                        if matches!(PushEvent::parse(&payload), Ok(PushEvent::MessageWaiting)) {
                            if let Err(error) = drain_messages(&context).await {
                                tracing::warn!(error, "could not fetch waiting messages");
                            }
                        }
                    }
                    // Messages may have piled up while we were away.
                    // The nodes module announces its contacts; that is the
                    // only way this module learns a name for a prefix.
                    Ok(AppEvent::Module { module, kind, data })
                        if module == "nodes" && kind == "contact" =>
                    {
                        if let Err(error) = remember_sender(&context, &data).await {
                            tracing::error!(%error, "could not remember a contact name");
                        }
                    }
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
            .request(command::sync_next_message())
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

/// One side of a conversation: a contact, or a channel.
#[derive(Debug, Serialize, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Partner {
    /// A contact, addressed by their six-byte key prefix.
    Contact,
    /// A channel, addressed by its index.
    Channel,
}

/// Which way a message went.
#[derive(Debug, Serialize, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// It arrived over the air.
    Received,
    /// It was handed to the node to send.
    Sent,
}

/// A conversation, as the overview lists it.
#[derive(Debug, Serialize, PartialEq)]
pub struct Conversation {
    /// Contact or channel.
    pub partner: Partner,
    /// The key prefix, or the channel index as a string.
    pub id: String,
    /// The name, where one is known.
    pub name: Option<String>,
    /// For a contact: how many known contacts share this prefix.
    pub candidates: usize,
    /// The most recent message, whichever direction it went.
    pub last_text: String,
    /// When that was.
    pub last_at: DateTime<Utc>,
    /// Which way the most recent message went.
    pub last_direction: Direction,
    /// How many messages this conversation holds.
    pub messages: i64,
}

/// One message inside a conversation.
#[derive(Debug, Serialize, PartialEq)]
pub struct ConversationMessage {
    /// Whether it came in or went out.
    pub direction: Direction,
    /// The text.
    pub text: String,
    /// When MeshDash recorded it.
    pub at: DateTime<Utc>,
    /// Reception quality, for messages that arrived.
    pub snr: Option<f32>,
    /// Stations the packet passed, for messages that arrived.
    pub stations: Option<u8>,
    /// Whether the node flooded it, for messages that went out.
    pub flooded: Option<bool>,
}

/// Notes which name belongs to a public key, for resolving sender prefixes.
///
/// A payload that does not carry both fields is skipped: its shape belongs to
/// the publishing module and may change.
pub async fn remember_sender(
    context: &AppContext,
    data: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    let (Some(public_key), Some(name)) = (
        data.get("public_key").and_then(serde_json::Value::as_str),
        data.get("name").and_then(serde_json::Value::as_str),
    ) else {
        return Ok(());
    };

    // Twelve hex digits are the six bytes a message carries as its sender.
    let Some(prefix) = public_key.get(..12) else {
        return Ok(());
    };

    sqlx::query(
        "INSERT INTO messages_senders (public_key, prefix, name, updated_at)
         VALUES (?, ?, ?, ?)
         ON CONFLICT (public_key) DO UPDATE SET
            name = excluded.name,
            updated_at = excluded.updated_at",
    )
    .bind(public_key)
    .bind(prefix)
    .bind(name)
    .bind(Utc::now().to_rfc3339())
    .execute(context.db.pool())
    .await?;

    Ok(())
}

/// One row of a conversation thread: text, time, quality, hops, flood flag,
/// direction.
type ThreadRow = (
    String,
    String,
    Option<f64>,
    Option<i64>,
    Option<i64>,
    String,
);

/// One row of the conversation overview: partner kind, id, text, time, count,
/// direction.
type OverviewRow = (String, String, String, String, i64, String);

/// Lists every conversation, most recently active first.
///
/// A conversation exists as soon as one message went either way, so a contact
/// that was only written to appears alongside one that only wrote.
pub async fn read_conversations(
    context: &AppContext,
    limit: i64,
) -> Result<Vec<Conversation>, sqlx::Error> {
    // One row per partner, built from three tables at once: received direct,
    // received channel, and sent. Doing it in SQL keeps the "most recent
    // first" ordering honest across all three.
    let rows: Vec<OverviewRow> = sqlx::query_as(
        "WITH alle AS (
            SELECT 'contact' AS partner, sender_prefix AS id, text,
                   received_at AS at, 'received' AS direction
              FROM messages_received
            UNION ALL
            SELECT 'channel', CAST(channel_index AS TEXT), text,
                   received_at, 'received'
              FROM messages_channel_received
            UNION ALL
            SELECT CASE WHEN target LIKE 'channel:%' THEN 'channel' ELSE 'contact' END,
                   CASE WHEN target LIKE 'channel:%'
                        THEN substr(target, 9) ELSE target END,
                   text, sent_at, 'sent'
              FROM messages_sent
         ),
         letzte AS (
            SELECT partner, id, text, at, direction,
                   ROW_NUMBER() OVER (PARTITION BY partner, id ORDER BY at DESC) AS rang,
                   COUNT(*) OVER (PARTITION BY partner, id) AS anzahl
              FROM alle
         )
         SELECT partner, id, text, at, anzahl, direction
           FROM letzte WHERE rang = 1
          ORDER BY at DESC
          LIMIT ?",
    )
    .bind(limit)
    .fetch_all(context.db.pool())
    .await?;

    let senders = sender_names(context).await?;
    let channels = channel_names(context).await?;

    Ok(rows
        .into_iter()
        .filter_map(|(partner, id, text, at, anzahl, direction)| {
            let partner = if partner == "channel" {
                Partner::Channel
            } else {
                Partner::Contact
            };

            let (name, candidates) = match partner {
                Partner::Channel => (channels.get(&id).cloned(), 0),
                Partner::Contact => {
                    let matches = senders.get(&id);
                    (
                        matches
                            .filter(|names| names.len() == 1)
                            .map(|names| names[0].clone()),
                        matches.map_or(0, Vec::len),
                    )
                }
            };

            Some(Conversation {
                partner,
                id,
                name,
                candidates,
                last_text: text,
                last_at: parse_time(&at)?,
                last_direction: if direction == "sent" {
                    Direction::Sent
                } else {
                    Direction::Received
                },
                messages: anzahl,
            })
        })
        .collect())
}

/// Reads one conversation, oldest message first.
///
/// Sent and received are interleaved by time, which is the whole point: two
/// separate lists cannot show that an answer followed a question.
pub async fn read_conversation(
    context: &AppContext,
    partner: Partner,
    id: &str,
    limit: i64,
) -> Result<Vec<ConversationMessage>, sqlx::Error> {
    let sent_target = match partner {
        Partner::Channel => format!("channel:{id}"),
        Partner::Contact => id.to_owned(),
    };

    // The received half differs per partner; the sent half does not.
    let received = match partner {
        Partner::Contact => {
            "SELECT text, received_at AS at, snr, path_len, NULL AS flooded, 'received' AS direction
               FROM messages_received WHERE sender_prefix = ?1"
        }
        Partner::Channel => {
            "SELECT text, received_at AS at, snr, path_len, NULL AS flooded, 'received' AS direction
               FROM messages_channel_received WHERE channel_index = CAST(?1 AS INTEGER)"
        }
    };

    let query = format!(
        "SELECT * FROM (
            {received}
            UNION ALL
            SELECT text, sent_at AS at, NULL, NULL, flooded, 'sent'
              FROM messages_sent WHERE target = ?2
         ) ORDER BY at DESC LIMIT ?3"
    );

    let rows: Vec<ThreadRow> = sqlx::query_as(&query)
        .bind(id)
        .bind(&sent_target)
        .bind(limit)
        .fetch_all(context.db.pool())
        .await?;

    // Read newest-first so the limit keeps the *recent* end, then turn it
    // around: a conversation reads forwards.
    let mut messages: Vec<ConversationMessage> = rows
        .into_iter()
        .filter_map(|(text, at, snr, stations, flooded, direction)| {
            Some(ConversationMessage {
                direction: if direction == "sent" {
                    Direction::Sent
                } else {
                    Direction::Received
                },
                text,
                at: parse_time(&at)?,
                snr: snr.map(|value| value as f32),
                stations: stations.map(|value| value as u8),
                flooded: flooded.map(|value| value != 0),
            })
        })
        .collect();
    messages.reverse();

    Ok(messages)
}

/// Every known channel index and its name.
async fn channel_names(
    context: &AppContext,
) -> Result<std::collections::HashMap<String, String>, sqlx::Error> {
    let rows: Vec<(i64, String)> =
        sqlx::query_as("SELECT channel_index, name FROM messages_channels WHERE name <> ''")
            .fetch_all(context.db.pool())
            .await?;

    Ok(rows
        .into_iter()
        .map(|(index, name)| (index.to_string(), name))
        .collect())
}

/// Every known prefix and the names behind it.
///
/// Read in one go and matched in memory: a listing of five hundred messages
/// would otherwise mean five hundred queries.
async fn sender_names(
    context: &AppContext,
) -> Result<std::collections::HashMap<String, Vec<String>>, sqlx::Error> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT prefix, name FROM messages_senders ORDER BY name")
            .fetch_all(context.db.pool())
            .await?;

    let mut by_prefix: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for (prefix, name) in rows {
        by_prefix.entry(prefix).or_default().push(name);
    }

    Ok(by_prefix)
}

/// Who a sender prefix might be.
#[derive(Debug, Serialize, PartialEq)]
pub struct SenderIdentity {
    /// The name, when exactly one contact matches.
    pub name: Option<String>,
    /// How many known contacts share this prefix.
    ///
    /// Zero means nobody known; more than one means the prefix is ambiguous
    /// and `name` stays empty on purpose.
    pub candidates: usize,
}

/// Looks up who a prefix belongs to.
///
/// With several candidates no name is returned. Picking one would be a coin
/// toss presented as fact, and on a mesh where messages carry instructions,
/// attributing one to the wrong person is worse than showing a hex prefix.
pub async fn identify_sender(
    context: &AppContext,
    prefix: &str,
) -> Result<SenderIdentity, sqlx::Error> {
    let names: Vec<(String,)> =
        sqlx::query_as("SELECT name FROM messages_senders WHERE prefix = ? ORDER BY name")
            .bind(prefix)
            .fetch_all(context.db.pool())
            .await?;

    Ok(SenderIdentity {
        name: (names.len() == 1).then(|| names[0].0.clone()),
        candidates: names.len(),
    })
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
            .request(command::get_channel(index))
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

/// Which conversation to show.
#[derive(Debug, Deserialize)]
pub struct ConversationQuery {
    /// A contact's six-byte key prefix, lowercase hex.
    with: Option<String>,
    /// A channel index.
    channel: Option<u8>,
    /// Upper bound, capped like every other listing.
    limit: Option<i64>,
}

/// Answers with every conversation, most recently active first.
async fn list_conversations(
    State(context): State<AppContext>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<Conversation>>, ListError> {
    read_conversations(&context, query.effective_limit())
        .await
        .map(Json)
        .map_err(ListError)
}

/// Answers with one conversation, oldest message first.
async fn show_conversation(
    State(context): State<AppContext>,
    Query(query): Query<ConversationQuery>,
) -> Result<Json<Vec<ConversationMessage>>, ListError> {
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);

    let (partner, id) = match (&query.with, query.channel) {
        (Some(prefix), _) => (Partner::Contact, prefix.clone()),
        (None, Some(index)) => (Partner::Channel, index.to_string()),
        // Neither given: an empty conversation is a truthful answer to
        // "show me nothing in particular".
        (None, None) => return Ok(Json(Vec::new())),
    };

    read_conversation(&context, partner, &id, limit)
        .await
        .map(Json)
        .map_err(ListError)
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

    // One lookup for the whole listing rather than one per row: a busy mesh
    // makes this the difference between one query and five hundred.
    let senders = sender_names(context).await?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let matches = senders.get(&row.1);
            Some(StoredMessage {
                sender_name: matches
                    .filter(|names| names.len() == 1)
                    .map(|names| names[0].clone()),
                sender_candidates: matches.map_or(0, Vec::len),
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
