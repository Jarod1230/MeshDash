//! Warnings for the nodes an operator watches, and for losing the own node.
//!
//! # What warns
//!
//! Two things, and deliberately no more (ADR-0021):
//!
//! - **A watched node falls silent.** No advert from it for longer than
//!   `silent_after_hours`. Only nodes the operator marked are watched: the
//!   mesh knows every node that was ever heard once, and warning about all of
//!   them would warn about a repeater a freak of propagation carried in from
//!   a hundred kilometres away — every day, forever.
//! - **The own node is gone.** The link has been down for longer than
//!   `disconnected_after_minutes`.
//!
//! # Why adverts, and only adverts
//!
//! An advert names its sender by the full public key — both the push the node
//! sends for it and the raw packet in the receive log (`Packet::advert_sender`).
//! A path names stations by one to three bytes; at one byte every twin of a
//! watched node would count as "heard", and a warning that stays away because
//! another node shares a first byte is worse than none. This module keeps its
//! own "last heard" for the nodes it watches, because it may not read the
//! tables of `nodes` (`docs/module-system.md`).
//!
//! # Where a warning goes
//!
//! Into this module's log, with the moment the condition began, and onto the
//! bus as `AppEvent::Module { module: "alerts", … }`. The event stream carries
//! it to the browser. Nowhere else: MeshDash runs where there is no uplink,
//! and a webhook would stay silent exactly when it mattered.
//!
//! # Events published
//!
//! `kind: "raised"` and `kind: "cleared"`, both with the same `data`:
//!
//! ```json
//! { "id": 7, "kind": "silent", "subject": "<64 hex>",
//!   "since": "2026-09-18T20:11:52Z", "raised_at": "2026-09-19T20:12:00Z",
//!   "cleared_at": null }
//! ```
//!
//! `kind` inside `data` is `"silent"` or `"disconnected"`; `subject` is the
//! watched node's key, or empty for the own node.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, put},
};
use chrono::{DateTime, Utc};
use meshdash_core::{
    db::Migration,
    event::AppEvent,
    module::{AppContext, Module},
};
use meshdash_proto::{packet::Packet, push::PushEvent};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

/// Schema of this module. Versions count from 1, per module.
const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    description: "watched nodes and the warnings about them",
    sql: "
        -- The nodes the operator asked to be warned about, by full public
        -- key, and when this module last heard an advert from each.
        -- last_heard_at is NULL until the first one: a node watched but never
        -- heard is measured from added_at, so it warns after the same grace.
        CREATE TABLE alerts_watched (
            public_key    TEXT PRIMARY KEY,
            added_at      TEXT NOT NULL,
            last_heard_at TEXT
        );

        -- Every warning, open or over. 'since' is when the condition began —
        -- the last advert, the moment the link dropped — and says more than
        -- raised_at, which only says when the grace ran out.
        CREATE TABLE alerts_log (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            kind       TEXT NOT NULL,
            subject    TEXT NOT NULL,
            since      TEXT NOT NULL,
            raised_at  TEXT NOT NULL,
            cleared_at TEXT
        );

        CREATE INDEX alerts_log_open ON alerts_log (cleared_at);
    ",
}];

/// How this module may be configured, under `[modules.alerts]`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// How long a watched node may go without an advert.
    ///
    /// Repeaters advertise on their own every few hours; a day leaves room
    /// for a missed one or two without calling a node dead.
    pub silent_after_hours: i64,
    /// How long the own node may be gone before it is a warning.
    ///
    /// A reconnect after an unplugged cable or a firmware restart takes
    /// seconds. Minutes mean something is actually wrong.
    pub disconnected_after_minutes: i64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            silent_after_hours: 24,
            disconnected_after_minutes: 5,
        }
    }
}

/// How often the conditions are looked at.
const EVALUATE_EVERY: Duration = Duration::from_secs(30);

/// What a warning is about.
pub const SILENT: &str = "silent";
/// The own node is not connected.
pub const DISCONNECTED: &str = "disconnected";

/// Since when the own node has been gone, or `None` while it is connected.
///
/// Starts as "gone since start": the link opens only after every module is
/// listening (`meshdash-server`'s `main`), so a node that connects at all is
/// seen connecting.
#[derive(Debug)]
pub struct LinkState {
    gone_since: Mutex<Option<DateTime<Utc>>>,
}

impl LinkState {
    /// Gone since `at`, until told otherwise.
    pub fn gone_since(at: DateTime<Utc>) -> Self {
        Self {
            gone_since: Mutex::new(Some(at)),
        }
    }
}

/// Warnings for watched nodes and the own node.
#[derive(Debug, Default)]
pub struct AlertsModule;

#[async_trait]
impl Module for AlertsModule {
    fn name(&self) -> &'static str {
        "alerts"
    }

    fn migrations(&self) -> &'static [Migration] {
        MIGRATIONS
    }

    fn routes(&self) -> Option<Router<AppContext>> {
        Some(
            Router::new()
                .route("/watched", get(list_watched))
                .route("/watched/{key}", put(put_watched).delete(delete_watched))
                .route("/log", get(list_alerts)),
        )
    }

    async fn start(&self, context: &AppContext) -> Result<(), String> {
        // Read once to fail early on a misspelled option; the loops below
        // read it again, so a change takes effect without a restart.
        let _: Settings = context
            .settings
            .get("alerts")
            .map_err(|error| error.to_string())?;

        let context = Arc::new(context.clone());
        let link = Arc::new(LinkState::gone_since(Utc::now()));
        let mut events = context.events.subscribe();

        let listening = Arc::clone(&context);
        let listening_link = Arc::clone(&link);
        tokio::spawn(async move {
            loop {
                let event = match events.recv().await {
                    Ok(event) => event,
                    // A missed advert costs at worst a late all-clear; carry on.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(missed)) => {
                        tracing::warn!(missed, "alerts module missed events");
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                };

                if let Err(error) = handle_event(&listening, &listening_link, &event).await {
                    tracing::error!(%error, "could not note an event for the alerts");
                }
            }
        });

        let evaluating = Arc::clone(&context);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(EVALUATE_EVERY);
            loop {
                ticker.tick().await;
                let settings = evaluating
                    .settings
                    .get::<Settings>("alerts")
                    .unwrap_or_default();

                if let Err(error) = evaluate(&evaluating, &settings, &link, Utc::now()).await {
                    tracing::error!(%error, "could not evaluate the alerts");
                }
            }
        });

        Ok(())
    }
}

/// Reads one event for what it says about a watched node or the link.
pub async fn handle_event(
    context: &AppContext,
    link: &LinkState,
    event: &AppEvent,
) -> Result<(), sqlx::Error> {
    match event {
        AppEvent::NodeConnected => {
            *link.gone_since.lock().await = None;
            clear(context, DISCONNECTED, "", Utc::now()).await?;
        }
        AppEvent::NodeDisconnected { .. } => {
            let mut gone = link.gone_since.lock().await;
            if gone.is_none() {
                *gone = Some(Utc::now());
            }
        }
        AppEvent::Push { payload } => {
            if let Some(key) = advertised_key(payload) {
                heard(context, &key, Utc::now()).await?;
            }
        }
        _ => {}
    }

    Ok(())
}

/// The full key of whoever advertised, if this push is an advert at all.
///
/// Two forms say the same: the node's own advert push, and the raw packet in
/// its receive log. A companion that hears an advert usually sends both.
fn advertised_key(payload: &[u8]) -> Option<String> {
    match PushEvent::parse(payload).ok()? {
        PushEvent::Advert(advert) => Some(to_hex(advert.public_key())),
        PushEvent::ReceivedPacketLog { packet, .. } => {
            Packet::parse(&packet).ok()?.advert_sender().map(to_hex)
        }
        _ => None,
    }
}

/// Notes an advert from `key`, and ends its silence if it was silent.
pub async fn heard(context: &AppContext, key: &str, at: DateTime<Utc>) -> Result<(), sqlx::Error> {
    let watched = sqlx::query("UPDATE alerts_watched SET last_heard_at = ? WHERE public_key = ?")
        .bind(at.to_rfc3339())
        .bind(key)
        .execute(context.db.pool())
        .await?
        .rows_affected();

    if watched > 0 {
        clear(context, SILENT, key, at).await?;
    }

    Ok(())
}

/// Raises what has run past its grace. Called on a timer; `now` is a
/// parameter so a test can say what time it is.
pub async fn evaluate(
    context: &AppContext,
    settings: &Settings,
    link: &LinkState,
    now: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    let silent_cutoff = now - chrono::Duration::hours(settings.silent_after_hours);

    let watched: Vec<(String, String)> =
        sqlx::query_as("SELECT public_key, COALESCE(last_heard_at, added_at) FROM alerts_watched")
            .fetch_all(context.db.pool())
            .await?;

    for (key, reference) in watched {
        let Some(since) = parse_time(&reference) else {
            continue;
        };
        if since < silent_cutoff {
            raise(context, SILENT, &key, since, now).await?;
        }
    }

    let gone = *link.gone_since.lock().await;
    if let Some(since) = gone {
        if since < now - chrono::Duration::minutes(settings.disconnected_after_minutes) {
            raise(context, DISCONNECTED, "", since, now).await?;
        }
    }

    Ok(())
}

/// Opens a warning unless the same one is open already.
async fn raise(
    context: &AppContext,
    kind: &str,
    subject: &str,
    since: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    let open: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM alerts_log WHERE kind = ? AND subject = ? AND cleared_at IS NULL",
    )
    .bind(kind)
    .bind(subject)
    .fetch_optional(context.db.pool())
    .await?;

    if open.is_some() {
        return Ok(());
    }

    let id =
        sqlx::query("INSERT INTO alerts_log (kind, subject, since, raised_at) VALUES (?, ?, ?, ?)")
            .bind(kind)
            .bind(subject)
            .bind(since.to_rfc3339())
            .bind(now.to_rfc3339())
            .execute(context.db.pool())
            .await?
            .last_insert_rowid();

    tracing::info!(kind, subject, "raised a warning");
    publish(context, "raised", id).await
}

/// Closes the open warning of this kind and subject, if there is one.
async fn clear(
    context: &AppContext,
    kind: &str,
    subject: &str,
    at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    let open: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM alerts_log WHERE kind = ? AND subject = ? AND cleared_at IS NULL",
    )
    .bind(kind)
    .bind(subject)
    .fetch_optional(context.db.pool())
    .await?;

    let Some((id,)) = open else {
        return Ok(());
    };

    sqlx::query("UPDATE alerts_log SET cleared_at = ? WHERE id = ?")
        .bind(at.to_rfc3339())
        .bind(id)
        .execute(context.db.pool())
        .await?;

    tracing::info!(kind, subject, "cleared a warning");
    publish(context, "cleared", id).await
}

/// Puts one warning on the bus, as it now stands.
async fn publish(context: &AppContext, what: &str, id: i64) -> Result<(), sqlx::Error> {
    if let Some(alert) = read_alert(context, id).await? {
        context.events.publish(AppEvent::Module {
            module: "alerts".to_owned(),
            kind: what.to_owned(),
            data: serde_json::to_value(alert).unwrap_or_default(),
        });
    }

    Ok(())
}

/// One warning, as the API and the bus spell it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Alert {
    pub id: i64,
    /// `"silent"` or `"disconnected"`.
    pub kind: String,
    /// The watched node's key; empty for the own node.
    pub subject: String,
    /// When the condition began.
    pub since: DateTime<Utc>,
    /// When the grace ran out and the warning was raised.
    pub raised_at: DateTime<Utc>,
    /// When it ended; `None` while it is still on.
    pub cleared_at: Option<DateTime<Utc>>,
}

type AlertRow = (i64, String, String, String, String, Option<String>);

fn to_alert(row: AlertRow) -> Option<Alert> {
    Some(Alert {
        id: row.0,
        kind: row.1,
        subject: row.2,
        since: parse_time(&row.3)?,
        raised_at: parse_time(&row.4)?,
        cleared_at: match row.5 {
            Some(text) => Some(parse_time(&text)?),
            None => None,
        },
    })
}

async fn read_alert(context: &AppContext, id: i64) -> Result<Option<Alert>, sqlx::Error> {
    let row: Option<AlertRow> = sqlx::query_as(
        "SELECT id, kind, subject, since, raised_at, cleared_at FROM alerts_log WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(context.db.pool())
    .await?;

    Ok(row.and_then(to_alert))
}

/// Open warnings first, then the most recent that ended.
pub async fn read_alerts(context: &AppContext, limit: i64) -> Result<Vec<Alert>, sqlx::Error> {
    let rows: Vec<AlertRow> = sqlx::query_as(
        "SELECT id, kind, subject, since, raised_at, cleared_at FROM alerts_log
         ORDER BY cleared_at IS NOT NULL, raised_at DESC, id DESC
         LIMIT ?",
    )
    .bind(limit)
    .fetch_all(context.db.pool())
    .await?;

    Ok(rows.into_iter().filter_map(to_alert).collect())
}

/// A node on the watch list.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Watched {
    pub public_key: String,
    pub added_at: DateTime<Utc>,
    /// `None` until an advert from it has been heard since it was added.
    pub last_heard_at: Option<DateTime<Utc>>,
}

pub async fn read_watched(context: &AppContext) -> Result<Vec<Watched>, sqlx::Error> {
    let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT public_key, added_at, last_heard_at FROM alerts_watched ORDER BY added_at",
    )
    .fetch_all(context.db.pool())
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|(public_key, added_at, last_heard_at)| {
            Some(Watched {
                public_key,
                added_at: parse_time(&added_at)?,
                last_heard_at: last_heard_at.as_deref().and_then(parse_time),
            })
        })
        .collect())
}

/// Starts watching a node. Watching one twice changes nothing.
pub async fn watch(context: &AppContext, key: &str, at: DateTime<Utc>) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT OR IGNORE INTO alerts_watched (public_key, added_at) VALUES (?, ?)")
        .bind(key)
        .bind(at.to_rfc3339())
        .execute(context.db.pool())
        .await?;

    Ok(())
}

/// Stops watching a node, and ends a warning about it: nobody asked any more.
pub async fn unwatch(
    context: &AppContext,
    key: &str,
    at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM alerts_watched WHERE public_key = ?")
        .bind(key)
        .execute(context.db.pool())
        .await?;

    clear(context, SILENT, key, at).await
}

/// A full public key: 64 lowercase hex digits. Anything shorter is a prefix,
/// and a prefix cannot be watched — see the top of this file.
fn is_public_key(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

async fn list_watched(
    State(context): State<AppContext>,
) -> Result<Json<Vec<Watched>>, AlertsError> {
    Ok(Json(read_watched(&context).await?))
}

async fn put_watched(
    State(context): State<AppContext>,
    Path(key): Path<String>,
) -> Result<StatusCode, AlertsError> {
    if !is_public_key(&key) {
        return Err(AlertsError::NotAKey);
    }
    watch(&context, &key, Utc::now()).await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn delete_watched(
    State(context): State<AppContext>,
    Path(key): Path<String>,
) -> Result<StatusCode, AlertsError> {
    if !is_public_key(&key) {
        return Err(AlertsError::NotAKey);
    }
    unwatch(&context, &key, Utc::now()).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Query of `GET /api/v1/alerts/log`.
#[derive(Debug, Deserialize)]
struct AlertsQuery {
    limit: Option<i64>,
}

async fn list_alerts(
    State(context): State<AppContext>,
    Query(query): Query<AlertsQuery>,
) -> Result<Json<Vec<Alert>>, AlertsError> {
    let limit = query.limit.unwrap_or(100).clamp(1, 1000);

    Ok(Json(read_alerts(&context, limit).await?))
}

/// Why a request to this module failed.
#[derive(Debug)]
pub enum AlertsError {
    /// The database could not be read or written.
    Storage(sqlx::Error),
    /// The path named something that is not a full public key.
    NotAKey,
}

impl From<sqlx::Error> for AlertsError {
    fn from(error: sqlx::Error) -> Self {
        Self::Storage(error)
    }
}

impl IntoResponse for AlertsError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Storage(error) => {
                tracing::error!(%error, "could not read or write the alerts");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "storage_failed",
                    "could not read or write the alerts",
                )
            }
            Self::NotAKey => (
                StatusCode::BAD_REQUEST,
                "not_a_public_key",
                "a watched node is named by its full public key: 64 lowercase hex digits",
            ),
        };

        (
            status,
            Json(serde_json::json!({ "error": { "code": code, "message": message } })),
        )
            .into_response()
    }
}

fn parse_time(text: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|time| time.with_timezone(&Utc))
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests;
