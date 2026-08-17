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
use axum::{Json, Router, extract::State, routing::get};
use chrono::{DateTime, Utc};
use meshdash_core::{
    db::Migration,
    event::AppEvent,
    module::{AppContext, Module},
};
use meshdash_proto::device::{self, DeviceInfo};
use serde::Serialize;

/// Schema of this module. Versions count from 1, per module.
const MIGRATIONS: &[Migration] = &[Migration {
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
}];

/// Reports whether the node is reachable, and what it says about itself.
#[derive(Debug, Default)]
pub struct SystemModule;

/// What `/api/v1/system/status` answers.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct SystemStatus {
    /// Whether the link currently holds a connection.
    pub connected: bool,
    /// When that last changed. `None` before anything was recorded.
    pub since: Option<DateTime<Utc>>,
    /// Why the connection ended, if it did.
    pub reason: Option<String>,
    /// What the node last reported about itself.
    pub node: Option<NodeIdentity>,
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
        Some(Router::new().route("/status", get(status)))
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
            refresh_identity(context).await;
        }
        AppEvent::NodeDisconnected { reason } => {
            if let Err(error) = record_connection(context, false, Some(&reason)).await {
                tracing::error!(%error, "could not record that the node disconnected");
            }
        }
        // Everything else belongs to other modules.
        AppEvent::Push { .. } => {}
    }
}

/// Answers with the current state.
async fn status(State(context): State<AppContext>) -> Result<Json<SystemStatus>, StatusError> {
    read_status(&context).await.map(Json).map_err(StatusError)
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
pub struct StatusError(sqlx::Error);

impl axum::response::IntoResponse for StatusError {
    fn into_response(self) -> axum::response::Response {
        tracing::error!(error = %self.0, "could not read the system status");

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
}

#[cfg(test)]
mod tests;
