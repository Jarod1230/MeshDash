//! How the node's battery and storage develop over time.
//!
//! Asks the node at a fixed interval and keeps every reading, so an operator
//! can see a curve rather than a single number — "is the battery falling
//! faster than last week" is the question this answers.
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
const MIGRATIONS: &[Migration] = &[Migration {
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
}];

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

#[async_trait]
impl Module for TelemetryModule {
    fn name(&self) -> &'static str {
        "telemetry"
    }

    fn migrations(&self) -> &'static [Migration] {
        MIGRATIONS
    }

    fn routes(&self) -> Option<Router<AppContext>> {
        Some(Router::new().route("/battery", get(list_samples)))
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
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
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
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);

    read_samples(&context, limit)
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
