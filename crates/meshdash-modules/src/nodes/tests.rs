//! Tests for the nodes module, against a real database and a mock node.

use crate::query::Window;
use meshdash_core::{
    config::ModuleSettings,
    db::Database,
    event::EventBus,
    link::{self, LinkConfig},
    module::{AppContext, ModuleRegistry},
};
use meshdash_proto::opcode::Response;
use meshdash_transport::mock::{MockTransport, Step};

use super::*;

/// Prefixes a test script with the answer to the session start.
///
/// Since the link announces itself on every connection, frame 1 on the wire
/// is always `CMD_APP_START`. A script written as if the test's own command
/// came first would have its answers handed to the session start instead.
/// The `AwaitSent` counts inside the script are shifted by one to match.
fn after_session_start(script: Vec<Step>) -> Vec<Step> {
    let mut full = vec![Step::Emit(session_answer())];
    for step in script {
        match step {
            Step::AwaitSent(count) => full.push(Step::AwaitSent(count + 1)),
            // A dropped link reconnects, and the new connection starts a
            // session of its own before anything else goes out.
            Step::Drop(reason) => {
                full.push(Step::Drop(reason));
                full.push(Step::Emit(session_answer()));
            }
            other => full.push(other),
        }
    }
    full
}

/// A minimal `RESP_CODE_SELF_INFO`, enough to end the session start.
fn session_answer() -> Vec<u8> {
    let mut payload = vec![0u8; 58];
    payload[0] = u8::from(Response::SelfInfo);
    payload
}

/// Builds a context whose node replies with the given script.
async fn context_with(script: Vec<Step>) -> AppContext {
    let db = Database::open_in_memory().await.unwrap();
    let events = EventBus::new();
    let (link, _task) = link::spawn(
        MockTransport::new(after_session_start(script)),
        LinkConfig::default(),
        events.clone(),
    );
    let context = AppContext {
        db,
        events,
        link,
        settings: ModuleSettings::default(),
    };

    let mut registry = ModuleRegistry::new();
    registry.register(Box::new(NodesModule)).unwrap();
    registry.start_all(&context).await.unwrap();
    context
}

/// A contact frame as the firmware lays it out.
fn contact_frame(key: u8, name: &str, path: &[u8]) -> Vec<u8> {
    let mut payload = vec![0u8; 148];
    payload[0] = u8::from(Response::Contact);
    payload[1..33].copy_from_slice(&[key; 32]);
    payload[33] = 2;
    payload[35] = path.len() as u8;
    payload[36..36 + path.len()].copy_from_slice(path);
    payload[100..100 + name.len()].copy_from_slice(name.as_bytes());
    payload[132..136].copy_from_slice(&1_700_000_000_u32.to_le_bytes());
    payload[136..140].copy_from_slice(&52_520_008_i32.to_le_bytes());
    payload[140..144].copy_from_slice(&13_404_954_i32.to_le_bytes());
    payload
}

/// Start and end markers of a listing.
fn start_frame() -> Vec<u8> {
    let mut frame = vec![0u8; 5];
    frame[0] = u8::from(Response::ContactsStart);
    frame[1..5].copy_from_slice(&2_u32.to_le_bytes());
    frame
}

fn end_frame() -> Vec<u8> {
    let mut frame = vec![0u8; 5];
    frame[0] = u8::from(Response::EndOfContacts);
    frame
}

/// A script answering one listing with the given contacts.
fn listing(contacts: Vec<Vec<u8>>) -> Vec<Step> {
    let mut script = vec![Step::AwaitSent(1), Step::Emit(start_frame())];
    script.extend(contacts.into_iter().map(Step::Emit));
    script.push(Step::Emit(end_frame()));
    script.push(Step::Drop("script finished".into()));
    script
}

#[tokio::test]
async fn starts_with_nothing_known() {
    let context = context_with(vec![]).await;

    assert!(read_contacts(&context).await.unwrap().is_empty());
}

#[tokio::test]
async fn fetches_and_stores_the_contact_list() {
    let context = context_with(listing(vec![
        contact_frame(0xAA, "Repeater Nord", &[1, 2]),
        contact_frame(0xBB, "Room Server", &[]),
    ]))
    .await;

    let stored = sync_contacts(&context).await.unwrap();

    assert_eq!(stored, 2);
    let contacts = read_contacts(&context).await.unwrap();
    assert_eq!(contacts.len(), 2);
    assert!(contacts.iter().any(|c| c.name == "Repeater Nord"));
}

#[tokio::test]
async fn writes_the_key_and_path_as_hex() {
    let context = context_with(listing(vec![contact_frame(0xAB, "Node", &[1, 2, 3])])).await;
    sync_contacts(&context).await.unwrap();

    let contacts = read_contacts(&context).await.unwrap();

    assert!(
        contacts[0].public_key.starts_with("abab"),
        "got {}",
        contacts[0].public_key
    );
    assert_eq!(contacts[0].path.as_deref(), Some("010203"));
}

#[tokio::test]
async fn reports_positions_in_degrees() {
    let context = context_with(listing(vec![contact_frame(0xAA, "Node", &[])])).await;
    sync_contacts(&context).await.unwrap();

    let contacts = read_contacts(&context).await.unwrap();

    let latitude = contacts[0].latitude.unwrap();
    assert!((latitude - 52.520_008).abs() < 1e-6, "got {latitude}");
}

#[tokio::test]
async fn keeps_the_first_sighting_across_updates() {
    // A node that forgets a contact must not erase our own history of it.
    let context = context_with(vec![]).await;
    let contact = Contact::parse(&contact_frame(0xAA, "Erst", &[])).unwrap();

    store_contact(&context, &contact).await.unwrap();
    let first = read_contacts(&context).await.unwrap()[0].first_seen;

    let renamed = Contact::parse(&contact_frame(0xAA, "Danach", &[])).unwrap();
    store_contact(&context, &renamed).await.unwrap();

    let contacts = read_contacts(&context).await.unwrap();
    assert_eq!(contacts.len(), 1, "same key, same row");
    assert_eq!(contacts[0].name, "Danach", "the newer name wins");
    assert_eq!(contacts[0].first_seen, first, "the first sighting stands");
}

#[tokio::test]
async fn passes_unverified_fields_through_unread() {
    // type and flags have no documented meaning; inventing one would be worse
    // than handing the number on.
    let context = context_with(listing(vec![contact_frame(0xAA, "Node", &[])])).await;
    sync_contacts(&context).await.unwrap();

    let contacts = read_contacts(&context).await.unwrap();

    assert_eq!(contacts[0].contact_type, 2);
}

#[tokio::test]
async fn survives_a_node_that_answers_nothing() {
    let context = context_with(vec![Step::Drop("silent".into())]).await;

    assert!(sync_contacts(&context).await.is_err());
    assert!(read_contacts(&context).await.unwrap().is_empty());
}

#[tokio::test]
async fn fetches_when_the_node_becomes_reachable() {
    let context = context_with(listing(vec![contact_frame(0xAA, "Node", &[])])).await;

    context.events.publish(AppEvent::NodeConnected);
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    assert_eq!(read_contacts(&context).await.unwrap().len(), 1);
}

/// A short advert: the push opcode and a public key.
fn known_advert_frame(key: u8) -> Vec<u8> {
    let mut payload = vec![0u8; 33];
    payload[0] = u8::from(meshdash_proto::opcode::Push::Advert);
    payload[1..33].copy_from_slice(&[key; 32]);
    payload
}

/// A long advert: a contact frame under the new-advert opcode.
fn new_advert_frame(key: u8, name: &str) -> Vec<u8> {
    let mut payload = contact_frame(key, name, &[3, 4]);
    payload[0] = u8::from(meshdash_proto::opcode::Push::NewAdvert);
    payload
}

/// A contact with a route of our choosing.
fn contact_with_path(key: u8, name: &str, hops: &[u8]) -> Contact {
    Contact {
        public_key: [key; 32],
        contact_type: 2,
        flags: 0,
        path: (!hops.is_empty()).then(|| meshdash_proto::contact::Route {
            stations: hops.len() as u8,
            hops: hops.to_vec(),
        }),
        name: name.into(),
        last_advert: 0,
        latitude: None,
        longitude: None,
        last_modified: 0,
    }
}

/// A new-contact advert carrying a route of our choosing.
fn new_advert_frame_with_path(key: u8, name: &str, path: &[u8]) -> Vec<u8> {
    let mut payload = contact_frame(key, name, path);
    payload[0] = u8::from(meshdash_proto::opcode::Push::NewAdvert);
    payload
}

/// Hands the module a push and waits for it to be processed.
async fn push(context: &AppContext, payload: Vec<u8>) {
    context.events.publish(AppEvent::Push { payload });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
}

#[tokio::test]
async fn a_new_advert_stores_the_contact_it_carries() {
    let context = context_with(vec![]).await;

    push(&context, new_advert_frame(0xCD, "Nachbar")).await;

    let contacts = read_contacts(&context).await.unwrap();
    assert_eq!(contacts.len(), 1);
    assert_eq!(contacts[0].name, "Nachbar");
    assert_eq!(contacts[0].path.as_deref(), Some("0304"));
}

#[tokio::test]
async fn every_advert_becomes_a_sighting() {
    let context = context_with(vec![]).await;

    push(&context, new_advert_frame(0xCD, "Nachbar")).await;
    push(&context, known_advert_frame(0xCD)).await;

    let sightings = read_adverts(&context, None, &Window::paged(200, None))
        .await
        .unwrap();
    assert_eq!(sightings.len(), 2);
    assert!(sightings.iter().all(|s| s.public_key == "cd".repeat(32)));
    // Newest first: the short one arrived last.
    assert!(!sightings[0].was_new);
    assert!(sightings[1].was_new);
}

#[tokio::test]
async fn a_short_advert_moves_the_last_sighting_forward() {
    let context = context_with(listing(vec![contact_frame(0xAA, "Node", &[])])).await;
    sync_contacts(&context).await.unwrap();
    let before = read_contacts(&context).await.unwrap()[0].last_seen;

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    push(&context, known_advert_frame(0xAA)).await;

    let contacts = read_contacts(&context).await.unwrap();
    let after = &contacts[0];
    assert!(after.last_seen > before);
    // A short advert says "heard", not "changed" — the details stay.
    assert_eq!(after.name, "Node");
}

#[tokio::test]
async fn a_short_advert_for_an_unknown_key_is_still_recorded() {
    let context = context_with(vec![]).await;

    push(&context, known_advert_frame(0x11)).await;

    // The sighting is true even without a name for the key.
    assert_eq!(
        read_adverts(&context, None, &Window::paged(200, None))
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(read_contacts(&context).await.unwrap().is_empty());
}

#[tokio::test]
async fn ignores_pushes_that_are_not_adverts() {
    let context = context_with(vec![]).await;

    push(&context, vec![0x83, 0x00]).await;

    assert!(
        read_adverts(&context, None, &Window::paged(200, None))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn a_contact_without_a_known_route_stores_no_path() {
    // The firmware marks that with OUT_PATH_UNKNOWN (0xFF) in the length
    // field. Real hardware carries it on nearly every contact, and storing it
    // as 64 zero bytes invented a 64-hop route.
    let mut frame = contact_frame(0xAA, "Weit weg", &[]);
    frame[35] = 0xFF; // OUT_PATH_UNKNOWN
    let context = context_with(listing(vec![frame])).await;

    sync_contacts(&context).await.unwrap();

    let contacts = read_contacts(&context).await.unwrap();
    assert_eq!(contacts[0].path, None);
    assert_eq!(contacts[0].stations, None);
}

#[tokio::test]
async fn a_direct_contact_is_not_the_same_as_an_unknown_one() {
    // Zero stations is knowledge: reachable directly. It must stay distinct
    // from "no idea how to reach them".
    let context = context_with(listing(vec![contact_frame(0xBB, "Direkt", &[])])).await;

    sync_contacts(&context).await.unwrap();

    let contacts = read_contacts(&context).await.unwrap();
    assert_eq!(contacts[0].path.as_deref(), Some(""));
    assert_eq!(contacts[0].stations, Some(0));
}

#[tokio::test]
async fn counts_stations_rather_than_bytes() {
    // Two stations of two bytes each: four bytes of path, but two hops. The
    // two numbers are different, and only one of them is what an operator
    // wants to read.
    let mut frame = contact_frame(0xCC, "Breiter Weg", &[]);
    frame[35] = 0x42; // 0b01_000010
    frame[36..40].copy_from_slice(&[1, 2, 3, 4]);
    let context = context_with(listing(vec![frame])).await;

    sync_contacts(&context).await.unwrap();

    let contacts = read_contacts(&context).await.unwrap();
    assert_eq!(contacts[0].stations, Some(2));
    assert_eq!(contacts[0].path.as_deref(), Some("01020304"));
}

#[tokio::test]
async fn sightings_can_be_asked_for_one_node_only() {
    // A page about one node should not fetch two hundred rows to show five.
    let context = context_with(vec![]).await;
    push(&context, known_advert_frame(0xAA)).await;
    push(&context, known_advert_frame(0xBB)).await;

    let alle = read_adverts(&context, None, &Window::paged(200, None))
        .await
        .unwrap();
    let nur_einer = read_adverts(&context, Some(&"aa".repeat(32)), &Window::paged(200, None))
        .await
        .unwrap();

    assert_eq!(alle.len(), 2);
    assert_eq!(nur_einer.len(), 1);
    assert_eq!(nur_einer[0].public_key, "aa".repeat(32));
}

#[tokio::test]
async fn a_cursor_continues_the_sighting_list() {
    let context = context_with(vec![]).await;
    push(&context, new_advert_frame(0xCD, "Nachbar")).await;
    push(&context, known_advert_frame(0xCD)).await;
    push(&context, known_advert_frame(0xCD)).await;

    let erste_seite = read_adverts(&context, None, &Window::paged(2, None))
        .await
        .unwrap();
    let zweite_seite = read_adverts(&context, None, &Window::paged(2, Some(erste_seite[1].id)))
        .await
        .unwrap();

    assert_eq!(erste_seite.len(), 2);
    assert_eq!(zweite_seite.len(), 1);
    // The oldest one is the first advert, the one that carried a name.
    assert!(zweite_seite[0].was_new);
}

#[tokio::test]
async fn a_changed_route_is_recorded_as_a_change() {
    // Two listings for the same contact: first over one station, then over
    // two. Overwriting the row alone loses the fact that anything moved.
    let context = context_with(listing(vec![contact_frame(0xAA, "Node", &[0x03])])).await;
    sync_contacts(&context).await.unwrap();

    let changes = read_route_changes(&context, None, &Window::paged(50, None))
        .await
        .unwrap();

    assert!(
        changes.is_empty(),
        "the first route known is not a change, it is the starting point"
    );
}

#[tokio::test]
async fn a_route_that_moves_leaves_a_trail() {
    let context = context_with(listing(vec![contact_frame(0xAA, "Node", &[0x03])])).await;
    sync_contacts(&context).await.unwrap();

    push(
        &context,
        new_advert_frame_with_path(0xAA, "Node", &[0x03, 0x07]),
    )
    .await;

    let changes = read_route_changes(&context, None, &Window::paged(50, None))
        .await
        .unwrap();

    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].public_key, "aa".repeat(32));
    assert_eq!(changes[0].previous_path.as_deref(), Some("03"));
    assert_eq!(changes[0].previous_stations, Some(1));
    assert_eq!(changes[0].path.as_deref(), Some("0307"));
    assert_eq!(changes[0].stations, Some(2));
}

#[tokio::test]
async fn an_unchanged_route_records_nothing() {
    let context = context_with(listing(vec![contact_frame(0xAA, "Node", &[0x03])])).await;
    sync_contacts(&context).await.unwrap();

    push(&context, new_advert_frame_with_path(0xAA, "Node", &[0x03])).await;

    assert!(
        read_route_changes(&context, None, &Window::paged(50, None))
            .await
            .unwrap()
            .is_empty(),
        "the same route seen again is not a change"
    );
}

#[tokio::test]
async fn presence_counts_sightings_per_bucket() {
    let context = context_with(vec![]).await;
    // Three sightings: two in the first hour, one in the third.
    for at in [
        "2026-08-21T10:05:00+00:00",
        "2026-08-21T10:45:00.123456+00:00",
        "2026-08-21T12:30:00+00:00",
    ] {
        sqlx::query("INSERT INTO nodes_adverts (public_key, heard_at, was_new) VALUES (?, ?, 0)")
            .bind("aa".repeat(32))
            .bind(at)
            .execute(context.db.pool())
            .await
            .unwrap();
    }

    let presence = read_presence(
        &context,
        &"aa".repeat(32),
        DateTime::parse_from_rfc3339("2026-08-21T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
        DateTime::parse_from_rfc3339("2026-08-21T13:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
        3,
    )
    .await
    .unwrap();

    assert_eq!(
        presence
            .buckets
            .iter()
            .map(|b| b.sightings)
            .collect::<Vec<_>>(),
        vec![2, 0, 1],
        "an hour without a sighting is a zero, not a missing bucket"
    );
}

#[tokio::test]
async fn presence_ignores_other_nodes_and_other_times() {
    let context = context_with(vec![]).await;
    let rows = [
        ("aa".repeat(32), "2026-08-21T10:30:00+00:00"),
        ("bb".repeat(32), "2026-08-21T10:30:00+00:00"),
        ("aa".repeat(32), "2026-08-20T10:30:00+00:00"),
    ];
    for (key, at) in rows {
        sqlx::query("INSERT INTO nodes_adverts (public_key, heard_at, was_new) VALUES (?, ?, 0)")
            .bind(key)
            .bind(at)
            .execute(context.db.pool())
            .await
            .unwrap();
    }

    let presence = read_presence(
        &context,
        &"aa".repeat(32),
        DateTime::parse_from_rfc3339("2026-08-21T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
        DateTime::parse_from_rfc3339("2026-08-21T11:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
        2,
    )
    .await
    .unwrap();

    assert_eq!(presence.buckets.iter().map(|b| b.sightings).sum::<i64>(), 1);
}

/// The node's receipt for a trace: the same frame a sent message gets, with
/// the tag where the acknowledgement would be.
fn trace_receipt(tag: u32, timeout_ms: u32) -> Vec<u8> {
    let mut frame = vec![u8::from(Response::Sent), 0];
    frame.extend_from_slice(&tag.to_le_bytes());
    frame.extend_from_slice(&timeout_ms.to_le_bytes());
    frame
}

/// A trace coming back over two stations, one SNR each.
fn trace_answer(tag: u32, hops: &[u8], snrs: &[f32], final_snr: f32) -> Vec<u8> {
    let mut frame = vec![
        u8::from(meshdash_proto::opcode::Push::TraceData),
        0,
        hops.len() as u8,
        0, // flags: one SNR per station
    ];
    frame.extend_from_slice(&tag.to_le_bytes());
    frame.extend_from_slice(&0u32.to_le_bytes()); // authentication code
    frame.extend_from_slice(hops);
    frame.extend(snrs.iter().map(|snr| (snr * 4.0) as i8 as u8));
    frame.push((final_snr * 4.0) as i8 as u8);
    frame
}

#[tokio::test]
async fn a_trace_records_every_station_and_its_reception() {
    let context = context_with(vec![
        Step::AwaitSent(1),
        Step::Emit(trace_receipt(0x1234_5678, 4_000)),
    ])
    .await;
    store_contact(&context, &contact_with_path(0xAA, "Fern", &[0x03, 0x07]))
        .await
        .unwrap();

    start_trace(&context, &"aa".repeat(32)).await.unwrap();

    // The answer comes over the bus, the way the link delivers it. Fed in
    // here rather than scripted into the mock: the mock hands out its frames
    // as fast as the reader asks for them, so an answer scripted next to the
    // receipt can arrive before the request that it answers.
    push(
        &context,
        trace_answer(0x1234_5678, &[0x03, 0x07], &[6.5, -2.0], 9.0),
    )
    .await;

    let traces = read_traces(&context, None, &Window::paged(10, None))
        .await
        .unwrap();
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].public_key, "aa".repeat(32));
    assert_eq!(traces[0].final_snr, Some(9.0));
    assert_eq!(
        traces[0]
            .hops
            .iter()
            .map(|hop| (hop.key_prefix.clone(), hop.snr))
            .collect::<Vec<_>>(),
        vec![("03".into(), Some(6.5)), ("07".into(), Some(-2.0))]
    );
}

#[tokio::test]
async fn a_trace_that_never_comes_back_stays_unanswered() {
    let context = context_with(vec![
        Step::AwaitSent(1),
        Step::Emit(trace_receipt(0xAAAA_BBBB, 4_000)),
    ])
    .await;
    store_contact(&context, &contact_with_path(0xBB, "Still", &[0x05]))
        .await
        .unwrap();

    start_trace(&context, &"bb".repeat(32)).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let traces = read_traces(&context, None, &Window::paged(10, None))
        .await
        .unwrap();
    // Recorded as asked and unanswered rather than not recorded: "we tried
    // and nothing came back" is a finding about the route.
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].answered_at, None);
    assert!(traces[0].hops.is_empty());
}

#[tokio::test]
async fn a_node_without_a_route_cannot_be_traced() {
    let context = context_with(vec![]).await;
    store_contact(&context, &contact_with_path(0xCC, "Direkt", &[]))
        .await
        .unwrap();

    // A trace follows a route station by station. With no station in
    // between there is nothing to ask about, and the firmware refuses the
    // frame outright.
    assert!(matches!(
        start_trace(&context, &"cc".repeat(32)).await,
        Err(TraceError::NoRoute)
    ));
}
