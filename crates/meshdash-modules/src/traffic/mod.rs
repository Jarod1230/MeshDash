//! What is on the air, and who hears whom.
//!
//! The node reports **every packet it hears** — `PUSH_CODE_LOG_RX_DATA`,
//! unprompted, before any check, including packets meant for others and
//! packets it went on to discard. This module writes that down.
//!
//! # Two things, kept apart
//!
//! The **log** is one row per packet and has a retention period. The
//! **summary** is who was heard by whom, kept as a pair of key prefixes with a
//! count, and stays. Traffic grows with the operating hours; the summary grows
//! with the number of prefixes, which is small. See ADR-0016.
//!
//! # What the path proves
//!
//! A forwarding station appends its own prefix to the end of the path
//! (`Mesh::routeRecvPacket`, verified against firmware `d929643`). So in a
//! packet that arrives here, every station heard the one before it, and the
//! last one was heard by this node. That is a measurement, not an inference —
//! and it is the only one that arrives without anybody transmitting for it.
//!
//! # What this module publishes
//!
//! Every packet it manages to read goes on the bus as
//! `AppEvent::Module { module: "traffic", kind: "packet" }`:
//!
//! ```json
//! { "route_type": 1, "payload_type": 2, "stations": ["fb07", "d795"],
//!   "width": 2, "snr": 12.5, "rssi": -7, "size": 23 }
//! ```
//!
//! Decoded here rather than in the browser: reading a packet means knowing the
//! protocol, and that knowledge belongs on this side. What a listener does
//! with it — draw it, count it, ignore it — is not this module's business.
//!
//! # The payload is not stored
//!
//! It is encrypted and none of MeshDash's business. It is not written down at
//! all: what is never stored cannot leak.

use std::{sync::Arc, time::Duration};

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
use meshdash_proto::{
    packet::{Packet, PayloadType, RouteType},
    push::PushEvent,
};
use serde::{Deserialize, Serialize};

use crate::query::{BadTimeRange, TimeRange, Window};

/// Schema of this module. Versions count from 1, per module.
const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    description: "the packet log and the summary of who hears whom",
    sql: "
        -- One row per heard packet. Subject to [modules.traffic] keep_days;
        -- see ADR-0016. No payload: it is encrypted and not ours to keep.
        CREATE TABLE traffic_packets (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            heard_at     TEXT    NOT NULL,
            route_type   INTEGER NOT NULL,
            payload_type INTEGER NOT NULL,
            version      INTEGER NOT NULL,
            stations     INTEGER NOT NULL,
            -- The path as it arrived, lowercase hex, entries concatenated.
            -- Stored raw rather than resolved to nodes: resolving is a guess
            -- with more than one candidate, and a guess written into a table
            -- stops looking like one.
            path         TEXT    NOT NULL,
            -- Bytes per station, as the sender chose. Says how strong a
            -- match against a contact can possibly be.
            path_width   INTEGER NOT NULL,
            snr          REAL,
            rssi         INTEGER,
            size         INTEGER NOT NULL
        );

        CREATE INDEX traffic_packets_heard_at ON traffic_packets (heard_at);

        -- Who heard whom, directly. Grows with the number of prefixes seen,
        -- not with the traffic, and is therefore kept without a deadline.
        --
        -- 'listener' is the station that heard 'talker'. The empty string as
        -- listener means this node itself, which has no prefix in a path it
        -- received.
        CREATE TABLE traffic_links (
            talker     TEXT    NOT NULL,
            listener   TEXT    NOT NULL,
            width      INTEGER NOT NULL,
            first_seen TEXT    NOT NULL,
            last_seen  TEXT    NOT NULL,
            heard      INTEGER NOT NULL,
            PRIMARY KEY (talker, listener, width)
        );
    ",
}];

/// How this module may be configured, under `[modules.traffic]`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// Whether to write the packet log at all.
    ///
    /// On, because the summary of who hears whom is built from it and there is
    /// no other source for that. Switching it off keeps the summary and drops
    /// the history.
    pub record: bool,
    /// How many days of packet log to keep.
    ///
    /// Generous on purpose: whoever wants to understand a disturbance from the
    /// week before last needs the packets, not a summary of them.
    pub keep_days: i64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            record: true,
            keep_days: 30,
        }
    }
}

/// How often old rows are swept out. Not on the hour — a sweep that lands with
/// everything else does not spread the load.
const SWEEP_EVERY: Duration = Duration::from_secs(3_600);

/// Records what the node hears, and what that says about the mesh.
#[derive(Debug, Default)]
pub struct TrafficModule;

/// One heard packet, as stored.
#[derive(Debug, Serialize, PartialEq)]
pub struct HeardPacket {
    /// Running number, ascending with arrival. Cursor for the next page.
    pub id: i64,
    /// When MeshDash was told about it.
    pub heard_at: DateTime<Utc>,
    /// Flood, direct, or one of the transport-coded forms.
    pub route_type: u8,
    /// What the packet carries — advert, text, acknowledgement, and so on.
    pub payload_type: u8,
    /// Payload version. Zero is the only one in use.
    pub version: u8,
    /// How many stations forwarded it before it arrived.
    pub stations: u8,
    /// The path as it arrived, lowercase hex.
    pub path: String,
    /// Bytes per station on that path, one to three.
    pub path_width: u8,
    /// Signal-to-noise, decibels.
    pub snr: Option<f64>,
    /// Received strength, dBm.
    pub rssi: Option<i64>,
    /// Size of the raw packet in bytes.
    pub size: i64,
}

/// One observed "this station hears that one".
#[derive(Debug, Serialize, PartialEq)]
pub struct HeardBy {
    /// Prefix of the station that transmitted, lowercase hex.
    pub talker: String,
    /// Prefix of the station that heard it. Empty means this node.
    pub listener: String,
    /// Bytes per prefix. One byte is a weak match; three is nearly certain.
    pub width: u8,
    /// When this pair was first seen.
    pub first_seen: DateTime<Utc>,
    /// When it was last seen.
    pub last_seen: DateTime<Utc>,
    /// How many packets showed it.
    pub heard: i64,
}

#[async_trait]
impl Module for TrafficModule {
    fn name(&self) -> &'static str {
        "traffic"
    }

    fn migrations(&self) -> &'static [Migration] {
        MIGRATIONS
    }

    fn routes(&self) -> Option<Router<AppContext>> {
        Some(
            Router::new()
                .route("/packets", get(list_packets))
                .route("/links", get(list_links)),
        )
    }

    async fn start(&self, context: &AppContext) -> Result<(), String> {
        let settings: Settings = context
            .settings
            .get("traffic")
            .map_err(|error| error.to_string())?;

        let context = Arc::new(context.clone());
        let mut events = context.events.subscribe();

        let listening = Arc::clone(&context);
        let recording = settings.record;
        tokio::spawn(async move {
            loop {
                let event = match events.recv().await {
                    Ok(event) => event,
                    // Falling behind costs history, but carrying on is better
                    // than giving up on the rest.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(missed)) => {
                        tracing::warn!(missed, "traffic module missed events");
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                };

                if let AppEvent::Push { payload } = event {
                    handle_push(&listening, &payload, recording).await;
                }
            }
        });

        let sweeping = Arc::clone(&context);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(SWEEP_EVERY);
            loop {
                ticker.tick().await;
                match sweep(&sweeping, settings.keep_days).await {
                    Ok(0) => {}
                    Ok(removed) => tracing::info!(removed, "swept out old traffic"),
                    Err(error) => tracing::error!(%error, "could not sweep old traffic"),
                }
            }
        });

        Ok(())
    }
}

/// Reads one push and keeps what it says.
async fn handle_push(context: &AppContext, payload: &[u8], recording: bool) {
    let Ok(PushEvent::ReceivedPacketLog { snr, rssi, packet }) = PushEvent::parse(payload) else {
        // Everything else belongs to another module.
        return;
    };

    let parsed = match Packet::parse(&packet) {
        Ok(parsed) => parsed,
        // A packet this build cannot read is not an error worth shouting
        // about: newer firmware may carry shapes this one has never seen.
        Err(error) => {
            tracing::debug!(?error, "could not read a heard packet");
            return;
        }
    };

    if let Err(error) = record_hearing(context, &parsed).await {
        tracing::error!(%error, "could not record who heard whom");
    }

    if recording {
        if let Err(error) = record_packet(context, &parsed, packet.len(), snr, rssi).await {
            tracing::error!(%error, "could not record a heard packet");
        }
    }

    announce(context, &parsed, packet.len(), snr, rssi);
}

/// Puts one read packet on the bus for whoever wants to watch it travel.
///
/// Published whether or not it was recorded: watching what is happening now
/// and keeping a history are two different wishes, and switching off the
/// second should not switch off the first.
fn announce(context: &AppContext, packet: &Packet<'_>, size: usize, snr: f32, rssi: i8) {
    context.events.publish(AppEvent::Module {
        module: "traffic".to_owned(),
        kind: "packet".to_owned(),
        data: serde_json::json!({
            "route_type": route_byte(packet.route),
            "payload_type": payload_byte(packet.payload_type),
            // In travel order, and as prefixes — resolving them to nodes is a
            // guess whenever more than one key starts the same way, and the
            // reader is the one who knows how many candidates it has.
            "stations": packet
                .path
                .iter()
                .map(|station| to_hex(station.key_prefix))
                .collect::<Vec<_>>(),
            "width": packet.shape.bytes_per_station,
            "snr": snr,
            "rssi": rssi,
            "size": size,
        }),
    });
}

/// Writes one packet into the log.
pub async fn record_packet(
    context: &AppContext,
    packet: &Packet<'_>,
    size: usize,
    snr: f32,
    rssi: i8,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO traffic_packets
            (heard_at, route_type, payload_type, version, stations, path, path_width,
             snr, rssi, size)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(i64::from(route_byte(packet.route)))
    .bind(i64::from(payload_byte(packet.payload_type)))
    .bind(i64::from(packet.version))
    .bind(i64::try_from(packet.stations()).unwrap_or(i64::MAX))
    .bind(path_hex(packet))
    .bind(i64::from(packet.shape.bytes_per_station))
    .bind(f64::from(snr))
    .bind(i64::from(rssi))
    .bind(i64::try_from(size).unwrap_or(i64::MAX))
    .execute(context.db.pool())
    .await?;

    Ok(())
}

/// Notes every "heard directly" the path of this packet proves.
///
/// Station `n + 1` heard station `n`, and this node heard the last one. An
/// empty path proves nothing here: the sender is named only inside the
/// payload, which is encrypted.
pub async fn record_hearing(context: &AppContext, packet: &Packet<'_>) -> Result<(), sqlx::Error> {
    let stations: Vec<String> = packet
        .path
        .iter()
        .map(|station| to_hex(station.key_prefix))
        .collect();

    let now = Utc::now().to_rfc3339();
    let width = i64::from(packet.shape.bytes_per_station);

    for pair in stations.windows(2) {
        let (talker, listener) = (&pair[0], &pair[1]);
        note_pair(context, talker, listener, width, &now).await?;
    }

    // The last station is the one this node heard. Its listener is written as
    // the empty string: a node that receives a packet does not appear in its
    // own path, so there is no prefix to write.
    if let Some(last) = stations.last() {
        note_pair(context, last, "", width, &now).await?;
    }

    Ok(())
}

/// Counts one sighting of a pair, creating it if it is new.
async fn note_pair(
    context: &AppContext,
    talker: &str,
    listener: &str,
    width: i64,
    now: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO traffic_links (talker, listener, width, first_seen, last_seen, heard)
         VALUES (?1, ?2, ?3, ?4, ?4, 1)
         ON CONFLICT(talker, listener, width) DO UPDATE SET
            last_seen = excluded.last_seen,
            heard = heard + 1",
    )
    .bind(talker)
    .bind(listener)
    .bind(width)
    .bind(now)
    .execute(context.db.pool())
    .await?;

    Ok(())
}

/// Removes packets older than the retention period. Returns how many went.
pub async fn sweep(context: &AppContext, keep_days: i64) -> Result<u64, sqlx::Error> {
    // A period of zero or less would empty the table on every sweep, which is
    // never what somebody meant by it.
    if keep_days <= 0 {
        return Ok(0);
    }

    let cutoff = (Utc::now() - chrono::Duration::days(keep_days)).to_rfc3339();
    let removed = sqlx::query("DELETE FROM traffic_packets WHERE heard_at < ?")
        .bind(cutoff)
        .execute(context.db.pool())
        .await?
        .rows_affected();

    Ok(removed)
}

/// The route type as the firmware numbers it, `src/Packet.h`.
///
/// Written out rather than cast from the enum: a cast would silently follow
/// the order the variants happen to be declared in, and a wrong number here
/// produces no error at all — only wrong rows.
fn route_byte(route: RouteType) -> u8 {
    match route {
        RouteType::TransportFlood => 0,
        RouteType::Flood => 1,
        RouteType::Direct => 2,
        RouteType::TransportDirect => 3,
    }
}

/// The payload type as the firmware numbers it, `src/Packet.h`.
fn payload_byte(payload: PayloadType) -> u8 {
    match payload {
        PayloadType::Request => 0x00,
        PayloadType::Response => 0x01,
        PayloadType::TextMessage => 0x02,
        PayloadType::Ack => 0x03,
        PayloadType::Advert => 0x04,
        PayloadType::GroupText => 0x05,
        PayloadType::GroupData => 0x06,
        PayloadType::AnonymousRequest => 0x07,
        PayloadType::Path => 0x08,
        PayloadType::Trace => 0x09,
        PayloadType::Multipart => 0x0A,
        PayloadType::Control => 0x0B,
        PayloadType::RawCustom => 0x0F,
        // Kept as it arrived: a build that does not know a type must not turn
        // it into one it does know.
        PayloadType::Unknown(value) => value,
    }
}

/// The path of a packet as one hex string, entries in travel order.
fn path_hex(packet: &Packet<'_>) -> String {
    packet
        .path
        .iter()
        .map(|station| to_hex(station.key_prefix))
        .collect()
}

/// Turns bytes into lowercase hex, as the API spells binary data.
fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}

/// Which packets to answer with.
#[derive(Debug, Deserialize, Default)]
pub struct PacketQuery {
    /// Only packets older than this one.
    before: Option<i64>,
    /// How many to return.
    limit: Option<i64>,
    /// Which stretch of time to cover.
    #[serde(flatten)]
    range: TimeRange,
}

impl PacketQuery {
    /// What the request asks for, or what is wrong with its time range.
    fn window(&self) -> Result<Window, BadTimeRange> {
        Ok(Window::new(
            self.limit.unwrap_or(DEFAULT_PACKETS).clamp(1, MAX_PACKETS),
            self.before,
            self.range.bounds()?,
        ))
    }
}

/// Largest number of packets one request may ask for.
const MAX_PACKETS: i64 = 1_000;
/// How many it gets without saying.
const DEFAULT_PACKETS: i64 = 200;

async fn list_packets(
    State(context): State<AppContext>,
    Query(query): Query<PacketQuery>,
) -> Result<Json<Vec<HeardPacket>>, TrafficError> {
    Ok(Json(read_packets(&context, &query.window()?).await?))
}

async fn list_links(State(context): State<AppContext>) -> Result<Json<Vec<HeardBy>>, TrafficError> {
    Ok(Json(read_links(&context).await?))
}

type PacketRow = (
    i64,
    String,
    i64,
    i64,
    i64,
    i64,
    String,
    i64,
    Option<f64>,
    Option<i64>,
    i64,
);

/// Reads the packet log, newest first.
pub async fn read_packets(
    context: &AppContext,
    window: &Window,
) -> Result<Vec<HeardPacket>, sqlx::Error> {
    let rows: Vec<PacketRow> = sqlx::query_as(
        "SELECT id, heard_at, route_type, payload_type, version, stations, path,
                path_width, snr, rssi, size
         FROM traffic_packets
         WHERE (?1 IS NULL OR id < ?1)
           AND (?2 IS NULL OR heard_at >= ?2)
           AND (?3 IS NULL OR heard_at <= ?3)
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
            Some(HeardPacket {
                id: row.0,
                heard_at: parse_time(&row.1)?,
                route_type: row.2 as u8,
                payload_type: row.3 as u8,
                version: row.4 as u8,
                stations: row.5 as u8,
                path: row.6,
                path_width: row.7 as u8,
                snr: row.8,
                rssi: row.9,
                size: row.10,
            })
        })
        .collect())
}

/// Reads the summary of who hears whom, most recently seen first.
pub async fn read_links(context: &AppContext) -> Result<Vec<HeardBy>, sqlx::Error> {
    let rows: Vec<(String, String, i64, String, String, i64)> = sqlx::query_as(
        "SELECT talker, listener, width, first_seen, last_seen, heard
         FROM traffic_links ORDER BY last_seen DESC",
    )
    .fetch_all(context.db.pool())
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            Some(HeardBy {
                talker: row.0,
                listener: row.1,
                width: row.2 as u8,
                first_seen: parse_time(&row.3)?,
                last_seen: parse_time(&row.4)?,
                heard: row.5,
            })
        })
        .collect())
}

/// Reads a stored timestamp back.
///
/// A row we cannot parse is left out rather than failing the whole request —
/// the rest of the answer is still useful.
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
pub enum TrafficError {
    /// The database could not be read.
    Storage(sqlx::Error),
    /// The request asked for a time range that is not a time.
    BadRange(BadTimeRange),
}

impl From<sqlx::Error> for TrafficError {
    fn from(error: sqlx::Error) -> Self {
        Self::Storage(error)
    }
}

impl From<BadTimeRange> for TrafficError {
    fn from(error: BadTimeRange) -> Self {
        Self::BadRange(error)
    }
}

impl axum::response::IntoResponse for TrafficError {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::Storage(error) => {
                tracing::error!(%error, "could not read the traffic log");

                (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": {
                            "code": "storage_failed",
                            "message": "could not read the traffic log"
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
