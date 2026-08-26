//! Is the node reachable, and which node is it?
//!
//! The smallest question an operator asks, and the first one MeshDash answers
//! end to end: the link reports on the event bus, this module writes it down
//! and offers it under `/api/v1/system/`.
//!
//! # It keeps a history, not just a state
//!
//! Every connect and disconnect is recorded. "Currently reachable" is worth
//! little on its own — that a node dropped out eleven times last night is the
//! finding a repeater operator is after, and that is only visible if the
//! moments were kept.

use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    Json, Router,
    extract::{Query, State},
    routing::get,
};
use chrono::{DateTime, Utc};
use meshdash_core::{
    db::Migration,
    event::AppEvent,
    module::{AppContext, Module},
};
use meshdash_proto::device::{self, DeviceInfo, SelfInfo};
use serde::{Deserialize, Serialize};

use crate::query::{BadTimeRange, TimeRange, Window};

/// Schema of this module. Versions count from 1, per module.
const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        description: "connection history and node identity",
        sql: "
        CREATE TABLE system_connection_events (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            at         TEXT    NOT NULL,
            connected  INTEGER NOT NULL,
            reason     TEXT
        );

        CREATE INDEX system_connection_events_at ON system_connection_events (at);

        CREATE TABLE system_node_info (
            -- Exactly one row: there is one node per instance, see
            -- architecture.md. The check keeps a second one from appearing.
            id                    INTEGER PRIMARY KEY CHECK (id = 1),
            seen_at               TEXT    NOT NULL,
            firmware_version_code INTEGER NOT NULL,
            firmware_version      TEXT    NOT NULL,
            manufacturer          TEXT    NOT NULL,
            build_date            TEXT    NOT NULL,
            contact_capacity      INTEGER NOT NULL,
            group_channels        INTEGER NOT NULL,
            repeater_enabled      INTEGER
        );
    ",
    },
    Migration {
        version: 2,
        description: "what the node says about itself at the start of a session",
        sql: "
        -- Answered only by CMD_APP_START, which the link sends once per
        -- connection. Kept apart from system_node_info because that describes
        -- the hardware and firmware, while this describes the node's identity
        -- in the mesh — its key, its name, where it says it is.
        CREATE TABLE system_self (
            id                 INTEGER PRIMARY KEY CHECK (id = 1),
            seen_at            TEXT    NOT NULL,
            public_key         TEXT    NOT NULL,
            name               TEXT    NOT NULL,
            latitude           REAL,
            longitude          REAL,
            transmit_power_dbm INTEGER NOT NULL,
            max_power_dbm      INTEGER NOT NULL,
            frequency_khz      INTEGER NOT NULL,
            bandwidth_hz       INTEGER NOT NULL,
            spreading_factor   INTEGER NOT NULL,
            coding_rate        INTEGER NOT NULL
        );
    ",
    },
];

/// What the node says about itself in the mesh.
#[derive(Debug, Serialize, PartialEq)]
pub struct SelfDescription {
    /// When the node last said it.
    pub seen_at: DateTime<Utc>,
    /// The node's own public key, lowercase hex.
    pub public_key: String,
    /// The name it advertises.
    pub name: String,
    /// Latitude in degrees, or `None` when the node has none set.
    pub latitude: Option<f64>,
    /// Longitude in degrees.
    pub longitude: Option<f64>,
    /// Transmit power in dBm.
    pub transmit_power_dbm: u8,
    /// The highest the board allows.
    pub max_power_dbm: u8,
    /// Frequency in kilohertz — 869618 means 869.618 MHz.
    ///
    /// Kilohertz here and hertz in `bandwidth_hz`: two neighbouring fields in
    /// two units, exactly as the firmware sends them. Converting one silently
    /// would make the pair look consistent and be wrong.
    pub frequency_khz: u32,
    /// Bandwidth in hertz — 62500 means 62.5 kHz.
    pub bandwidth_hz: u32,
    /// LoRa spreading factor.
    pub spreading_factor: u8,
    /// LoRa coding rate.
    pub coding_rate: u8,
}

/// Stores what the node said about itself.
async fn store_self(context: &AppContext, info: &SelfInfo) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO system_self
            (id, seen_at, public_key, name, latitude, longitude, transmit_power_dbm,
             max_power_dbm, frequency_khz, bandwidth_hz, spreading_factor, coding_rate)
         VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (id) DO UPDATE SET
            seen_at = excluded.seen_at,
            public_key = excluded.public_key,
            name = excluded.name,
            latitude = excluded.latitude,
            longitude = excluded.longitude,
            transmit_power_dbm = excluded.transmit_power_dbm,
            max_power_dbm = excluded.max_power_dbm,
            frequency_khz = excluded.frequency_khz,
            bandwidth_hz = excluded.bandwidth_hz,
            spreading_factor = excluded.spreading_factor,
            coding_rate = excluded.coding_rate",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(to_hex(&info.public_key))
    .bind(&info.name)
    // Micro-degrees on the wire, degrees here — the same conversion the
    // contacts get, so a position means the same thing wherever it is read.
    .bind(info.latitude.map(|value| f64::from(value) / 1e6))
    .bind(info.longitude.map(|value| f64::from(value) / 1e6))
    .bind(i64::from(info.transmit_power_dbm))
    .bind(i64::from(info.max_transmit_power_dbm))
    .bind(i64::from(info.frequency_khz))
    .bind(i64::from(info.bandwidth_hz))
    .bind(i64::from(info.spreading_factor))
    .bind(i64::from(info.coding_rate))
    .execute(context.db.pool())
    .await?;

    Ok(())
}

/// Reads back what the node last said about itself.
pub async fn read_self(context: &AppContext) -> Result<Option<SelfDescription>, sqlx::Error> {
    type Row = (
        String,
        String,
        String,
        Option<f64>,
        Option<f64>,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
    );

    let row: Option<Row> = sqlx::query_as(
        "SELECT seen_at, public_key, name, latitude, longitude, transmit_power_dbm,
                max_power_dbm, frequency_khz, bandwidth_hz, spreading_factor, coding_rate
         FROM system_self WHERE id = 1",
    )
    .fetch_optional(context.db.pool())
    .await?;

    Ok(row.and_then(|row| {
        Some(SelfDescription {
            seen_at: parse_time(&row.0)?,
            public_key: row.1,
            name: row.2,
            latitude: row.3,
            longitude: row.4,
            transmit_power_dbm: row.5 as u8,
            max_power_dbm: row.6 as u8,
            frequency_khz: row.7 as u32,
            bandwidth_hz: row.8 as u32,
            spreading_factor: row.9 as u8,
            coding_rate: row.10 as u8,
        })
    }))
}

/// Turns bytes into lowercase hex, as the API spells binary data.
fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}

/// Reports whether the node is reachable, and what it says about itself.
#[derive(Debug, Default)]
pub struct SystemModule;

/// What `/api/v1/system/status` answers.
#[derive(Debug, Serialize, PartialEq)]
pub struct SystemStatus {
    /// Whether the link currently holds a connection.
    pub connected: bool,
    /// When that last changed. `None` before anything was recorded.
    pub since: Option<DateTime<Utc>>,
    /// Why the connection ended, if it did.
    pub reason: Option<String>,
    /// What the node last reported about itself.
    pub node: Option<NodeIdentity>,
    /// Who the node is in the mesh: key, name, position, radio settings.
    ///
    /// `None` until a session start was answered — an older firmware or a
    /// node that stayed silent leaves this empty, and the rest still works.
    pub node_self: Option<SelfDescription>,
}

/// The node's own description, as stored.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct NodeIdentity {
    /// When this was last read from the node.
    pub seen_at: DateTime<Utc>,
    /// Firmware's numeric version.
    pub firmware_version_code: u8,
    /// Firmware version string.
    pub firmware_version: String,
    /// Hardware manufacturer.
    pub manufacturer: String,
    /// When the firmware was built.
    pub build_date: String,
    /// How many contacts fit.
    pub contact_capacity: u16,
    /// How many group channels exist.
    pub group_channels: u8,
    /// Whether the node also repeats, if the firmware says.
    pub repeater_enabled: Option<bool>,
}

#[async_trait]
impl Module for SystemModule {
    fn name(&self) -> &'static str {
        "system"
    }

    fn migrations(&self) -> &'static [Migration] {
        MIGRATIONS
    }

    fn routes(&self) -> Option<Router<AppContext>> {
        Some(
            Router::new()
                .route("/status", get(status))
                .route("/connections", get(connections)),
        )
    }

    async fn start(&self, context: &AppContext) -> Result<(), String> {
        let context = Arc::new(context.clone());
        let mut events = context.events.subscribe();

        // Long-running work belongs in its own task: `start` means "up and
        // running", not "finished".
        tokio::spawn(async move {
            loop {
                let event = match events.recv().await {
                    Ok(event) => event,
                    // Falling behind costs history, but carrying on is better
                    // than giving up on the rest.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(missed)) => {
                        tracing::warn!(missed, "system module missed events");
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                };

                handle(&context, event).await;
            }
        });

        Ok(())
    }
}

/// Reacts to one event from the bus.
async fn handle(context: &AppContext, event: AppEvent) {
    match event {
        AppEvent::NodeConnected => {
            if let Err(error) = record_connection(context, true, None).await {
                tracing::error!(%error, "could not record that the node connected");
            }
            // On its own task, because asking means waiting for the node.
            // Awaited here, this module would read no further events until
            // the answer came — and the session start that follows right
            // behind would sit in the queue behind a five-second timeout.
            let context = context.clone();
            tokio::spawn(async move {
                refresh_identity(&context).await;
            });
        }
        // The node's answer to the session start: its own key, name,
        // position and radio settings. Nothing else returns them.
        AppEvent::SessionStarted { payload } => match SelfInfo::parse(&payload) {
            Ok(info) => {
                if let Err(error) = store_self(context, &info).await {
                    tracing::error!(%error, "could not store what the node says about itself");
                }
            }
            Err(error) => tracing::warn!(%error, "could not read the node's self description"),
        },
        AppEvent::NodeDisconnected { reason } => {
            if let Err(error) = record_connection(context, false, Some(&reason)).await {
                tracing::error!(%error, "could not record that the node disconnected");
            }
        }
        // Everything else belongs to other modules.
        AppEvent::Push { .. } | AppEvent::Module { .. } => {}
    }
}

/// One recorded change of the connection.
#[derive(Debug, Serialize, PartialEq)]
pub struct ConnectionEvent {
    /// Running number, ascending with arrival. Cursor for the next page.
    pub id: i64,
    /// When it happened.
    pub at: DateTime<Utc>,
    /// Whether this was a connection or a loss.
    pub connected: bool,
    /// Why it ended, when it ended.
    pub reason: Option<String>,
}

/// How many changes the listing returns unless it asks for fewer.
const DEFAULT_LIMIT: i64 = 100;

/// Largest number a single request may ask for.
const MAX_LIMIT: i64 = 1_000;

/// How many changes to return.
#[derive(Debug, Deserialize, Default)]
pub struct ListQuery {
    /// Upper bound, capped at [`MAX_LIMIT`].
    limit: Option<i64>,
    /// Only changes older than this one.
    before: Option<i64>,
    /// Which stretch of time to cover.
    #[serde(flatten)]
    range: TimeRange,
}

impl ListQuery {
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

/// Answers with the recorded connection changes, newest first.
async fn connections(
    State(context): State<AppContext>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<ConnectionEvent>>, StatusError> {
    read_connections(&context, &query.window()?)
        .await
        .map(Json)
        .map_err(StatusError::from)
}

/// Reads the connection history, newest first.
///
/// The current state alone cannot answer "is this link stable" — a node that
/// reconnects every two minutes reports itself as connected each time.
pub async fn read_connections(
    context: &AppContext,
    window: &Window,
) -> Result<Vec<ConnectionEvent>, sqlx::Error> {
    let rows: Vec<(i64, String, i64, Option<String>)> = sqlx::query_as(
        "SELECT id, at, connected, reason FROM system_connection_events
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
            Some(ConnectionEvent {
                id: row.0,
                at: parse_time(&row.1)?,
                connected: row.2 != 0,
                reason: row.3,
            })
        })
        .collect())
}

/// Answers with the current state.
async fn status(State(context): State<AppContext>) -> Result<Json<SystemStatus>, StatusError> {
    read_status(&context)
        .await
        .map(Json)
        .map_err(StatusError::from)
}

/// Reads the stored state.
pub async fn read_status(context: &AppContext) -> Result<SystemStatus, sqlx::Error> {
    let latest: Option<(String, i64, Option<String>)> = sqlx::query_as(
        "SELECT at, connected, reason FROM system_connection_events ORDER BY id DESC LIMIT 1",
    )
    .fetch_optional(context.db.pool())
    .await?;

    let (since, connected, reason) = match latest {
        Some((at, connected, reason)) => (parse_time(&at), connected != 0, reason),
        None => (None, false, None),
    };

    Ok(SystemStatus {
        connected,
        since,
        reason,
        node: read_identity(context).await?,
        node_self: read_self(context).await?,
    })
}

/// One row of `system_node_info`, in the order the query asks for it.
///
/// SQLite gives integers back as `i64`; the narrower types are restored when
/// building [`NodeIdentity`].
type IdentityRow = (String, i64, String, String, String, i64, i64, Option<i64>);

/// Reads the stored node identity, if there is one.
async fn read_identity(context: &AppContext) -> Result<Option<NodeIdentity>, sqlx::Error> {
    let row: Option<IdentityRow> = sqlx::query_as(
        "SELECT seen_at, firmware_version_code, firmware_version, manufacturer,
                    build_date, contact_capacity, group_channels, repeater_enabled
             FROM system_node_info WHERE id = 1",
    )
    .fetch_optional(context.db.pool())
    .await?;

    Ok(row.and_then(|row| {
        Some(NodeIdentity {
            seen_at: parse_time(&row.0)?,
            firmware_version_code: row.1 as u8,
            firmware_version: row.2,
            manufacturer: row.3,
            build_date: row.4,
            contact_capacity: row.5 as u16,
            group_channels: row.6 as u8,
            repeater_enabled: row.7.map(|value| value != 0),
        })
    }))
}

/// Records a change in reachability.
pub async fn record_connection(
    context: &AppContext,
    connected: bool,
    reason: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO system_connection_events (at, connected, reason) VALUES (?, ?, ?)")
        .bind(Utc::now().to_rfc3339())
        .bind(i64::from(connected))
        .bind(reason)
        .execute(context.db.pool())
        .await?;

    Ok(())
}

/// Stores what the node reported about itself.
///
/// Replaces the previous row: there is one node per instance, and an older
/// description of it is of no use once a newer one exists.
pub async fn store_identity(context: &AppContext, info: &DeviceInfo) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT OR REPLACE INTO system_node_info
            (id, seen_at, firmware_version_code, firmware_version, manufacturer,
             build_date, contact_capacity, group_channels, repeater_enabled)
         VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(i64::from(info.firmware_version_code))
    .bind(&info.firmware_version)
    .bind(&info.manufacturer)
    .bind(&info.build_date)
    .bind(i64::from(info.contact_capacity))
    .bind(i64::from(info.group_channels))
    .bind(info.repeater_enabled.map(i64::from))
    .execute(context.db.pool())
    .await?;

    Ok(())
}

/// Asks the node to describe itself and stores the answer.
///
/// Failing is not fatal: that the node is reachable is worth more than knowing
/// its name, so a silent node still leaves a recorded connection behind.
async fn refresh_identity(context: &AppContext) {
    let answer = match context
        .link
        .request(device::device_query(device::PROTOCOL_VERSION))
        .await
    {
        Ok(answer) => answer,
        Err(error) => {
            tracing::warn!(%error, "node did not answer the device query");
            return;
        }
    };

    match DeviceInfo::parse(&answer) {
        Ok(info) => {
            if let Err(error) = store_identity(context, &info).await {
                tracing::error!(%error, "could not store the node identity");
            }
        }
        Err(error) => tracing::warn!(%error, "could not read the node identity"),
    }
}

/// Reads a stored timestamp back.
///
/// A row we cannot parse is treated as absent rather than as a reason to fail
/// the whole request — the rest of the status is still useful.
fn parse_time(text: &str) -> Option<DateTime<Utc>> {
    match DateTime::parse_from_rfc3339(text) {
        Ok(time) => Some(time.with_timezone(&Utc)),
        Err(error) => {
            tracing::error!(%error, text, "stored timestamp is not RFC 3339");
            None
        }
    }
}

/// Turns a storage failure into an API error.
#[derive(Debug)]
pub enum StatusError {
    /// The database could not be read.
    Storage(sqlx::Error),
    /// The request asked for a time range that is not a time.
    BadRange(BadTimeRange),
}

impl From<sqlx::Error> for StatusError {
    fn from(error: sqlx::Error) -> Self {
        Self::Storage(error)
    }
}

impl From<BadTimeRange> for StatusError {
    fn from(error: BadTimeRange) -> Self {
        Self::BadRange(error)
    }
}

impl axum::response::IntoResponse for StatusError {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::Storage(error) => {
                tracing::error!(%error, "could not read the system status");

                // The caller learns that it failed, not how the storage is built.
                (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": {
                            "code": "storage_failed",
                            "message": "could not read the system status"
                        }
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
