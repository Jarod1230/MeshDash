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
//! # What this module publishes
//!
//! Every contact it stores is announced on the bus as `AppEvent::Module` with
//! module `nodes` and kind `contact`:
//!
//! ```json
//! { "public_key": "a1a1…", "name": "Repeater Nord" }
//! ```
//!
//! That is how `messages` learns whose six-byte prefix belongs to which name.
//! It cannot read this module's tables, and this module has no business
//! knowing that messages exist — see
//! `docs/decisions/0007-modul-ereignisse.md`.
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
use axum::{
    Json, Router,
    extract::{Query, State},
    routing::{get, put},
};
use chrono::{DateTime, Utc};
use meshdash_core::{
    db::Migration,
    event::AppEvent,
    module::{AppContext, Module},
};
use meshdash_proto::{
    advert::Advert, command, contact::Contact, opcode::Response, push::PushEvent,
};
use serde::{Deserialize, Serialize};

use crate::query::{BadTimeRange, TimeRange, Window};

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
    Migration {
        version: 3,
        description: "routes are stored as stations and hops, not as a byte count",
        sql: "
        -- The length byte of a route packs two fields: how many stations,
        -- and how many bytes each takes (meshdash_proto::path). Reading it as
        -- a plain byte count made 0xFF — the firmware's marker for 'no route'
        -- — into a 64-hop journey, and 64 into sixty-four stations when it
        -- means none. Real hardware carries these values constantly.
        --
        -- `path` becomes nullable and gains a station count beside it, which
        -- is not derivable from the byte length. SQLite cannot drop NOT NULL
        -- in place, so the table is rebuilt.
        CREATE TABLE nodes_contacts_rebuilt (
            public_key    TEXT    PRIMARY KEY,
            name          TEXT    NOT NULL,
            contact_type  INTEGER NOT NULL,
            flags         INTEGER NOT NULL,
            path          TEXT,
            stations      INTEGER,
            latitude      INTEGER,
            longitude     INTEGER,
            last_advert   INTEGER NOT NULL,
            first_seen    TEXT    NOT NULL,
            last_seen     TEXT    NOT NULL
        );

        -- Old paths were decoded with the wrong rule, so none of them can be
        -- trusted and no station count can be recovered from them. Both are
        -- dropped; the next contact listing — one arrives on every connection
        -- — fills them in correctly.
        INSERT INTO nodes_contacts_rebuilt
            SELECT public_key, name, contact_type, flags, NULL, NULL,
                   latitude, longitude, last_advert, first_seen, last_seen
            FROM nodes_contacts;

        DROP TABLE nodes_contacts;
        ALTER TABLE nodes_contacts_rebuilt RENAME TO nodes_contacts;

        CREATE INDEX nodes_contacts_last_seen ON nodes_contacts (last_seen);
    ",
    },
    Migration {
        version: 4,
        description: "history of route changes",
        sql: "
        -- The contact row carries only the route that holds right now. A mesh
        -- reroutes on its own, and the interesting part — that a node moved
        -- from one station to three, and when — was overwritten each time.
        CREATE TABLE nodes_route_changes (
            id                INTEGER PRIMARY KEY,
            public_key        TEXT    NOT NULL,
            changed_at        TEXT    NOT NULL,
            path              TEXT,
            stations          INTEGER,
            previous_path     TEXT,
            previous_stations INTEGER
        );

        CREATE INDEX nodes_route_changes_key ON nodes_route_changes (public_key, id);
    ",
    },
    Migration {
        version: 5,
        description: "positions the operator set by hand",
        sql: "
        -- Kept apart from nodes_contacts on purpose. That row is overwritten
        -- by every contact listing and every advert; a correction the
        -- operator made would be gone with the next one. Here it survives,
        -- and it survives the contact being forgotten and rediscovered too.
        CREATE TABLE nodes_manual_positions (
            public_key  TEXT    PRIMARY KEY,
            latitude    REAL    NOT NULL,
            longitude   REAL    NOT NULL,
            set_at      TEXT    NOT NULL
        );
    ",
    },
];

/// Largest number of sightings a single request may ask for.
///
/// The table grows with every advert the mesh sends; an unbounded read would
/// eventually try to serialise all of it into one response.
const ADVERT_LIMIT: i64 = 200;

/// Largest number a single request may ask for.
const MAX_ADVERT_LIMIT: i64 = 2_000;

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
    /// The known route as hex hop bytes, or `None` when the node has no route
    /// to this contact.
    ///
    /// An empty string is not the same thing: it means reachable directly.
    pub path: Option<String>,
    /// How many stations the route passes through, or `None` without a route.
    ///
    /// Not derivable from `path`: a station can take more than one byte, so
    /// the hop count and the byte count are different numbers. See
    /// `meshdash_proto::path`.
    pub stations: Option<u8>,
    /// Latitude in degrees that applies, if any.
    ///
    /// A position the operator set wins over the one the node reports: the
    /// operator knows where the repeater stands, the node only knows what its
    /// GPS told it, and half the nodes carry no GPS at all.
    pub latitude: Option<f64>,
    /// Longitude in degrees that applies, if any.
    pub longitude: Option<f64>,
    /// Where the position that applies comes from, or `None` without one.
    pub position_source: Option<PositionSource>,
    /// Latitude the node itself reported, even where a set one overrides it.
    ///
    /// Kept visible rather than hidden: a node whose GPS puts it in the wrong
    /// valley is worth noticing, and it cannot be noticed if the correction
    /// swallows the claim.
    pub reported_latitude: Option<f64>,
    /// Longitude the node itself reported.
    pub reported_longitude: Option<f64>,
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
                .route("/adverts", get(list_adverts))
                .route("/route-changes", get(list_route_changes))
                .route("/presence", get(node_presence))
                .route("/position", put(put_position).delete(delete_position)),
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
                        if let Ok(PushEvent::Advert(advert)) = PushEvent::parse(&payload)
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
    read_contacts(&context)
        .await
        .map(Json)
        .map_err(ListError::from)
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
            command::get_contacts(),
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
    let public_key = to_hex(&contact.public_key);
    let path = contact.path.as_ref().map(|route| to_hex(&route.hops));
    let stations = contact.path.as_ref().map(|route| i64::from(route.stations));

    // Read before write: afterwards the previous route is gone.
    record_route_change(context, &public_key, path.as_deref(), stations, &now).await?;

    // ON CONFLICT rather than REPLACE: replacing would reset first_seen, and
    // with it the answer to "since when do we know this node".
    sqlx::query(
        "INSERT INTO nodes_contacts
            (public_key, name, contact_type, flags, path, stations, latitude,
             longitude, last_advert, first_seen, last_seen)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (public_key) DO UPDATE SET
            name = excluded.name,
            contact_type = excluded.contact_type,
            flags = excluded.flags,
            path = excluded.path,
            stations = excluded.stations,
            latitude = excluded.latitude,
            longitude = excluded.longitude,
            last_advert = excluded.last_advert,
            last_seen = excluded.last_seen",
    )
    .bind(&public_key)
    .bind(&contact.name)
    .bind(i64::from(contact.contact_type))
    .bind(i64::from(contact.flags))
    .bind(&path)
    .bind(stations)
    .bind(contact.latitude.map(i64::from))
    .bind(contact.longitude.map(i64::from))
    .bind(i64::from(contact.last_advert))
    .bind(&now)
    .bind(&now)
    .execute(context.db.pool())
    .await?;

    announce_contact(context, contact);

    Ok(())
}

/// Writes down that the route to a contact changed, if it did.
///
/// Only for a contact that was already known: the first route to a node is
/// where its history starts, not a change within it. A node whose route is
/// unknown in both states has not moved either — `None` twice is not a step.
async fn record_route_change(
    context: &AppContext,
    public_key: &str,
    path: Option<&str>,
    stations: Option<i64>,
    at: &str,
) -> Result<(), sqlx::Error> {
    let known: Option<(Option<String>, Option<i64>)> =
        sqlx::query_as("SELECT path, stations FROM nodes_contacts WHERE public_key = ?")
            .bind(public_key)
            .fetch_optional(context.db.pool())
            .await?;

    let Some((previous_path, previous_stations)) = known else {
        return Ok(());
    };

    if previous_path.as_deref() == path && previous_stations == stations {
        return Ok(());
    }

    sqlx::query(
        "INSERT INTO nodes_route_changes
            (public_key, changed_at, path, stations, previous_path, previous_stations)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(public_key)
    .bind(at)
    .bind(path)
    .bind(stations)
    .bind(&previous_path)
    .bind(previous_stations)
    .execute(context.db.pool())
    .await?;

    Ok(())
}

/// One recorded change of the route to a node.
#[derive(Debug, Serialize, PartialEq)]
pub struct RouteChange {
    /// Running number, ascending with arrival. Cursor for the next page.
    pub id: i64,
    /// Whose route changed, lowercase hex.
    pub public_key: String,
    /// When MeshDash noticed.
    pub changed_at: DateTime<Utc>,
    /// The route from then on, hex hop bytes, or `None` for no route.
    pub path: Option<String>,
    /// How many stations it passes through.
    pub stations: Option<u8>,
    /// The route until then.
    pub previous_path: Option<String>,
    /// How many stations that one passed through.
    pub previous_stations: Option<u8>,
}

/// Which route changes to answer with.
#[derive(Debug, Deserialize, Default)]
pub struct RouteChangeQuery {
    /// Only changes for this public key, lowercase hex.
    node: Option<String>,
    /// Only changes older than this one.
    before: Option<i64>,
    /// How many to return.
    limit: Option<i64>,
    /// Which stretch of time to cover.
    #[serde(flatten)]
    range: TimeRange,
}

impl RouteChangeQuery {
    /// What the request asks for, or what is wrong with its time range.
    fn window(&self) -> Result<Window, BadTimeRange> {
        Ok(Window::new(
            self.limit
                .unwrap_or(ADVERT_LIMIT)
                .clamp(1, MAX_ADVERT_LIMIT),
            self.before,
            self.range.bounds()?,
        ))
    }
}

/// Answers with the recorded route changes, newest first.
async fn list_route_changes(
    State(context): State<AppContext>,
    Query(query): Query<RouteChangeQuery>,
) -> Result<Json<Vec<RouteChange>>, ListError> {
    read_route_changes(&context, query.node.as_deref(), &query.window()?)
        .await
        .map(Json)
        .map_err(ListError::from)
}

/// Reads the recorded route changes, newest first.
pub async fn read_route_changes(
    context: &AppContext,
    node: Option<&str>,
    window: &Window,
) -> Result<Vec<RouteChange>, sqlx::Error> {
    type Row = (
        i64,
        String,
        String,
        Option<String>,
        Option<i64>,
        Option<String>,
        Option<i64>,
    );

    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, public_key, changed_at, path, stations, previous_path, previous_stations
         FROM nodes_route_changes
         WHERE (?1 IS NULL OR public_key = ?1)
           AND (?2 IS NULL OR id < ?2)
           AND (?3 IS NULL OR changed_at >= ?3)
           AND (?4 IS NULL OR changed_at <= ?4)
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
            Some(RouteChange {
                id: row.0,
                public_key: row.1,
                changed_at: match DateTime::parse_from_rfc3339(&row.2) {
                    Ok(time) => time.with_timezone(&Utc),
                    Err(error) => {
                        tracing::error!(%error, "stored timestamp is not RFC 3339");
                        return None;
                    }
                },
                path: row.3,
                stations: row.4.map(|value| value as u8),
                previous_path: row.5,
                previous_stations: row.6.map(|value| value as u8),
            })
        })
        .collect())
}

/// How often a node was heard, cut into equal stretches of time.
///
/// A listing of sightings answers "when was it heard"; this answers "how
/// reachable has it been" — which is a different question as soon as the
/// stretch is longer than a screen holds.
#[derive(Debug, Serialize, PartialEq)]
pub struct Presence {
    /// Start of the whole stretch.
    pub from: DateTime<Utc>,
    /// End of the whole stretch.
    pub to: DateTime<Utc>,
    /// The equal stretches it is cut into, oldest first.
    pub buckets: Vec<PresenceBucket>,
}

/// One stretch of time and how often the node was heard within it.
#[derive(Debug, Serialize, PartialEq)]
pub struct PresenceBucket {
    /// Start of this stretch.
    pub from: DateTime<Utc>,
    /// End of this stretch.
    pub to: DateTime<Utc>,
    /// How many adverts arrived in it. Zero means silence, not missing data.
    pub sightings: i64,
}

/// How many stretches the presence is cut into unless asked otherwise.
const DEFAULT_BUCKETS: i64 = 48;

/// Most stretches a single request may ask for.
///
/// One pixel per stretch is the useful floor; beyond that the answer grows
/// without telling anyone more.
const MAX_BUCKETS: i64 = 500;

/// How far back the presence reaches when the request names no start.
const DEFAULT_PRESENCE_HOURS: i64 = 24;

/// Which node's reachability to answer with, over which stretch.
#[derive(Debug, Deserialize, Default)]
pub struct PresenceQuery {
    /// Whose reachability, lowercase hex. Required.
    node: Option<String>,
    /// How many stretches to cut the time into.
    buckets: Option<i64>,
    /// Which stretch of time to cover.
    #[serde(flatten)]
    range: TimeRange,
}

/// Answers how reachable one node has been.
async fn node_presence(
    State(context): State<AppContext>,
    Query(query): Query<PresenceQuery>,
) -> Result<Json<Presence>, ListError> {
    let Some(node) = query.node.as_deref() else {
        return Err(ListError::MissingNode);
    };

    let times = query.range.times()?;
    let to = times.until.unwrap_or_else(Utc::now);
    let from = match times.since {
        Some(time) => time,
        // Without a start, the band covers everything known about this node —
        // that is what "alles" in the interface promises. A fixed window here
        // would answer a different question than the one that was asked.
        None => first_sighting(&context, node)
            .await
            .map_err(ListError::from)?
            .unwrap_or_else(|| to - chrono::Duration::hours(DEFAULT_PRESENCE_HOURS)),
    };

    if from >= to {
        return Err(ListError::BackwardsRange);
    }

    let buckets = query
        .buckets
        .unwrap_or(DEFAULT_BUCKETS)
        .clamp(1, MAX_BUCKETS);

    read_presence(&context, node, from, to, buckets)
        .await
        .map(Json)
        .map_err(ListError::from)
}

/// When this node was first heard, if it ever was.
async fn first_sighting(
    context: &AppContext,
    node: &str,
) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
    let earliest: Option<(String,)> =
        sqlx::query_as("SELECT MIN(heard_at) FROM nodes_adverts WHERE public_key = ?")
            .bind(node)
            .fetch_optional(context.db.pool())
            .await?;

    Ok(earliest.and_then(|(text,)| {
        DateTime::parse_from_rfc3339(&text)
            .ok()
            .map(|time| time.with_timezone(&Utc))
    }))
}

/// Counts sightings per equal stretch of time, oldest first.
pub async fn read_presence(
    context: &AppContext,
    node: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    buckets: i64,
) -> Result<Presence, sqlx::Error> {
    let start = from.timestamp();
    // Rounded up so the last stretch reaches the end instead of stopping just
    // short of it.
    let width = ((to.timestamp() - start) as f64 / buckets as f64).ceil() as i64;
    let width = width.max(1);

    // Counted in SQL rather than by reading every row: a month of adverts from
    // a busy node is tens of thousands of rows, and all the interface draws
    // from them is one number per stretch.
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT (CAST(strftime('%s', heard_at) AS INTEGER) - ?2) / ?3 AS bucket, COUNT(*)
         FROM nodes_adverts
         WHERE public_key = ?1
           AND heard_at >= ?4
           AND heard_at <= ?5
         GROUP BY bucket",
    )
    .bind(node)
    .bind(start)
    .bind(width)
    .bind(from.to_rfc3339())
    .bind(to.to_rfc3339())
    .fetch_all(context.db.pool())
    .await?;

    let counted: std::collections::HashMap<i64, i64> = rows.into_iter().collect();

    Ok(Presence {
        from,
        to,
        buckets: (0..buckets)
            .map(|index| PresenceBucket {
                from: from + chrono::Duration::seconds(index * width),
                to: from + chrono::Duration::seconds((index + 1) * width),
                // A stretch nobody was heard in is a zero. Leaving it out
                // would make silence look like a gap in the recording.
                sightings: counted.get(&index).copied().unwrap_or(0),
            })
            .collect(),
    })
}

/// Tells whoever is listening that this contact exists, under this name.
///
/// Fire and forget: nobody may be listening, and this module must not care.
/// `messages` uses it to put a name on a six-byte sender prefix — it cannot
/// read this module's tables, and this module has no business knowing that
/// messages exist. See `docs/decisions/0007-modul-ereignisse.md`.
fn announce_contact(context: &AppContext, contact: &Contact) {
    context.events.publish(AppEvent::Module {
        module: "nodes".into(),
        kind: "contact".into(),
        data: serde_json::json!({
            "public_key": to_hex(&contact.public_key),
            "name": contact.name,
        }),
    });
}

/// One sighting, as the API reports it.
#[derive(Debug, Serialize, PartialEq)]
pub struct Sighting {
    /// Running number, ascending with arrival.
    ///
    /// Doubles as the cursor for paging: ask for `before=<id>` to get what
    /// came before this one.
    pub id: i64,
    /// Public key that was heard, lowercase hex.
    pub public_key: String,
    /// When MeshDash received the advert.
    pub heard_at: DateTime<Utc>,
    /// Whether the node had not known this contact before.
    pub was_new: bool,
}

/// Which sightings to answer with.
#[derive(Debug, Deserialize, Default)]
pub struct SightingQuery {
    /// Only sightings of this public key, lowercase hex.
    node: Option<String>,
    /// Only sightings older than this one — the id of the last one seen.
    before: Option<i64>,
    /// How many to return.
    limit: Option<i64>,
    /// Which stretch of time to cover.
    #[serde(flatten)]
    range: TimeRange,
}

impl SightingQuery {
    /// What the request asks for, or what is wrong with its time range.
    fn window(&self) -> Result<Window, BadTimeRange> {
        Ok(Window::new(
            self.limit
                .unwrap_or(ADVERT_LIMIT)
                .clamp(1, MAX_ADVERT_LIMIT),
            self.before,
            self.range.bounds()?,
        ))
    }
}

/// Answers with the most recent sightings.
async fn list_adverts(
    State(context): State<AppContext>,
    Query(query): Query<SightingQuery>,
) -> Result<Json<Vec<Sighting>>, ListError> {
    read_adverts(&context, query.node.as_deref(), &query.window()?)
        .await
        .map(Json)
        .map_err(ListError::from)
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
///
/// With a `node` given, only that node's — which is what a page about one
/// node needs, and what keeps it from fetching two hundred rows to show five.
pub async fn read_adverts(
    context: &AppContext,
    node: Option<&str>,
    window: &Window,
) -> Result<Vec<Sighting>, sqlx::Error> {
    // Paged by id rather than by timestamp: two adverts can share a
    // timestamp, and a cursor that is not unique either repeats a row or
    // skips one.
    let rows: Vec<(i64, String, String, i64)> = sqlx::query_as(
        "SELECT id, public_key, heard_at, was_new FROM nodes_adverts
         WHERE (?1 IS NULL OR public_key = ?1)
           AND (?2 IS NULL OR id < ?2)
           AND (?3 IS NULL OR heard_at >= ?3)
           AND (?4 IS NULL OR heard_at <= ?4)
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
            Some(Sighting {
                id: row.0,
                public_key: row.1,
                heard_at: parse_time(&row.2)?,
                was_new: row.3 != 0,
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
    Option<String>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
    i64,
    String,
    String,
    Option<f64>,
    Option<f64>,
);

/// Where a node's position comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PositionSource {
    /// The node advertised these coordinates itself.
    Reported,
    /// The operator wrote them down.
    Manual,
}

/// Why a position could not be stored.
#[derive(Debug)]
pub enum PositionError {
    /// Latitude or longitude lies outside the globe.
    OutOfRange {
        /// Which of the two.
        field: &'static str,
        /// The value as it arrived.
        value: f64,
    },
    /// The key is not 64 hex characters.
    BadKey,
    /// The database could not be written.
    Storage(sqlx::Error),
}

impl From<sqlx::Error> for PositionError {
    fn from(error: sqlx::Error) -> Self {
        Self::Storage(error)
    }
}

impl axum::response::IntoResponse for PositionError {
    fn into_response(self) -> axum::response::Response {
        let (code, message) = match self {
            Self::OutOfRange { field, value } => (
                "invalid_parameter",
                format!("{field} is outside the globe: {value}"),
            ),
            Self::BadKey => (
                "invalid_parameter",
                "public_key must be 64 hex characters".to_owned(),
            ),
            Self::Storage(error) => {
                tracing::error!(%error, "could not store the position");
                return (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": { "code": "storage_failed", "message": "could not store the position" }
                    })),
                )
                    .into_response();
            }
        };

        (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": { "code": code, "message": message } })),
        )
            .into_response()
    }
}

/// What a request sends to place a node.
#[derive(Debug, Deserialize)]
pub struct PositionRequest {
    /// Whose position, lowercase hex.
    public_key: String,
    /// Latitude in degrees.
    latitude: f64,
    /// Longitude in degrees.
    longitude: f64,
}

/// What a request sends to take a position back.
#[derive(Debug, Deserialize)]
pub struct PositionKey {
    /// Whose position, lowercase hex.
    public_key: String,
}

async fn put_position(
    State(context): State<AppContext>,
    Json(request): Json<PositionRequest>,
) -> Result<axum::http::StatusCode, PositionError> {
    set_position(
        &context,
        &request.public_key,
        request.latitude,
        request.longitude,
    )
    .await?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

async fn delete_position(
    State(context): State<AppContext>,
    Json(request): Json<PositionKey>,
) -> Result<axum::http::StatusCode, PositionError> {
    clear_position(&context, &request.public_key).await?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// Writes down where a node stands, whatever the node says about itself.
pub async fn set_position(
    context: &AppContext,
    public_key: &str,
    latitude: f64,
    longitude: f64,
) -> Result<(), PositionError> {
    if !is_key(public_key) {
        return Err(PositionError::BadKey);
    }

    // Refused rather than clamped: a latitude of 91 is a typo or a unit
    // mix-up, and clamping it to 90 would put a node at the pole and call it
    // an answer.
    if !(-90.0..=90.0).contains(&latitude) {
        return Err(PositionError::OutOfRange {
            field: "latitude",
            value: latitude,
        });
    }
    if !(-180.0..=180.0).contains(&longitude) {
        return Err(PositionError::OutOfRange {
            field: "longitude",
            value: longitude,
        });
    }

    sqlx::query(
        "INSERT INTO nodes_manual_positions (public_key, latitude, longitude, set_at)
         VALUES (?, ?, ?, ?)
         ON CONFLICT (public_key) DO UPDATE SET
            latitude = excluded.latitude,
            longitude = excluded.longitude,
            set_at = excluded.set_at",
    )
    .bind(public_key)
    .bind(latitude)
    .bind(longitude)
    .bind(Utc::now().to_rfc3339())
    .execute(context.db.pool())
    .await?;

    Ok(())
}

/// Takes a set position back, leaving whatever the node reports.
pub async fn clear_position(context: &AppContext, public_key: &str) -> Result<(), PositionError> {
    if !is_key(public_key) {
        return Err(PositionError::BadKey);
    }

    sqlx::query("DELETE FROM nodes_manual_positions WHERE public_key = ?")
        .bind(public_key)
        .execute(context.db.pool())
        .await?;

    Ok(())
}

/// Whether this is a full public key in the form the API uses.
fn is_key(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Reads every known contact, most recently seen first.
pub async fn read_contacts(context: &AppContext) -> Result<Vec<KnownContact>, sqlx::Error> {
    let rows: Vec<ContactRow> = sqlx::query_as(
        // LEFT JOIN within this module's own tables — the rule forbids
        // reaching into another module's, not joining one's own.
        "SELECT c.public_key, c.name, c.contact_type, c.flags, c.path, c.stations,
                c.latitude, c.longitude, c.last_advert, c.first_seen, c.last_seen,
                m.latitude, m.longitude
         FROM nodes_contacts c
         LEFT JOIN nodes_manual_positions m ON m.public_key = c.public_key
         ORDER BY c.last_seen DESC, c.public_key",
    )
    .fetch_all(context.db.pool())
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            // Microdegrees from the node, plain degrees from the operator:
            // the node's own field is an i32 of microdegrees, and a set
            // position never went through it.
            let reported_latitude = row.6.map(|value| value as f64 / 1e6);
            let reported_longitude = row.7.map(|value| value as f64 / 1e6);
            let (latitude, longitude, position_source) = match (row.11, row.12) {
                (Some(latitude), Some(longitude)) => (
                    Some(latitude),
                    Some(longitude),
                    Some(PositionSource::Manual),
                ),
                _ => match (reported_latitude, reported_longitude) {
                    (Some(latitude), Some(longitude)) => (
                        Some(latitude),
                        Some(longitude),
                        Some(PositionSource::Reported),
                    ),
                    _ => (None, None, None),
                },
            };

            Some(KnownContact {
                public_key: row.0,
                name: row.1,
                contact_type: row.2 as u8,
                flags: row.3 as u8,
                path: row.4,
                stations: row.5.map(|value| value as u8),
                latitude,
                longitude,
                position_source,
                reported_latitude,
                reported_longitude,
                last_advert: row.8 as u32,
                first_seen: parse_time(&row.9)?,
                last_seen: parse_time(&row.10)?,
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
pub enum ListError {
    /// The database could not be read.
    Storage(sqlx::Error),
    /// The request asked for a time range that is not a time.
    BadRange(BadTimeRange),
    /// The request asked about a node without saying which.
    MissingNode,
    /// The request asked for a stretch that ends before it starts.
    BackwardsRange,
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
                tracing::error!(%error, "could not read the contacts");

                (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": { "code": "storage_failed", "message": "could not read the contacts" }
                    })),
                )
                    .into_response()
            }
            Self::BadRange(bad) => bad.into_response(),
            Self::MissingNode => (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": {
                        "code": "invalid_parameter",
                        "message": "node is required: reachability is about one node"
                    }
                })),
            )
                .into_response(),
            // Both timestamps are readable, so saying one of them is not a
            // timestamp would send the caller looking in the wrong place.
            Self::BackwardsRange => (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": {
                        "code": "invalid_parameter",
                        "message": "until must lie after since"
                    }
                })),
            )
                .into_response(),
        }
    }
}

#[cfg(test)]
mod tests;
