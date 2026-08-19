//! Which nodes does this mesh know?
//!
//! Fetches the node's contact list and keeps it, so an operator can see who is
//! out there and when each was last heard from.
//!
//! # First and last sighting are ours, the rest is the node's
//!
//! The node reports what it currently knows. When a contact first appeared in
//! *our* records, and when we last saw it, is something only MeshDash can know
//! — a node that forgets a contact would otherwise erase its own history.
//!
//! # Adverts are the live half
//!
//! The contact listing is a snapshot; adverts tell us who is being heard right
//! now, without polling. Both pushes are recorded as a sighting, and a new
//! contact is stored along the way — see [`meshdash_proto::advert`] for why the
//! two forms carry different amounts of detail.
//!
//! A short advert for a contact we do not know yet still gets recorded. The
//! sighting is true whether or not we have a name for the key; the listing on
//! the next connection fills in the rest.

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
    advert::Advert,
    contact::Contact,
    opcode::{Command, Response},
};
use serde::Serialize;

/// Schema of this module. Versions count from 1, per module.
const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        description: "known contacts with first and last sighting",
        sql: "
        CREATE TABLE nodes_contacts (
            public_key    TEXT    PRIMARY KEY,
            name          TEXT    NOT NULL,
            contact_type  INTEGER NOT NULL,
            flags         INTEGER NOT NULL,
            path          TEXT    NOT NULL,
            latitude      INTEGER,
            longitude     INTEGER,
            last_advert   INTEGER NOT NULL,
            first_seen    TEXT    NOT NULL,
            last_seen     TEXT    NOT NULL
        );

        CREATE INDEX nodes_contacts_last_seen ON nodes_contacts (last_seen);
    ",
    },
    Migration {
        version: 2,
        description: "history of advert sightings",
        sql: "
        CREATE TABLE nodes_adverts (
            id          INTEGER PRIMARY KEY,
            public_key  TEXT    NOT NULL,
            heard_at    TEXT    NOT NULL,
            was_new     INTEGER NOT NULL
        );

        CREATE INDEX nodes_adverts_heard_at ON nodes_adverts (heard_at);
    ",
    },
];

/// How many sightings the listing returns at most.
///
/// The table grows with every advert the mesh sends; an unbounded read would
/// eventually try to serialise all of it into one response.
const ADVERT_LIMIT: i64 = 200;

/// Keeps track of the contacts the node knows.
#[derive(Debug, Default)]
pub struct NodesModule;

/// One known contact, as the API reports it.
#[derive(Debug, Serialize, PartialEq)]
pub struct KnownContact {
    /// Public key, lowercase hex — see `docs/conventions.md`.
    pub public_key: String,
    /// Display name as the node reports it.
    pub name: String,
    /// Firmware's contact type. Its meaning is not verified, so it is passed
    /// through as a number rather than interpreted.
    pub contact_type: u8,
    /// Firmware flags, likewise unread.
    pub flags: u8,
    /// The known route, as hex hop bytes.
    pub path: String,
    /// Latitude in degrees, if known.
    pub latitude: Option<f64>,
    /// Longitude in degrees, if known.
    pub longitude: Option<f64>,
    /// When the contact last advertised itself, in seconds since the epoch.
    pub last_advert: u32,
    /// When MeshDash first recorded this contact.
    pub first_seen: DateTime<Utc>,
    /// When MeshDash last saw it in a listing.
    pub last_seen: DateTime<Utc>,
}

#[async_trait]
impl Module for NodesModule {
    fn name(&self) -> &'static str {
        "nodes"
    }

    fn migrations(&self) -> &'static [Migration] {
        MIGRATIONS
    }

    fn routes(&self) -> Option<Router<AppContext>> {
        Some(
            Router::new()
                .route("/contacts", get(list_contacts))
                .route("/adverts", get(list_adverts)),
        )
    }

    async fn start(&self, context: &AppContext) -> Result<(), String> {
        let context = Arc::new(context.clone());
        let mut events = context.events.subscribe();

        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    // A fresh connection is the moment to catch up.
                    Ok(AppEvent::NodeConnected) => {
                        if let Err(error) = sync_contacts(&context).await {
                            tracing::warn!(error, "could not fetch the contact list");
                        }
                    }
                    Ok(AppEvent::Push { payload }) => {
                        // Every push lands here; only adverts concern us.
                        if let Ok(advert) = Advert::parse(&payload)
                            && let Err(error) = record_advert(&context, &advert).await
                        {
                            tracing::error!(%error, "could not record an advert");
                        }
                    }
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(missed)) => {
                        tracing::warn!(missed, "nodes module missed events");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        Ok(())
    }
}

/// Answers with every known contact.
async fn list_contacts(
    State(context): State<AppContext>,
) -> Result<Json<Vec<KnownContact>>, ListError> {
    read_contacts(&context).await.map(Json).map_err(ListError)
}

/// Fetches the contact list from the node and records it.
///
/// Returns how many contacts were stored.
pub async fn sync_contacts(context: &AppContext) -> Result<usize, String> {
    // The listing ends with its own marker. Counting the number from the start
    // frame would not work: the firmware sends the *total*, not how many pass
    // the filter, so waiting for that many would hang.
    let frames = context
        .link
        .request_until(
            vec![u8::from(Command::GetContacts)],
            Box::new(|frame: &[u8]| {
                frame
                    .first()
                    .is_some_and(|&opcode| Response::from(opcode) == Response::EndOfContacts)
            }),
        )
        .await
        .map_err(|error| error.to_string())?;

    let mut stored = 0;
    for frame in &frames {
        // Start and end markers travel with the contacts; only the entries in
        // between describe one.
        let Ok(contact) = Contact::parse(frame) else {
            continue;
        };

        store_contact(context, &contact)
            .await
            .map_err(|error| error.to_string())?;
        stored += 1;
    }

    tracing::info!(stored, "fetched the contact list");
    Ok(stored)
}

/// Stores one contact, keeping its first sighting.
pub async fn store_contact(context: &AppContext, contact: &Contact) -> Result<(), sqlx::Error> {
    let now = Utc::now().to_rfc3339();

    // ON CONFLICT rather than REPLACE: replacing would reset first_seen, and
    // with it the answer to "since when do we know this node".
    sqlx::query(
        "INSERT INTO nodes_contacts
            (public_key, name, contact_type, flags, path, latitude, longitude,
             last_advert, first_seen, last_seen)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (public_key) DO UPDATE SET
            name = excluded.name,
            contact_type = excluded.contact_type,
            flags = excluded.flags,
            path = excluded.path,
            latitude = excluded.latitude,
            longitude = excluded.longitude,
            last_advert = excluded.last_advert,
            last_seen = excluded.last_seen",
    )
    .bind(to_hex(&contact.public_key))
    .bind(&contact.name)
    .bind(i64::from(contact.contact_type))
    .bind(i64::from(contact.flags))
    .bind(to_hex(&contact.path))
    .bind(contact.latitude.map(i64::from))
    .bind(contact.longitude.map(i64::from))
    .bind(i64::from(contact.last_advert))
    .bind(&now)
    .bind(&now)
    .execute(context.db.pool())
    .await?;

    Ok(())
}

/// One sighting, as the API reports it.
#[derive(Debug, Serialize, PartialEq)]
pub struct Sighting {
    /// Public key that was heard, lowercase hex.
    pub public_key: String,
    /// When MeshDash received the advert.
    pub heard_at: DateTime<Utc>,
    /// Whether the node had not known this contact before.
    pub was_new: bool,
}

/// Answers with the most recent sightings.
async fn list_adverts(State(context): State<AppContext>) -> Result<Json<Vec<Sighting>>, ListError> {
    read_adverts(&context).await.map(Json).map_err(ListError)
}

/// Records one advert: the sighting always, the contact when it came with one.
pub async fn record_advert(context: &AppContext, advert: &Advert) -> Result<(), sqlx::Error> {
    let now = Utc::now().to_rfc3339();
    let public_key = to_hex(advert.public_key());
    let was_new = matches!(advert, Advert::New(_));

    match advert {
        Advert::New(contact) => store_contact(context, contact).await?,
        // A short advert reports that a key was heard and nothing else.
        // Writing the missing fields as empty would erase what the contact
        // listing delivered.
        Advert::Known { .. } => {
            sqlx::query("UPDATE nodes_contacts SET last_seen = ? WHERE public_key = ?")
                .bind(&now)
                .bind(&public_key)
                .execute(context.db.pool())
                .await?;
        }
    }

    sqlx::query("INSERT INTO nodes_adverts (public_key, heard_at, was_new) VALUES (?, ?, ?)")
        .bind(&public_key)
        .bind(&now)
        .bind(i64::from(was_new))
        .execute(context.db.pool())
        .await?;

    Ok(())
}

/// Reads the most recent sightings, newest first.
pub async fn read_adverts(context: &AppContext) -> Result<Vec<Sighting>, sqlx::Error> {
    // The id breaks ties: two adverts can share a timestamp, and then the
    // order would otherwise be whatever SQLite feels like.
    let rows: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT public_key, heard_at, was_new FROM nodes_adverts
         ORDER BY heard_at DESC, id DESC LIMIT ?",
    )
    .bind(ADVERT_LIMIT)
    .fetch_all(context.db.pool())
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            Some(Sighting {
                public_key: row.0,
                heard_at: parse_time(&row.1)?,
                was_new: row.2 != 0,
            })
        })
        .collect())
}

/// One row of `nodes_contacts`, in the order the query asks for it.
type ContactRow = (
    String,
    String,
    i64,
    i64,
    String,
    Option<i64>,
    Option<i64>,
    i64,
    String,
    String,
);

/// Reads every known contact, most recently seen first.
pub async fn read_contacts(context: &AppContext) -> Result<Vec<KnownContact>, sqlx::Error> {
    let rows: Vec<ContactRow> = sqlx::query_as(
        "SELECT public_key, name, contact_type, flags, path, latitude, longitude,
                last_advert, first_seen, last_seen
         FROM nodes_contacts ORDER BY last_seen DESC, public_key",
    )
    .fetch_all(context.db.pool())
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            Some(KnownContact {
                public_key: row.0,
                name: row.1,
                contact_type: row.2 as u8,
                flags: row.3 as u8,
                path: row.4,
                latitude: row.5.map(|value| value as f64 / 1e6),
                longitude: row.6.map(|value| value as f64 / 1e6),
                last_advert: row.7 as u32,
                first_seen: parse_time(&row.8)?,
                last_seen: parse_time(&row.9)?,
            })
        })
        .collect())
}

/// Reads a stored timestamp back.
///
/// A row we cannot parse is left out rather than failing the whole listing —
/// the other contacts are still worth reporting.
fn parse_time(text: &str) -> Option<DateTime<Utc>> {
    match DateTime::parse_from_rfc3339(text) {
        Ok(time) => Some(time.with_timezone(&Utc)),
        Err(error) => {
            tracing::error!(%error, text, "stored timestamp is not RFC 3339");
            None
        }
    }
}

/// Turns bytes into lowercase hex, as the API spells binary data.
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
        tracing::error!(error = %self.0, "could not read the contacts");

        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": { "code": "storage_failed", "message": "could not read the contacts" }
            })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests;
