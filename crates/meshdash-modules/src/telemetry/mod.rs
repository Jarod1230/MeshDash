//! How the node's battery and storage develop over time.
//!
//! Asks the node at a fixed interval and keeps every reading, so an operator
//! can see a curve rather than a single number — "is the battery falling
//! faster than last week" is the question this answers.
//!
//! # Reception quality comes from another module, not from the node
//!
//! The SNR of a received packet is not something this module can ask for: it
//! arrives with each message, on a link the `messages` module owns. So it is
//! not fetched here — it is listened for. `messages` publishes every message it
//! stores as `AppEvent::Module { module: "messages", kind: "signal" }`, and
//! this module records what it hears.
//!
//! Neither module knows whether the other is running. Without `messages` the
//! curve simply stays empty; without this module nobody listens. That is the
//! coupling the module rules prescribe — see
//! `docs/decisions/0007-modul-ereignisse.md`.
//!
//! # What this module publishes
//!
//! A neighbour's answer can carry its position, and a position belongs on the
//! map — which `nodes` owns. So every position that arrives is published as
//! `AppEvent::Module { module: "telemetry", kind: "position" }`:
//!
//! ```json
//! { "public_key": "aa…", "latitude": 48.137154, "longitude": 11.576124, "altitude": 519.0 }
//! ```
//!
//! Whoever listens decides what to do with it. This module keeps the reading
//! either way — the announcement is a second use of it, not a hand-off.
//!
//! # Asking other nodes costs airtime, so it is off by default
//!
//! Readings from another node have to be requested over the air — nothing
//! arrives unasked (ADR-0009). Every request occupies the shared band that the
//! whole mesh uses, and in the European bands there is a duty cycle to respect.
//!
//! So this is opt-in, and deliberately slow when switched on: **one node per
//! round**, in turn, and only nodes heard recently. A node that has been silent
//! for a day is not worth transmitting at.
//!
//! ```toml
//! [modules.telemetry]
//! neighbours = true       # off unless asked for
//! every_minutes = 30      # one request per interval, taking turns
//! ```
//!
//! # An answer does not say who sent it
//!
//! `PUSH_CODE_BINARY_RESPONSE` carries only the tag of the request. This module
//! therefore remembers which contact a tag belongs to — in memory, because a
//! restart loses the pending requests anyway and an answer to a question
//! nobody remembers asking is not worth storing.
//!
//! # What this costs on disk
//!
//! One reading every five minutes is 288 a day, roughly 105 000 a year. At the
//! size of a row here that is a few megabytes per year — acceptable on a
//! Raspberry Pi, but it grows without bound. Thinning old readings is an open
//! point in `docs/roadmap.md`, and this module does not do it.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use axum::{Json, Router, extract::Query, extract::State, routing::get};
use chrono::{DateTime, Utc};
use meshdash_core::{
    db::Migration,
    event::AppEvent,
    module::{AppContext, Module},
};
use meshdash_proto::{
    battery::{self, BatteryAndStorage},
    binary_request::{self, Telemetry},
    lpp::{self, Value},
    push::PushEvent,
    send::SendReceipt,
};
use serde::{Deserialize, Serialize};

use crate::query::{BadTimeRange, TimeRange, Window};

/// Schema of this module. Versions count from 1, per module.
const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        description: "battery and storage readings over time",
        sql: "
        CREATE TABLE telemetry_battery_samples (
            id                INTEGER PRIMARY KEY AUTOINCREMENT,
            at                TEXT    NOT NULL,
            millivolts        INTEGER NOT NULL,
            storage_used_kib  INTEGER NOT NULL,
            storage_total_kib INTEGER NOT NULL
        );

        CREATE INDEX telemetry_battery_samples_at ON telemetry_battery_samples (at);
    ",
    },
    Migration {
        version: 2,
        description: "reception quality of received packets over time",
        sql: "
        CREATE TABLE telemetry_signal_samples (
            id        INTEGER PRIMARY KEY AUTOINCREMENT,
            at        TEXT    NOT NULL,
            source    TEXT    NOT NULL,
            snr       REAL    NOT NULL,
            path_len  INTEGER
        );

        CREATE INDEX telemetry_signal_samples_at ON telemetry_signal_samples (at);
    ",
    },
    Migration {
        version: 3,
        description: "readings other nodes reported about themselves",
        sql: "
        CREATE TABLE telemetry_neighbour_samples (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            public_key  TEXT    NOT NULL,
            at          TEXT    NOT NULL,
            channel     INTEGER NOT NULL,
            type_code   INTEGER NOT NULL,
            -- One of these three shapes is filled, depending on the type.
            value       REAL,
            axis_x      REAL,
            axis_y      REAL,
            axis_z      REAL,
            latitude    REAL,
            longitude   REAL,
            altitude    REAL
        );

        CREATE INDEX telemetry_neighbour_samples_at
            ON telemetry_neighbour_samples (at);
        CREATE INDEX telemetry_neighbour_samples_key
            ON telemetry_neighbour_samples (public_key);

        -- Whom to ask, and when they were last worth asking.
        --
        -- Kept here rather than read from the nodes module: a module does not
        -- read another's tables (docs/module-system.md), and 'when did we last
        -- ask this node' belongs to nobody else anyway. The keys come from
        -- advert pushes, which carry the full 32-byte key a request needs.
        CREATE TABLE telemetry_neighbours (
            public_key     TEXT PRIMARY KEY,
            last_heard_at  TEXT NOT NULL,
            last_asked_at  TEXT
        );
    ",
    },
];

/// How this module may be configured, under `[modules.telemetry]`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// Whether to ask other nodes for their readings at all.
    ///
    /// Off by default: this transmits into a shared band on the operator's
    /// behalf, which is their decision to make, not a default to inherit.
    pub neighbours: bool,
    /// Minutes between two requests. One node is asked per interval.
    pub every_minutes: u64,
    /// Do not ask a node that has not been heard in this many hours.
    pub silent_after_hours: i64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            neighbours: false,
            // Half an hour per node is slow on purpose. With ten reachable
            // neighbours that is a full round in five hours — plenty for a
            // battery curve, and gentle on a band everyone shares.
            every_minutes: 30,
            silent_after_hours: 24,
        }
    }
}

/// How often to look again while the asking is switched off.
///
/// Short, because the only thing being waited for is somebody ticking a box.
const IDLE_CHECK: Duration = Duration::from_secs(20);

/// How often the node is asked.
///
/// An operating choice, not a protocol value. Five minutes is fine enough to
/// show a discharge curve and coarse enough that a year of readings stays a few
/// megabytes. Asking every few seconds would fill the disk to draw the same
/// line.
const SAMPLE_INTERVAL: Duration = Duration::from_secs(300);

/// How many readings a request returns unless it asks for fewer.
///
/// The table grows without bound, so an unbounded query would eventually try to
/// serialise a year of readings into one response.
const DEFAULT_LIMIT: i64 = 500;

/// Largest number of readings a single request may ask for.
const MAX_LIMIT: i64 = 5_000;

/// How long a tag stays valid before the answer is given up on.
///
/// The node's own estimate travels in the receipt, but a mesh can be slow and
/// an answer that arrives after this is no longer attributable to anyone.
const TAG_LIFETIME: Duration = Duration::from_secs(300);

/// Records battery and storage over time.
#[derive(Debug, Default)]
pub struct TelemetryModule;

/// Which contact a pending request belongs to.
///
/// In memory only: a restart loses the pending requests, and an answer to a
/// question nobody remembers asking cannot be attributed to a node.
#[derive(Debug, Default, Clone)]
struct PendingRequests(Arc<std::sync::Mutex<Vec<(u32, String, std::time::Instant)>>>);

impl PendingRequests {
    fn remember(&self, tag: u32, public_key: String) {
        let Ok(mut pending) = self.0.lock() else {
            return;
        };
        pending.retain(|(_, _, since)| since.elapsed() < TAG_LIFETIME);
        pending.push((tag, public_key, std::time::Instant::now()));
    }

    /// Takes the contact a tag belongs to, if the tag is still known.
    fn take(&self, tag: u32) -> Option<String> {
        let mut pending = self.0.lock().ok()?;
        let index = pending
            .iter()
            .position(|(candidate, ..)| *candidate == tag)?;
        Some(pending.remove(index).1)
    }
}

/// One stored reception-quality reading.
#[derive(Debug, Serialize, PartialEq)]
pub struct SignalSample {
    /// Running number; doubles as the cursor for paging.
    pub id: i64,
    /// When the packet arrived.
    pub at: DateTime<Utc>,
    /// Where it came from: a direct message or a channel.
    pub source: String,
    /// Signal-to-noise ratio in dB. Negative is ordinary for LoRa.
    pub snr: f32,
    /// Hops the packet flooded over, or `None` if it did not flood.
    pub path_len: Option<u8>,
}

/// One stored reading.
#[derive(Debug, Serialize, PartialEq)]
pub struct BatterySample {
    /// Running number; doubles as the cursor for paging.
    pub id: i64,
    /// When it was taken.
    pub at: DateTime<Utc>,
    /// Battery voltage in millivolts, as the node measured it.
    pub millivolts: u16,
    /// Storage in use, in kibibytes.
    pub storage_used_kib: u32,
    /// Storage available in total, in kibibytes.
    pub storage_total_kib: u32,
}

/// How many readings to return.
#[derive(Debug, Deserialize, Default)]
pub struct ListQuery {
    /// Upper bound, capped at [`MAX_LIMIT`].
    limit: Option<i64>,
    /// Only readings older than this one — the id of the last one seen.
    before: Option<i64>,
    /// Which stretch of time to cover.
    #[serde(flatten)]
    range: TimeRange,
}

impl ListQuery {
    /// The number of rows to read, within the bounds this module allows.
    fn effective_limit(&self) -> i64 {
        self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
    }

    /// What the request asks for, or what is wrong with its time range.
    fn window(&self) -> Result<Window, BadTimeRange> {
        Ok(Window::new(
            self.effective_limit(),
            self.before,
            self.range.bounds()?,
        ))
    }
}

#[async_trait]
impl Module for TelemetryModule {
    fn name(&self) -> &'static str {
        "telemetry"
    }

    fn migrations(&self) -> &'static [Migration] {
        MIGRATIONS
    }

    fn routes(&self) -> Option<Router<AppContext>> {
        Some(
            Router::new()
                .route("/battery", get(list_samples))
                .route("/signal", get(list_signal_samples))
                .route("/neighbours", get(list_neighbour_samples)),
        )
    }

    async fn start(&self, context: &AppContext) -> Result<(), String> {
        let context = Arc::new(context.clone());
        let pending = PendingRequests::default();
        let pending_for_events = pending.clone();

        // One task listens, so a fresh connection is sampled at once instead of
        // waiting out the interval.
        let on_connect = Arc::clone(&context);
        let mut events = context.events.subscribe();
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(AppEvent::NodeConnected) => take_sample(&on_connect).await,
                    Ok(AppEvent::Push { payload }) => {
                        // Two kinds of push matter here: an advert tells us a
                        // node is worth asking, a binary response is an answer
                        // to something we asked.
                        handle_push(&on_connect, &payload, &pending_for_events).await;
                    }
                    // Reception quality is not ours to ask for; it is
                    // announced by whoever received the packet.
                    Ok(AppEvent::Module { module, kind, data })
                        if module == "messages" && kind == "signal" =>
                    {
                        record_signal(&on_connect, &data).await;
                    }
                    Ok(_) => {}
                    // Missed events used to cost nothing here: this module
                    // only reacted to a connection coming up, and the next one
                    // would come. Since it records reception quality, a missed
                    // event is a missing measurement — a gap in a curve that
                    // otherwise looks complete.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(missed)) => {
                        tracing::warn!(missed, "telemetry module missed events");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        // Asking other nodes is opt-in; see the note at the top of this file.
        //
        // The settings are read on every round rather than captured here, so
        // switching the asking on or off, or changing how often, takes effect
        // without a restart. An operator who ticks a box expects the box to
        // mean something now.
        let asking = Arc::clone(&context);
        let pending_for_task = pending.clone();
        tokio::spawn(async move {
            loop {
                let settings: Settings = match asking.settings.get("telemetry") {
                    Ok(settings) => settings,
                    Err(error) => {
                        tracing::error!(%error, "cannot read the telemetry settings");
                        Settings::default()
                    }
                };

                if !settings.neighbours {
                    // Nothing to do, and nothing to wait a long interval for:
                    // the box may be ticked at any moment.
                    tokio::time::sleep(IDLE_CHECK).await;
                    continue;
                }

                tokio::time::sleep(Duration::from_secs(settings.every_minutes.max(1) * 60)).await;

                // Checked again on the far side of the wait: half an hour is
                // long enough for somebody to have changed their mind.
                if asking
                    .settings
                    .get::<Settings>("telemetry")
                    .is_ok_and(|now| now.neighbours)
                {
                    ask_one_neighbour(&asking, &settings, &pending_for_task).await;
                }
            }
        });

        // The other keeps the curve going.
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(SAMPLE_INTERVAL);
            // The first tick fires immediately; the connect handler covers that
            // moment already.
            ticker.tick().await;

            loop {
                ticker.tick().await;
                take_sample(&context).await;
            }
        });

        Ok(())
    }
}

/// One reading another node reported, as the API returns it.
#[derive(Debug, Serialize, PartialEq)]
pub struct NeighbourSample {
    /// Running number; doubles as the cursor for paging.
    pub id: i64,
    /// Whose reading, lowercase hex.
    pub public_key: String,
    /// When MeshDash received it.
    pub at: DateTime<Utc>,
    /// Which sensor of that node. 1 is the node itself.
    pub channel: u8,
    /// The LPP type code, passed through rather than named: what a code means
    /// is the sensor's business, and inventing names would be guessing.
    pub type_code: u8,
    /// A single value, where the type has one.
    pub value: Option<f64>,
    /// Three axes, where the type has them.
    pub axes: Option<[f64; 3]>,
    /// A position, where the type is one.
    pub position: Option<[f64; 3]>,
}

/// Which neighbour readings to answer with.
#[derive(Debug, Deserialize, Default)]
pub struct NeighbourQuery {
    /// Only readings from this public key, lowercase hex.
    node: Option<String>,
    /// Upper bound, capped at [`MAX_LIMIT`].
    limit: Option<i64>,
    /// Only readings older than this one.
    before: Option<i64>,
    /// Which stretch of time to cover.
    #[serde(flatten)]
    range: TimeRange,
}

impl NeighbourQuery {
    /// What the request asks for, or what is wrong with its time range.
    fn window(&self) -> Result<Window, BadTimeRange> {
        Ok(Window::new(
            self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
            self.before,
            self.range.bounds()?,
        ))
    }
}

/// Answers with what other nodes reported, newest first.
async fn list_neighbour_samples(
    State(context): State<AppContext>,
    Query(query): Query<NeighbourQuery>,
) -> Result<Json<Vec<NeighbourSample>>, ListError> {
    read_neighbour_samples(&context, query.node.as_deref(), &query.window()?)
        .await
        .map(Json)
        .map_err(ListError::from)
}

/// One row of `telemetry_neighbour_samples`.
type NeighbourRow = (
    i64,
    String,
    String,
    i64,
    i64,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
);

/// Reads what other nodes reported, newest first.
pub async fn read_neighbour_samples(
    context: &AppContext,
    node: Option<&str>,
    window: &Window,
) -> Result<Vec<NeighbourSample>, sqlx::Error> {
    let rows: Vec<NeighbourRow> = sqlx::query_as(
        "SELECT id, public_key, at, channel, type_code, value,
                axis_x, axis_y, axis_z, latitude, longitude, altitude
         FROM telemetry_neighbour_samples
         WHERE (?1 IS NULL OR public_key = ?1)
           AND (?2 IS NULL OR id < ?2)
           AND (?3 IS NULL OR at >= ?3)
           AND (?4 IS NULL OR at <= ?4)
         ORDER BY id DESC LIMIT ?5",
    )
    .bind(node)
    .bind(window.before)
    .bind(&window.since)
    .bind(&window.until)
    .bind(window.limit)
    .fetch_all(context.db.pool())
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            Some(NeighbourSample {
                id: row.0,
                public_key: row.1,
                at: match DateTime::parse_from_rfc3339(&row.2) {
                    Ok(time) => time.with_timezone(&Utc),
                    Err(error) => {
                        tracing::error!(%error, "stored timestamp is not RFC 3339");
                        return None;
                    }
                },
                channel: row.3 as u8,
                type_code: row.4 as u8,
                value: row.5,
                axes: match (row.6, row.7, row.8) {
                    (Some(x), Some(y), Some(z)) => Some([x, y, z]),
                    _ => None,
                },
                position: match (row.9, row.10, row.11) {
                    (Some(lat), Some(lon), Some(alt)) => Some([lat, lon, alt]),
                    _ => None,
                },
            })
        })
        .collect())
}

/// Reacts to a push: adverts say whom to ask, responses are the answers.
async fn handle_push(context: &AppContext, payload: &[u8], pending: &PendingRequests) {
    let response = match PushEvent::parse(payload) {
        // An advert carries the full key a request needs.
        Ok(PushEvent::Advert(advert)) => {
            if let Err(error) = note_heard(context, advert.public_key()).await {
                tracing::error!(%error, "could not note that a node was heard");
            }
            return;
        }
        Ok(PushEvent::BinaryResponse(response)) => response,
        _ => return,
    };

    // Only the tag comes back, so the sender is whoever we asked under it.
    let Some(public_key) = pending.take(response.tag) else {
        tracing::debug!(
            tag = response.tag,
            "an answer to a question we no longer remember"
        );
        return;
    };

    let decoded = lpp::decode(&response.payload);
    if let Some(stopped) = &decoded.stopped {
        // Worth saying: it means a sensor type this build does not know, and
        // that everything after it in that payload was lost.
        tracing::warn!(%stopped, "stopped reading a neighbour's telemetry early");
    }

    if let Err(error) = store_neighbour_readings(context, &public_key, &decoded.readings).await {
        tracing::error!(%error, "could not store a neighbour's telemetry");
    }
}

/// Remembers that a node was heard, so it becomes worth asking.
async fn note_heard(context: &AppContext, public_key: &[u8; 32]) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO telemetry_neighbours (public_key, last_heard_at)
         VALUES (?, ?)
         ON CONFLICT (public_key) DO UPDATE SET last_heard_at = excluded.last_heard_at",
    )
    .bind(to_hex(public_key))
    .bind(Utc::now().to_rfc3339())
    .execute(context.db.pool())
    .await?;

    Ok(())
}

/// Asks the node that has waited longest for its turn.
///
/// One per round, on purpose: every request goes out over a band the whole
/// mesh shares. Nodes silent beyond `silent_after_hours` are skipped — no
/// point transmitting at something that is not there.
async fn ask_one_neighbour(context: &AppContext, settings: &Settings, pending: &PendingRequests) {
    let cutoff = (Utc::now() - chrono::Duration::hours(settings.silent_after_hours)).to_rfc3339();

    // Never asked comes first, then whoever was asked longest ago.
    let candidate: Option<(String,)> = match sqlx::query_as(
        "SELECT public_key FROM telemetry_neighbours
         WHERE last_heard_at > ?
         ORDER BY last_asked_at IS NOT NULL, last_asked_at, public_key
         LIMIT 1",
    )
    .bind(&cutoff)
    .fetch_optional(context.db.pool())
    .await
    {
        Ok(found) => found,
        Err(error) => {
            tracing::error!(%error, "could not pick a node to ask");
            return;
        }
    };

    let Some((public_key,)) = candidate else {
        tracing::debug!("nobody has been heard recently enough to ask");
        return;
    };

    let Some(key_bytes) = from_hex(&public_key) else {
        tracing::error!(public_key, "stored key is not readable hex");
        return;
    };

    // Four bytes that differ from last time, or the second request to the
    // same node would hash identically to the first and be dropped as a
    // duplicate. Not cryptographic randomness and not required to be: the
    // firmware calls it a "blob to help make packet-hash unique", so being
    // different is the whole job. A clock in nanoseconds does that without
    // pulling in a dependency for it.
    let nonce = nonce_from_clock();
    let frame = binary_request::encode_telemetry_request(&key_bytes, Telemetry::ALL, nonce);

    match context.link.request(frame).await {
        Ok(answer) => match SendReceipt::parse(&answer) {
            // The receipt's acknowledgement field carries the tag here; the
            // frame layout is the same as for a sent message.
            Ok(receipt) => {
                if let Some(tag) = receipt.expected_ack {
                    pending.remember(tag, public_key.clone());
                }
                if let Err(error) = note_asked(context, &public_key).await {
                    tracing::error!(%error, "could not note that a node was asked");
                }
                tracing::info!(node = public_key, "asked a node for its telemetry");
            }
            Err(error) => tracing::warn!(%error, "the node refused the telemetry request"),
        },
        Err(error) => tracing::warn!(%error, "could not send a telemetry request"),
    }
}

/// Four bytes that differ between calls. See the note at the call site.
fn nonce_from_clock() -> [u8; 4] {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.subsec_nanos())
        .unwrap_or(0);

    nanos.to_le_bytes()
}

/// Records that a node has had its turn, so the next round picks another.
async fn note_asked(context: &AppContext, public_key: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE telemetry_neighbours SET last_asked_at = ? WHERE public_key = ?")
        .bind(Utc::now().to_rfc3339())
        .bind(public_key)
        .execute(context.db.pool())
        .await?;

    Ok(())
}

/// Stores what another node reported about itself.
pub async fn store_neighbour_readings(
    context: &AppContext,
    public_key: &str,
    readings: &[lpp::Reading],
) -> Result<(), sqlx::Error> {
    let now = Utc::now().to_rfc3339();

    for reading in readings {
        let (value, axes, position) = match &reading.value {
            Value::Number(value) => (Some(*value), None, None),
            Value::Axes { x, y, z } => (None, Some((*x, *y, *z)), None),
            Value::Position {
                latitude,
                longitude,
                altitude,
            } => (None, None, Some((*latitude, *longitude, *altitude))),
        };

        sqlx::query(
            "INSERT INTO telemetry_neighbour_samples
                (public_key, at, channel, type_code, value,
                 axis_x, axis_y, axis_z, latitude, longitude, altitude)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(public_key)
        .bind(&now)
        .bind(i64::from(reading.channel))
        .bind(i64::from(reading.type_code))
        .bind(value)
        .bind(axes.map(|(x, ..)| x))
        .bind(axes.map(|(_, y, _)| y))
        .bind(axes.map(|(.., z)| z))
        .bind(position.map(|(lat, ..)| lat))
        .bind(position.map(|(_, lon, _)| lon))
        .bind(position.map(|(.., alt)| alt))
        .execute(context.db.pool())
        .await?;

        if let Some((latitude, longitude, altitude)) = position {
            publish_position(context, public_key, latitude, longitude, altitude);
        }
    }

    Ok(())
}

/// Announces where a neighbour says it is.
///
/// Fire and forget, like every announcement on this bus: nobody may be
/// listening, and this module must not care. Without `nodes` the position
/// stays a reading in a table and never reaches a map.
fn publish_position(
    context: &AppContext,
    public_key: &str,
    latitude: f64,
    longitude: f64,
    altitude: f64,
) {
    context.events.publish(AppEvent::Module {
        module: "telemetry".into(),
        kind: "position".into(),
        data: serde_json::json!({
            "public_key": public_key,
            "latitude": latitude,
            "longitude": longitude,
            "altitude": altitude,
        }),
    });
}

/// Turns bytes into lowercase hex, as the API spells binary data.
fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}

/// Reads a full 32-byte key back from hex.
fn from_hex(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 {
        return None;
    }

    let mut key = [0u8; 32];
    for (index, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(text.get(index * 2..index * 2 + 2)?, 16).ok()?;
    }

    Some(key)
}

/// Asks the node once and stores what it says.
///
/// A node that does not answer costs one reading, not the task: it is offline
/// often enough that giving up would end the curve for good.
async fn take_sample(context: &AppContext) {
    match read_battery(context).await {
        Ok(reading) => {
            if let Err(error) = store_sample(context, &reading).await {
                tracing::error!(%error, "could not store a battery reading");
            }
        }
        Err(error) => tracing::debug!(error, "no battery reading this time"),
    }
}

/// Records one announced reception quality.
///
/// A payload that does not carry an SNR is skipped without complaint: older
/// protocol variants do not report one, and a missing value is not an error.
/// The shape of `data` belongs to the publishing module, so anything
/// unexpected is treated as "nothing to record" rather than as a failure.
async fn record_signal(context: &AppContext, data: &serde_json::Value) {
    let Some(snr) = data.get("snr").and_then(serde_json::Value::as_f64) else {
        return;
    };

    let source = data
        .get("source")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let path_len = data.get("path_len").and_then(serde_json::Value::as_u64);

    if let Err(error) = store_signal(context, source, snr, path_len).await {
        tracing::error!(%error, "could not store a signal reading");
    }
}

/// Stores one reception-quality reading.
pub async fn store_signal(
    context: &AppContext,
    source: &str,
    snr: f64,
    path_len: Option<u64>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO telemetry_signal_samples (at, source, snr, path_len)
         VALUES (?, ?, ?, ?)",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(source)
    .bind(snr)
    .bind(path_len.map(|value| value as i64))
    .execute(context.db.pool())
    .await?;

    Ok(())
}

/// Answers with the stored reception qualities, newest first.
async fn list_signal_samples(
    State(context): State<AppContext>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<SignalSample>>, ListError> {
    read_signal_samples(&context, &query.window()?)
        .await
        .map(Json)
        .map_err(ListError::from)
}

/// Reads stored reception qualities, newest first.
pub async fn read_signal_samples(
    context: &AppContext,
    window: &Window,
) -> Result<Vec<SignalSample>, sqlx::Error> {
    let rows: Vec<(i64, String, String, f64, Option<i64>)> = sqlx::query_as(
        "SELECT id, at, source, snr, path_len FROM telemetry_signal_samples
         WHERE (?1 IS NULL OR id < ?1)
           AND (?2 IS NULL OR at >= ?2)
           AND (?3 IS NULL OR at <= ?3)
         ORDER BY id DESC LIMIT ?4",
    )
    .bind(window.before)
    .bind(&window.since)
    .bind(&window.until)
    .bind(window.limit)
    .fetch_all(context.db.pool())
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            Some(SignalSample {
                id: row.0,
                at: match DateTime::parse_from_rfc3339(&row.1) {
                    Ok(time) => time.with_timezone(&Utc),
                    Err(error) => {
                        tracing::error!(%error, "stored timestamp is not RFC 3339");
                        return None;
                    }
                },
                source: row.2,
                snr: row.3 as f32,
                path_len: row.4.map(|value| value as u8),
            })
        })
        .collect())
}

/// Asks the node for its battery and storage.
pub async fn read_battery(context: &AppContext) -> Result<BatteryAndStorage, String> {
    let answer = context
        .link
        .request(battery::battery_query())
        .await
        .map_err(|error| error.to_string())?;

    BatteryAndStorage::parse(&answer).map_err(|error| error.to_string())
}

/// Stores one reading.
pub async fn store_sample(
    context: &AppContext,
    reading: &BatteryAndStorage,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO telemetry_battery_samples
            (at, millivolts, storage_used_kib, storage_total_kib)
         VALUES (?, ?, ?, ?)",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(i64::from(reading.battery_millivolts))
    .bind(i64::from(reading.storage_used_kib))
    .bind(i64::from(reading.storage_total_kib))
    .execute(context.db.pool())
    .await?;

    Ok(())
}

/// Reads stored readings, newest first.
pub async fn read_samples(
    context: &AppContext,
    window: &Window,
) -> Result<Vec<BatterySample>, sqlx::Error> {
    let rows: Vec<(i64, String, i64, i64, i64)> = sqlx::query_as(
        "SELECT id, at, millivolts, storage_used_kib, storage_total_kib
         FROM telemetry_battery_samples
         WHERE (?1 IS NULL OR id < ?1)
           AND (?2 IS NULL OR at >= ?2)
           AND (?3 IS NULL OR at <= ?3)
         ORDER BY id DESC LIMIT ?4",
    )
    .bind(window.before)
    .bind(&window.since)
    .bind(&window.until)
    .bind(window.limit)
    .fetch_all(context.db.pool())
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            Some(BatterySample {
                id: row.0,
                at: match DateTime::parse_from_rfc3339(&row.1) {
                    Ok(time) => time.with_timezone(&Utc),
                    Err(error) => {
                        tracing::error!(%error, "stored timestamp is not RFC 3339");
                        return None;
                    }
                },
                millivolts: row.2 as u16,
                storage_used_kib: row.3 as u32,
                storage_total_kib: row.4 as u32,
            })
        })
        .collect())
}

/// Answers with the stored readings.
async fn list_samples(
    State(context): State<AppContext>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<BatterySample>>, ListError> {
    // Clamped rather than rejected: a caller asking for too much gets the most
    // it may have, not an error it has to handle.
    read_samples(&context, &query.window()?)
        .await
        .map(Json)
        .map_err(ListError::from)
}

/// What can go wrong answering a listing.
#[derive(Debug)]
pub enum ListError {
    /// The database could not be read.
    Storage(sqlx::Error),
    /// The request asked for a time range that is not a time.
    BadRange(BadTimeRange),
}

impl From<sqlx::Error> for ListError {
    fn from(error: sqlx::Error) -> Self {
        Self::Storage(error)
    }
}

impl From<BadTimeRange> for ListError {
    fn from(error: BadTimeRange) -> Self {
        Self::BadRange(error)
    }
}

impl axum::response::IntoResponse for ListError {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::Storage(error) => {
                tracing::error!(%error, "could not read the readings");

                (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": { "code": "storage_failed", "message": "could not read the readings" }
                    })),
                )
                    .into_response()
            }
            Self::BadRange(bad) => bad.into_response(),
        }
    }
}

#[cfg(test)]
mod tests;
