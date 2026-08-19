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
//! # Only the attached node, for now
//!
//! Telemetry from other nodes travels as CayenneLPP inside
//! `PUSH_CODE_TELEMETRY_RESPONSE`. That is a foreign format and needs its own
//! decision before anything reads it; this module sticks to what is verified.
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
use meshdash_proto::battery::{self, BatteryAndStorage};
use serde::{Deserialize, Serialize};

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
];

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

/// Records battery and storage over time.
#[derive(Debug, Default)]
pub struct TelemetryModule;

/// One stored reception-quality reading.
#[derive(Debug, Serialize, PartialEq)]
pub struct SignalSample {
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
}

impl ListQuery {
    /// The number of rows to read, within the bounds this module allows.
    fn effective_limit(&self) -> i64 {
        self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
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
                .route("/signal", get(list_signal_samples)),
        )
    }

    async fn start(&self, context: &AppContext) -> Result<(), String> {
        let context = Arc::new(context.clone());

        // One task listens, so a fresh connection is sampled at once instead of
        // waiting out the interval.
        let on_connect = Arc::clone(&context);
        let mut events = context.events.subscribe();
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(AppEvent::NodeConnected) => take_sample(&on_connect).await,
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
    read_signal_samples(&context, query.effective_limit())
        .await
        .map(Json)
        .map_err(ListError)
}

/// Reads stored reception qualities, newest first.
pub async fn read_signal_samples(
    context: &AppContext,
    limit: i64,
) -> Result<Vec<SignalSample>, sqlx::Error> {
    let rows: Vec<(String, String, f64, Option<i64>)> = sqlx::query_as(
        "SELECT at, source, snr, path_len FROM telemetry_signal_samples
         ORDER BY id DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(context.db.pool())
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            Some(SignalSample {
                at: match DateTime::parse_from_rfc3339(&row.0) {
                    Ok(time) => time.with_timezone(&Utc),
                    Err(error) => {
                        tracing::error!(%error, "stored timestamp is not RFC 3339");
                        return None;
                    }
                },
                source: row.1,
                snr: row.2 as f32,
                path_len: row.3.map(|value| value as u8),
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
    limit: i64,
) -> Result<Vec<BatterySample>, sqlx::Error> {
    let rows: Vec<(String, i64, i64, i64)> = sqlx::query_as(
        "SELECT at, millivolts, storage_used_kib, storage_total_kib
         FROM telemetry_battery_samples ORDER BY id DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(context.db.pool())
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            Some(BatterySample {
                at: match DateTime::parse_from_rfc3339(&row.0) {
                    Ok(time) => time.with_timezone(&Utc),
                    Err(error) => {
                        tracing::error!(%error, "stored timestamp is not RFC 3339");
                        return None;
                    }
                },
                millivolts: row.1 as u16,
                storage_used_kib: row.2 as u32,
                storage_total_kib: row.3 as u32,
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
    read_samples(&context, query.effective_limit())
        .await
        .map(Json)
        .map_err(ListError)
}

/// Turns a storage failure into an API error.
#[derive(Debug)]
pub struct ListError(sqlx::Error);

impl axum::response::IntoResponse for ListError {
    fn into_response(self) -> axum::response::Response {
        tracing::error!(error = %self.0, "could not read the battery readings");

        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": { "code": "storage_failed", "message": "could not read the readings" }
            })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests;
