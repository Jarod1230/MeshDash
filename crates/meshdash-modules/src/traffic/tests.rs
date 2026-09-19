//! Tests for the traffic module.
//!
//! Against a real in-memory database with the real schema and the real push
//! decoding — the point is that a frame off the wire ends up as the right rows,
//! and a stand-in for either end would not show that.

use meshdash_core::{
    config::ModuleSettings, db::Database, event::EventBus, link, module::ModuleRegistry,
    settings::Settings,
};
use meshdash_transport::mock::{MockTransport, Step};

use super::*;

/// A context with the traffic module migrated and running.
async fn context_with(settings: serde_json::Value) -> AppContext {
    let db = Database::open_in_memory().await.unwrap();
    let events = EventBus::new();
    let (handle, _task) = link::spawn(
        MockTransport::new(vec![Step::Drop("no node needed here".into())]),
        link::LinkConfig::default(),
        events.clone(),
    );

    let mut module_settings = ModuleSettings::default();
    module_settings.set("traffic", settings);

    let context = AppContext {
        db,
        events,
        link: handle,
        settings: Settings::from_file(module_settings),
    };

    let mut registry = ModuleRegistry::new();
    registry.register(Box::new(TrafficModule)).unwrap();
    registry.start_all(&context).await.unwrap();

    context
}

/// A raw packet: flood-routed text, one byte per station.
fn packet_over(stations: &[u8]) -> Vec<u8> {
    let mut raw = vec![
        // route 1 (flood), payload type 2 (text), version 0
        0b0000_1001,
        // width 1, and this many stations
        stations.len() as u8,
    ];
    raw.extend_from_slice(stations);
    // The encrypted remainder, which is none of MeshDash's business.
    raw.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);

    raw
}

/// That packet wrapped in the push the node sends for everything it hears.
fn heard(stations: &[u8]) -> Vec<u8> {
    heard_with_header(0b0000_1001, stations)
}

/// A heard packet with a chosen header byte: route in bits 0–1, payload type
/// in bits 2–5 (`src/Packet.h`).
fn heard_with_header(header: u8, stations: &[u8]) -> Vec<u8> {
    let mut raw = packet_over(stations);
    raw[0] = header;

    let mut frame = vec![
        0x88,
        // SNR in quarter-decibels, then RSSI.
        (-3.5_f32 * 4.0) as i8 as u8,
        -92_i8 as u8,
    ];
    frame.extend_from_slice(&raw);

    frame
}

/// Route 2 (direct), payload type 2 (text).
const DIRECT_TEXT: u8 = 0b0000_1010;
/// Route 2 (direct), payload type 9 (trace).
const DIRECT_TRACE: u8 = 0b0010_0110;

/// Feeds one push through the bus and waits for the module to have written.
async fn feed(context: &AppContext, frame: Vec<u8>) {
    context.events.publish(AppEvent::Push { payload: frame });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
}

fn everything() -> Window {
    Window::paged(500, None)
}

#[tokio::test]
async fn writes_down_what_it_heard() {
    let context = context_with(serde_json::json!({})).await;

    feed(&context, heard(&[0xAA, 0xBB])).await;

    let log = read_packets(&context, None, &everything()).await.unwrap();
    assert_eq!(log.len(), 1);
    let entry = &log[0];
    assert_eq!(entry.route_type, 1, "flood");
    assert_eq!(entry.payload_type, 2, "text message");
    assert_eq!(entry.stations, 2);
    assert_eq!(entry.path, "aabb");
    assert_eq!(entry.path_width, 1);
    assert_eq!(entry.snr, Some(-3.5));
    assert_eq!(entry.rssi, Some(-92));
}

#[tokio::test]
async fn never_keeps_the_payload() {
    // It is encrypted and not ours. Not stored at all, so it cannot leak —
    // and the answer cannot carry it either.
    let context = context_with(serde_json::json!({})).await;

    feed(&context, heard(&[0xAA])).await;

    let log = read_packets(&context, None, &everything()).await.unwrap();
    let answer = serde_json::to_string(&log).unwrap();
    assert!(!answer.contains("dead"), "{answer}");
    assert!(!answer.contains("beef"), "{answer}");
}

#[tokio::test]
async fn reads_the_chain_of_who_heard_whom() {
    let context = context_with(serde_json::json!({})).await;

    feed(&context, heard(&[0xAA, 0xBB])).await;

    let links = read_links(&context).await.unwrap();
    // Two statements: bb heard aa, and this node heard bb.
    assert_eq!(links.len(), 2);
    assert!(
        links
            .iter()
            .any(|link| link.talker == "aa" && link.listener == "bb")
    );
    // This node has no prefix in a path it received, so it is the empty one.
    assert!(
        links
            .iter()
            .any(|link| link.talker == "bb" && link.listener.is_empty())
    );
    assert!(links.iter().all(|link| link.width == 1));
}

#[tokio::test]
async fn a_packet_with_no_station_proves_nothing_about_a_pair() {
    // It was heard from its sender directly — but the sender is named only
    // inside the payload, which is encrypted. Nobody can be written down.
    let context = context_with(serde_json::json!({})).await;

    feed(&context, heard(&[])).await;

    assert_eq!(read_links(&context).await.unwrap(), vec![]);
    // The packet itself is still worth keeping.
    assert_eq!(
        read_packets(&context, None, &everything())
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn counts_a_pair_rather_than_repeating_it() {
    let context = context_with(serde_json::json!({})).await;

    feed(&context, heard(&[0xAA, 0xBB])).await;
    feed(&context, heard(&[0xAA, 0xBB])).await;

    let links = read_links(&context).await.unwrap();
    // Still two pairs, each seen twice — this table grows with the mesh, not
    // with the traffic. That is the whole point of it, see ADR-0016.
    assert_eq!(links.len(), 2);
    assert!(links.iter().all(|link| link.heard == 2));
    assert!(links.iter().all(|link| link.last_seen >= link.first_seen));
}

#[tokio::test]
async fn keeps_the_summary_when_the_log_is_switched_off() {
    let context = context_with(serde_json::json!({ "record": false })).await;

    feed(&context, heard(&[0xAA, 0xBB])).await;

    assert_eq!(
        read_packets(&context, None, &everything()).await.unwrap(),
        vec![]
    );
    assert_eq!(read_links(&context).await.unwrap().len(), 2);
}

#[tokio::test]
async fn sweeps_what_is_past_the_deadline_and_nothing_else() {
    let context = context_with(serde_json::json!({})).await;
    feed(&context, heard(&[0xAA])).await;

    // One row from long ago, written past the module so the age is the test.
    sqlx::query(
        "INSERT INTO traffic_packets
            (heard_at, route_type, payload_type, version, stations, path, path_width, size)
         VALUES (?, 1, 2, 0, 0, '', 1, 7)",
    )
    .bind((Utc::now() - chrono::Duration::days(90)).to_rfc3339())
    .execute(context.db.pool())
    .await
    .unwrap();

    assert_eq!(sweep(&context, 30).await.unwrap(), 1);
    assert_eq!(
        read_packets(&context, None, &everything())
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn a_deadline_of_zero_does_not_mean_delete_everything() {
    // Nobody writes keep_days = 0 meaning "empty the table every hour".
    let context = context_with(serde_json::json!({})).await;
    feed(&context, heard(&[0xAA])).await;

    assert_eq!(sweep(&context, 0).await.unwrap(), 0);
    assert_eq!(sweep(&context, -5).await.unwrap(), 0);
    assert_eq!(
        read_packets(&context, None, &everything())
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn a_packet_it_cannot_read_is_not_a_reason_to_stop() {
    let context = context_with(serde_json::json!({})).await;

    // A frame that ends inside the path field.
    feed(
        &context,
        vec![0x88, 0x00, 0x00, 0b0000_1001, 0b0000_0010, 0xAA],
    )
    .await;
    feed(&context, heard(&[0xCC])).await;

    // The good one after it still landed.
    assert_eq!(
        read_packets(&context, None, &everything())
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn puts_every_packet_it_can_read_on_the_bus() {
    let context = context_with(serde_json::json!({})).await;
    let mut watching = context.events.subscribe();

    feed(&context, heard(&[0xAA, 0xBB])).await;

    // The push itself comes first; the decoded packet follows it.
    let announced = loop {
        match watching.recv().await.unwrap() {
            AppEvent::Module { module, kind, data } => {
                assert_eq!(module, "traffic");
                assert_eq!(kind, "packet");
                break data;
            }
            _ => continue,
        }
    };

    assert_eq!(announced["route_type"], 1);
    assert_eq!(announced["payload_type"], 2);
    assert_eq!(announced["stations"], serde_json::json!(["aa", "bb"]));
    assert_eq!(announced["width"], 1);
    assert_eq!(announced["rssi"], -92);
    // The encrypted remainder is not in there, as it is not anywhere else.
    assert!(!announced.to_string().contains("dead"));
}

#[tokio::test]
async fn still_announces_when_the_log_is_switched_off() {
    // Watching what happens now and keeping a history are two wishes. Turning
    // off the second must not turn off the first.
    let context = context_with(serde_json::json!({ "record": false })).await;
    let mut watching = context.events.subscribe();

    feed(&context, heard(&[0xCC])).await;

    let announced = loop {
        match watching.recv().await.unwrap() {
            AppEvent::Module { data, .. } => break data,
            _ => continue,
        }
    };

    assert_eq!(announced["stations"], serde_json::json!(["cc"]));
}

#[tokio::test]
async fn finds_every_packet_a_node_touched() {
    let context = context_with(serde_json::json!({})).await;

    feed(&context, heard(&[0xAA, 0xBB])).await;
    feed(&context, heard(&[0xCC])).await;

    let key = format!("bb{}", "11".repeat(31));
    let touched = read_packets(&context, Some(&key), &everything())
        .await
        .unwrap();

    assert_eq!(touched.len(), 1);
    assert_eq!(touched[0].path, "aabb");
}

#[tokio::test]
async fn matches_at_station_boundaries_and_not_across_them() {
    // 'aabb' contains 'ab', which is no station at all. A substring search
    // over concatenated prefixes would find it; matching per station does not.
    let context = context_with(serde_json::json!({})).await;

    feed(&context, heard(&[0xAA, 0xBB])).await;

    let across = format!("ab{}", "11".repeat(31));
    assert_eq!(
        read_packets(&context, Some(&across), &everything())
            .await
            .unwrap(),
        vec![]
    );
}

#[tokio::test]
async fn a_packet_with_no_station_belongs_to_nobody() {
    let context = context_with(serde_json::json!({})).await;

    feed(&context, heard(&[])).await;

    let anyone = "aa".repeat(32);
    assert_eq!(
        read_packets(&context, Some(&anyone), &everything())
            .await
            .unwrap(),
        vec![]
    );
}

#[tokio::test]
async fn the_sweep_takes_the_stations_with_the_packets() {
    // Otherwise the station rows outlive their packets and the table grows
    // without bound behind the retention period.
    let context = context_with(serde_json::json!({})).await;
    feed(&context, heard(&[0xAA])).await;

    sqlx::query("UPDATE traffic_packets SET heard_at = ?")
        .bind((Utc::now() - chrono::Duration::days(90)).to_rfc3339())
        .execute(context.db.pool())
        .await
        .unwrap();

    sweep(&context, 30).await.unwrap();

    let left: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM traffic_packet_stations")
        .fetch_one(context.db.pool())
        .await
        .unwrap();
    assert_eq!(left.0, 0);
}

/// Inserts one flooded packet at a chosen time with its stations split out.
async fn insert_packet_at(
    context: &AppContext,
    heard_at: DateTime<Utc>,
    stations: &[&str],
    width: u8,
) {
    insert_routed_packet_at(context, heard_at, 1, 2, stations, width).await;
}

/// Inserts one packet with a chosen route and payload type.
async fn insert_routed_packet_at(
    context: &AppContext,
    heard_at: DateTime<Utc>,
    route_type: u8,
    payload_type: u8,
    stations: &[&str],
    width: u8,
) {
    let path: String = stations.iter().copied().collect();
    let id = sqlx::query(
        "INSERT INTO traffic_packets
            (heard_at, route_type, payload_type, version, stations, path, path_width, size)
         VALUES (?, ?, ?, 0, ?, ?, ?, 7)",
    )
    .bind(heard_at.to_rfc3339())
    .bind(i64::from(route_type))
    .bind(i64::from(payload_type))
    .bind(stations.len() as i64)
    .bind(&path)
    .bind(i64::from(width))
    .execute(context.db.pool())
    .await
    .unwrap()
    .last_insert_rowid();

    for (position, prefix) in stations.iter().enumerate() {
        sqlx::query(
            "INSERT INTO traffic_packet_stations (packet_id, position, prefix) VALUES (?, ?, ?)",
        )
        .bind(id)
        .bind(position as i64)
        .bind(prefix)
        .execute(context.db.pool())
        .await
        .unwrap();
    }
}

fn moment(text: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(text)
        .unwrap()
        .with_timezone(&Utc)
}

#[test]
fn path_pairs_match_what_record_hearing_writes() {
    // aa → bb → (this node): bb heard aa; this node heard bb.
    let stations = vec!["aa".into(), "bb".into()];
    assert_eq!(
        pairs_from_path(&stations, 1),
        vec![
            ("aa".into(), "bb".into(), 1),
            ("bb".into(), String::new(), 1),
        ]
    );
}

#[test]
fn an_empty_path_proves_no_pair() {
    assert_eq!(pairs_from_path(&[], 1), vec![]);
}

#[test]
fn a_single_station_is_only_heard_by_this_node() {
    assert_eq!(
        pairs_from_path(&["cc".into()], 2),
        vec![("cc".into(), String::new(), 2)]
    );
}

#[test]
fn clamp_leaves_a_fitting_window_alone() {
    let now = moment("2026-09-06T12:00:00Z");
    let since = moment("2026-09-01T12:00:00Z");
    let until = moment("2026-09-06T12:00:00Z");

    let effective = clamp_range(Some(since), Some(until), 30, now);
    assert_eq!(effective.since, since);
    assert_eq!(effective.until, until);
    assert!(!effective.clamped);
    assert_eq!(effective.keep_days, 30);
}

#[test]
fn clamp_pulls_since_up_to_retention() {
    let now = moment("2026-09-06T12:00:00Z");
    let since = moment("2026-07-01T12:00:00Z");
    let until = now;

    let effective = clamp_range(Some(since), Some(until), 30, now);
    assert_eq!(effective.since, moment("2026-08-07T12:00:00Z"));
    assert_eq!(effective.until, until);
    assert!(effective.clamped);
}

#[test]
fn clamp_shortens_a_span_longer_than_keep_days() {
    // Retention alone cannot shorten this: since sits on the retention floor,
    // until lies past "now", so the span is 40 days while keep_days is 30.
    // until stays; since moves forward.
    let now = moment("2026-09-06T12:00:00Z");
    let since = moment("2026-08-07T12:00:00Z");
    let until = moment("2026-09-16T12:00:00Z");

    let effective = clamp_range(Some(since), Some(until), 30, now);
    assert_eq!(effective.since, moment("2026-08-17T12:00:00Z"));
    assert_eq!(effective.until, until);
    assert!(effective.clamped);
}

#[test]
fn clamp_defaults_until_to_now_and_since_to_retention() {
    let now = moment("2026-09-06T12:00:00Z");
    let effective = clamp_range(None, None, 30, now);
    // Both missing still goes through clamp when the handler asks — the
    // handler itself only calls clamp when at least one end was set. Here we
    // check the defaults themselves.
    assert_eq!(effective.until, now);
    assert_eq!(effective.since, moment("2026-08-07T12:00:00Z"));
    assert!(!effective.clamped);
}

#[test]
fn clamp_does_not_invent_retention_when_keep_days_is_disabled() {
    let now = moment("2026-09-06T12:00:00Z");
    let since = moment("2020-01-01T00:00:00Z");
    let effective = clamp_range(Some(since), Some(now), 0, now);
    assert_eq!(effective.since, since);
    assert!(!effective.clamped);
}

#[tokio::test]
async fn timed_links_count_only_packets_inside_the_window() {
    let context = context_with(serde_json::json!({ "keep_days": 30 })).await;

    // Outside the window — must not count.
    insert_packet_at(&context, moment("2026-08-01T12:00:00Z"), &["aa", "bb"], 1).await;
    // Inside.
    insert_packet_at(&context, moment("2026-09-01T12:00:00Z"), &["aa", "bb"], 1).await;
    insert_packet_at(&context, moment("2026-09-02T12:00:00Z"), &["aa", "bb"], 1).await;
    // Different pair, also inside.
    insert_packet_at(&context, moment("2026-09-03T12:00:00Z"), &["cc"], 1).await;

    let links = read_links_in_window(
        &context,
        &moment("2026-09-01T00:00:00Z"),
        &moment("2026-09-05T00:00:00Z"),
    )
    .await
    .unwrap();

    assert_eq!(links.len(), 3);
    let aa_bb = links
        .iter()
        .find(|link| link.talker == "aa" && link.listener == "bb")
        .unwrap();
    assert_eq!(aa_bb.heard, 2);
    assert_eq!(aa_bb.first_seen, moment("2026-09-01T12:00:00Z"));
    assert_eq!(aa_bb.last_seen, moment("2026-09-02T12:00:00Z"));

    let bb_here = links
        .iter()
        .find(|link| link.talker == "bb" && link.listener.is_empty())
        .unwrap();
    assert_eq!(bb_here.heard, 2);

    let cc_here = links
        .iter()
        .find(|link| link.talker == "cc" && link.listener.is_empty())
        .unwrap();
    assert_eq!(cc_here.heard, 1);
}

#[tokio::test]
async fn timed_links_ignore_the_timeless_summary() {
    // A pair only in traffic_links (no packet in the window) must not appear.
    let context = context_with(serde_json::json!({})).await;

    sqlx::query(
        "INSERT INTO traffic_links (talker, listener, width, first_seen, last_seen, heard)
         VALUES ('ee', 'ff', 1, ?, ?, 99)",
    )
    .bind(moment("2026-09-01T12:00:00Z").to_rfc3339())
    .bind(moment("2026-09-05T12:00:00Z").to_rfc3339())
    .execute(context.db.pool())
    .await
    .unwrap();

    insert_packet_at(&context, moment("2026-09-02T12:00:00Z"), &["aa"], 1).await;

    let links = read_links_in_window(
        &context,
        &moment("2026-09-01T00:00:00Z"),
        &moment("2026-09-05T00:00:00Z"),
    )
    .await
    .unwrap();

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].talker, "aa");
    assert!(links[0].listener.is_empty());
    assert_eq!(links[0].heard, 1);
}

#[tokio::test]
async fn timeless_links_stay_a_bare_array_shape() {
    // The map loads `/traffic/links` as HeardBy[]. Timed answers wrap; the
    // default must not suddenly become an object.
    let context = context_with(serde_json::json!({})).await;
    feed(&context, heard(&[0xAA])).await;

    let body = serde_json::to_value(read_links(&context).await.unwrap()).unwrap();
    assert!(body.is_array(), "{body}");
}

#[test]
fn timed_links_json_names_the_clamp() {
    let body = TimedLinks {
        clamped: true,
        keep_days: 30,
        effective_since: moment("2026-08-07T12:00:00Z"),
        effective_until: moment("2026-09-06T12:00:00Z"),
        links: vec![],
    };
    let json = serde_json::to_value(&body).unwrap();
    assert_eq!(json["clamped"], true);
    assert_eq!(json["keep_days"], 30);
    assert!(json["links"].is_array());
    assert!(
        json["effective_since"]
            .as_str()
            .unwrap()
            .starts_with("2026-08-07")
    );
}

#[tokio::test]
async fn a_direct_path_proves_no_pair() {
    // The path of a direct packet is the route still ahead of it. Whoever
    // sent what this node heard has already removed itself from it.
    let context = context_with(serde_json::json!({})).await;

    feed(&context, heard_with_header(DIRECT_TEXT, &[0xAA, 0xBB])).await;

    assert!(read_links(&context).await.unwrap().is_empty());
    // Still heard, still logged — only no statement about who heard whom.
    let log = read_packets(&context, None, &everything()).await.unwrap();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].route_type, 2);
}

#[tokio::test]
async fn a_trace_path_is_not_a_list_of_stations() {
    // SNR values in the path, not prefixes. 0x28 is +10 dB — and it must not
    // make the node with prefix 28 look like it touched this packet.
    let context = context_with(serde_json::json!({})).await;

    feed(&context, heard_with_header(DIRECT_TRACE, &[0x28, 0x1C])).await;

    assert!(read_links(&context).await.unwrap().is_empty());
    assert!(
        read_packets(&context, Some("28"), &everything())
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        read_packets(&context, None, &everything())
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn timed_links_leave_direct_packets_out() {
    let context = context_with(serde_json::json!({ "keep_days": 30 })).await;

    insert_packet_at(&context, moment("2026-09-01T12:00:00Z"), &["aa"], 1).await;
    insert_routed_packet_at(
        &context,
        moment("2026-09-01T13:00:00Z"),
        2,
        2,
        &["aa", "bb"],
        1,
    )
    .await;
    insert_routed_packet_at(&context, moment("2026-09-01T14:00:00Z"), 3, 2, &["cc"], 1).await;

    let links = read_links_in_window(
        &context,
        &moment("2026-09-01T00:00:00Z"),
        &moment("2026-09-02T00:00:00Z"),
    )
    .await
    .unwrap();

    assert_eq!(links.len(), 1, "{links:?}");
    assert_eq!(links[0].talker, "aa");
    assert!(links[0].listener.is_empty());
    assert_eq!(links[0].heard, 1);
}

/// Writes a summary row the way the module did before migration 3.
async fn insert_link(context: &AppContext, talker: &str, listener: &str, heard: i64) {
    sqlx::query(
        "INSERT INTO traffic_links (talker, listener, width, first_seen, last_seen, heard)
         VALUES (?, ?, 1, '2026-09-01T00:00:00Z', '2026-09-01T00:00:00Z', ?)",
    )
    .bind(talker)
    .bind(listener)
    .bind(heard)
    .execute(context.db.pool())
    .await
    .unwrap();
}

async fn heard_count(context: &AppContext, talker: &str, listener: &str) -> Option<i64> {
    sqlx::query_scalar(
        "SELECT heard FROM traffic_links WHERE talker = ? AND listener = ? AND width = 1",
    )
    .bind(talker)
    .bind(listener)
    .fetch_optional(context.db.pool())
    .await
    .unwrap()
}

#[tokio::test]
async fn migration_3_takes_back_what_direct_packets_added() {
    let context = context_with(serde_json::json!({ "keep_days": 30 })).await;

    // What the old code wrote for: one flood packet over aa, bb, and two
    // direct packets over the same stations.
    insert_packet_at(&context, moment("2026-09-01T12:00:00Z"), &["aa", "bb"], 1).await;
    for hour in ["13", "14"] {
        let at = moment(&format!("2026-09-01T{hour}:00:00Z"));
        insert_routed_packet_at(&context, at, 2, 2, &["aa", "bb"], 1).await;
    }
    // A trace: its "stations" are SNR bytes.
    insert_routed_packet_at(&context, moment("2026-09-01T15:00:00Z"), 2, 9, &["28"], 1).await;
    insert_link(&context, "aa", "bb", 3).await;
    insert_link(&context, "bb", "", 3).await;
    insert_link(&context, "28", "", 1).await;
    // Heard before the retention period; nothing in the log to take back.
    insert_link(&context, "dd", "", 5).await;

    sqlx::raw_sql(FORGET_UNPROVEN_HEARINGS)
        .execute(context.db.pool())
        .await
        .unwrap();

    assert_eq!(heard_count(&context, "aa", "bb").await, Some(1));
    assert_eq!(heard_count(&context, "bb", "").await, Some(1));
    assert_eq!(
        heard_count(&context, "28", "").await,
        None,
        "down to zero is gone"
    );
    assert_eq!(heard_count(&context, "dd", "").await, Some(5));
    assert!(
        read_packets(&context, Some("28"), &everything())
            .await
            .unwrap()
            .is_empty()
    );
}

/// A heard advert from `sender`, with a chosen route and path.
///
/// The payload opens with the sender's key, as `Mesh::createAdvert` writes
/// it; what follows stands in for timestamp, signature and app data.
fn heard_advert(route: u8, stations: &[u8], sender: [u8; 32]) -> Vec<u8> {
    // payload type 4 (advert) in bits 2–5, the route in bits 0–1
    let mut raw = vec![(4 << 2) | route, stations.len() as u8];
    raw.extend_from_slice(stations);
    raw.extend_from_slice(&sender);
    raw.extend_from_slice(&[0x11; 4 + 64 + 3]);

    let mut frame = vec![0x88, (-3.5_f32 * 4.0) as i8 as u8, -92_i8 as u8];
    frame.extend_from_slice(&raw);
    frame
}

const COMPANION: [u8; 32] = [0x92; 32];

fn companion_hex() -> String {
    "92".repeat(32)
}

#[tokio::test]
async fn the_first_station_heard_the_sender_of_a_flooded_advert() {
    let context = context_with(serde_json::json!({})).await;

    feed(&context, heard_advert(1, &[0xAA, 0xBB], COMPANION)).await;

    let links = read_links(&context).await.unwrap();
    // The path as before — bb heard aa, this node heard bb — and now also
    // who heard the sender, which is in no path at all.
    assert_eq!(links.len(), 3, "{links:?}");
    let first = links
        .iter()
        .find(|link| link.talker == companion_hex())
        .unwrap();
    assert_eq!(first.listener, "aa");
    assert_eq!(first.width, 1);
}

#[tokio::test]
async fn a_zero_hop_advert_was_heard_by_this_node() {
    let context = context_with(serde_json::json!({})).await;

    // Direct with an empty path: `sendZeroHop`, straight from the sender.
    feed(&context, heard_advert(2, &[], COMPANION)).await;

    let links = read_links(&context).await.unwrap();
    assert_eq!(links.len(), 1, "{links:?}");
    assert_eq!(links[0].talker, companion_hex());
    assert!(links[0].listener.is_empty());
}

#[tokio::test]
async fn a_direct_advert_with_a_route_ahead_proves_nothing() {
    // Never sent by any firmware in d929643 — but if it were, its path would
    // be the route ahead, and its first station has heard nothing yet.
    let context = context_with(serde_json::json!({})).await;

    feed(&context, heard_advert(2, &[0xAA], COMPANION)).await;

    assert!(read_links(&context).await.unwrap().is_empty());
}

#[tokio::test]
async fn only_an_advert_has_a_sender() {
    let context = context_with(serde_json::json!({})).await;

    feed(&context, heard(&[0xAA])).await;

    let links = read_links(&context).await.unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].talker, "aa");
}

#[tokio::test]
async fn timed_links_count_who_heard_an_advert_first() {
    let context = context_with(serde_json::json!({ "keep_days": 30 })).await;

    feed(&context, heard_advert(1, &[0xAA], COMPANION)).await;
    feed(&context, heard_advert(2, &[], COMPANION)).await;
    // Refused in the summary, so refused in the window too.
    feed(&context, heard_advert(2, &[0xBB], COMPANION)).await;

    let now = Utc::now();
    let links = read_links_in_window(
        &context,
        &(now - chrono::Duration::hours(1)),
        &(now + chrono::Duration::minutes(1)),
    )
    .await
    .unwrap();

    let from_sender: Vec<_> = links
        .iter()
        .filter(|link| link.talker == companion_hex())
        .collect();
    assert_eq!(from_sender.len(), 2, "{links:?}");
    assert!(from_sender.iter().any(|link| link.listener == "aa"));
    assert!(from_sender.iter().any(|link| link.listener.is_empty()));
    assert!(from_sender.iter().all(|link| link.heard == 1));
}
