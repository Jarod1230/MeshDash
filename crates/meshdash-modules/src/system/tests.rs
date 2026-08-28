//! Tests for the system module.
//!
//! Against a real in-memory database with the real schema, and a mock node —
//! no stand-ins for the core, because the point is that module, storage and
//! link fit together.

use crate::query::Window;
use meshdash_core::{
    db::Database,
    event::EventBus,
    link::{self, LinkConfig},
    module::{AppContext, ModuleRegistry},
    settings::Settings,
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
        settings: Settings::default(),
    };

    // Migrate through the registry, so the test exercises the same path the
    // binary takes rather than applying migrations by hand.
    let mut registry = ModuleRegistry::new();
    registry.register(Box::new(SystemModule)).unwrap();
    registry.start_all(&context).await.unwrap();

    context
}

/// A device info payload as the firmware lays it out.
fn device_info_frame() -> Vec<u8> {
    let mut payload = vec![0u8; 82];
    payload[0] = u8::from(Response::DeviceInfo);
    payload[1] = 13;
    payload[2] = 50; // halved capacity: 100 contacts
    payload[3] = 8;
    payload[8..19].copy_from_slice(b"14 Aug 2026");
    payload[20..26].copy_from_slice(b"Heltec");
    payload[60..67].copy_from_slice(b"v1.17.1");
    payload[80] = 1;
    payload
}

#[tokio::test]
async fn reports_nothing_before_anything_happened() {
    let context = context_with(vec![]).await;

    let status = read_status(&context).await.unwrap();

    assert!(!status.connected);
    assert_eq!(status.since, None);
    assert_eq!(status.node, None);
}

#[tokio::test]
async fn reports_a_connection() {
    let context = context_with(vec![]).await;

    record_connection(&context, true, None).await.unwrap();

    let status = read_status(&context).await.unwrap();
    assert!(status.connected);
    assert!(status.since.is_some());
}

#[tokio::test]
async fn reports_the_latest_change_not_the_first() {
    let context = context_with(vec![]).await;

    record_connection(&context, true, None).await.unwrap();
    record_connection(&context, false, Some("cable pulled"))
        .await
        .unwrap();

    let status = read_status(&context).await.unwrap();
    assert!(!status.connected);
    assert_eq!(status.reason.as_deref(), Some("cable pulled"));
}

#[tokio::test]
async fn keeps_the_whole_history() {
    // "Currently reachable" says little; that a node dropped out repeatedly is
    // the finding worth having.
    let context = context_with(vec![]).await;

    for _ in 0..3 {
        record_connection(&context, true, None).await.unwrap();
        record_connection(&context, false, Some("flapping"))
            .await
            .unwrap();
    }

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM system_connection_events")
        .fetch_one(context.db.pool())
        .await
        .unwrap();

    assert_eq!(count.0, 6, "every change must be kept, not just the last");
}

#[tokio::test]
async fn stores_what_the_node_reported() {
    let context = context_with(vec![]).await;
    let info = DeviceInfo::parse(&device_info_frame()).unwrap();

    store_identity(&context, &info).await.unwrap();

    let status = read_status(&context).await.unwrap();
    let node = status.node.expect("expected an identity");
    assert_eq!(node.firmware_version, "v1.17.1");
    assert_eq!(node.manufacturer, "Heltec");
    assert_eq!(node.contact_capacity, 100);
    assert_eq!(node.repeater_enabled, Some(true));
}

#[tokio::test]
async fn keeps_only_the_newest_identity() {
    // One node per instance; a second row would make "the node" ambiguous.
    let context = context_with(vec![]).await;
    let info = DeviceInfo::parse(&device_info_frame()).unwrap();

    store_identity(&context, &info).await.unwrap();
    store_identity(&context, &info).await.unwrap();

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM system_node_info")
        .fetch_one(context.db.pool())
        .await
        .unwrap();

    assert_eq!(count.0, 1);
}

#[tokio::test]
async fn writes_down_what_the_link_reports() {
    // The whole point: the module listens, without anyone calling it.
    let context = context_with(vec![]).await;

    context.events.publish(AppEvent::NodeConnected);
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let status = read_status(&context).await.unwrap();
    assert!(status.connected, "the module must react to the bus");
}

#[tokio::test]
async fn asks_the_node_who_it_is_once_connected() {
    // The node answers only once asked — otherwise the idle reader would
    // swallow the answer before the question was sent.
    //
    let context = context_with(vec![
        Step::AwaitSent(1),
        Step::Emit(device_info_frame()),
        Step::Drop("script finished".into()),
    ])
    .await;

    context.events.publish(AppEvent::NodeConnected);
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let status = read_status(&context).await.unwrap();
    let node = status.node.expect("the module should have asked");
    assert_eq!(node.firmware_version, "v1.17.1");
}

#[tokio::test]
async fn survives_a_node_that_does_not_answer_the_query() {
    // A node that stays silent must not stop the connection from being
    // recorded — knowing it is reachable is worth more than its name.
    let context = context_with(vec![Step::Drop("silent".into())]).await;

    context.events.publish(AppEvent::NodeConnected);
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let status = read_status(&context).await.unwrap();
    assert!(status.connected);
    assert_eq!(status.node, None, "no identity, but the state is kept");
}

#[tokio::test]
async fn serialises_a_status_the_way_the_api_promises() {
    let context = context_with(vec![]).await;
    record_connection(&context, true, None).await.unwrap();

    let status = read_status(&context).await.unwrap();
    let json = serde_json::to_value(&status).unwrap();

    // snake_case fields and an RFC 3339 timestamp, per docs/conventions.md.
    assert_eq!(json["connected"], serde_json::json!(true));
    let since = json["since"].as_str().expect("expected a timestamp");
    assert!(since.ends_with('Z'), "expected UTC, got {since}");
    assert!(
        chrono::DateTime::parse_from_rfc3339(since).is_ok(),
        "expected RFC 3339, got {since}"
    );
}

#[tokio::test]
async fn keeps_a_history_of_connection_changes() {
    // The current state alone cannot answer "is this link stable": a node that
    // reconnects every two minutes reports itself connected each time.
    let context = context_with(vec![]).await;

    record_connection(&context, true, None).await.unwrap();
    record_connection(&context, false, Some("Kabel gezogen"))
        .await
        .unwrap();
    record_connection(&context, true, None).await.unwrap();

    let history = read_connections(&context, &Window::paged(10, None))
        .await
        .unwrap();

    assert_eq!(history.len(), 3);
    assert!(history[0].connected, "newest first");
    assert!(!history[1].connected);
    assert_eq!(history[1].reason.as_deref(), Some("Kabel gezogen"));
}

#[tokio::test]
async fn the_history_is_bounded() {
    let context = context_with(vec![]).await;
    for _ in 0..10 {
        record_connection(&context, true, None).await.unwrap();
    }

    assert_eq!(
        read_connections(&context, &Window::paged(4, None))
            .await
            .unwrap()
            .len(),
        4
    );
}

#[test]
fn caps_what_a_request_may_ask_for() {
    assert_eq!(ListQuery::default().effective_limit(), DEFAULT_LIMIT);
    assert_eq!(
        ListQuery {
            limit: Some(99_999),
            ..Default::default()
        }
        .effective_limit(),
        MAX_LIMIT
    );
    assert_eq!(
        ListQuery {
            limit: Some(0),
            ..Default::default()
        }
        .effective_limit(),
        1
    );
}

/// The node's answer to a session start, as the firmware builds it.
fn self_info_frame() -> Vec<u8> {
    let mut payload = vec![0u8; 58];
    payload[0] = u8::from(Response::SelfInfo);
    payload[1] = 1; // ADV_TYPE_CHAT
    payload[2] = 22; // transmit power
    payload[3] = 30; // what the board allows
    payload[4..36].copy_from_slice(&[0xEE; 32]);
    payload[36..40].copy_from_slice(&52_520_008_i32.to_le_bytes());
    payload[40..44].copy_from_slice(&13_404_954_i32.to_le_bytes());
    payload[48..52].copy_from_slice(&869_618_u32.to_le_bytes());
    payload[52..56].copy_from_slice(&62_500_u32.to_le_bytes());
    payload[56] = 11;
    payload[57] = 5;
    payload.extend_from_slice(b"DB0MSH");
    payload
}

#[tokio::test]
async fn the_node_says_who_it_is_when_a_session_starts() {
    let context = context_with(vec![]).await;

    context.events.publish(AppEvent::SessionStarted {
        payload: self_info_frame(),
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let described = read_self(&context).await.unwrap().unwrap();
    assert_eq!(described.name, "DB0MSH");
    assert_eq!(described.public_key, "ee".repeat(32));
    assert_eq!(described.latitude, Some(52.520008));
    assert_eq!(described.transmit_power_dbm, 22);
    // Kilohertz and hertz, side by side, exactly as the firmware sends them.
    assert_eq!(described.frequency_khz, 869_618);
    assert_eq!(described.bandwidth_hz, 62_500);
}

#[tokio::test]
async fn a_node_without_a_position_leaves_it_empty() {
    let context = context_with(vec![]).await;
    let mut frame = self_info_frame();
    // Zero is the firmware's "unset", not a spot in the Gulf of Guinea.
    frame[36..44].fill(0);

    context
        .events
        .publish(AppEvent::SessionStarted { payload: frame });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let described = read_self(&context).await.unwrap().unwrap();
    assert_eq!(described.latitude, None);
    assert_eq!(described.longitude, None);
}

#[tokio::test]
async fn a_second_session_replaces_what_the_first_said() {
    let context = context_with(vec![]).await;
    context.events.publish(AppEvent::SessionStarted {
        payload: self_info_frame(),
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let mut renamed = self_info_frame();
    renamed.truncate(58);
    renamed.extend_from_slice(b"DB0NEU");
    context
        .events
        .publish(AppEvent::SessionStarted { payload: renamed });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // One node per instance, so one row — a rename must not leave two.
    let described = read_self(&context).await.unwrap().unwrap();
    assert_eq!(described.name, "DB0NEU");
}

#[tokio::test]
async fn setting_the_position_reaches_the_node_and_the_row() {
    let context = context_with(vec![
        Step::AwaitSent(1),
        Step::Emit(vec![u8::from(Response::Ok)]),
    ])
    .await;

    // A session first: without it there is no self description to update.
    context.events.publish(AppEvent::SessionStarted {
        payload: self_info_frame(),
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    set_position(&context, 54.331026, 13.070254).await.unwrap();

    let described = read_self(&context).await.unwrap().unwrap();
    assert_eq!(described.latitude, Some(54.331026));
    assert_eq!(described.longitude, Some(13.070254));
}

#[tokio::test]
async fn a_refused_position_is_not_written_down() {
    let context = context_with(vec![
        Step::AwaitSent(1),
        Step::Emit(vec![u8::from(Response::Err), 1]),
    ])
    .await;

    context.events.publish(AppEvent::SessionStarted {
        payload: self_info_frame(),
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    assert!(matches!(
        set_position(&context, 10.0, 20.0).await,
        Err(PositionError::Refused { .. })
    ));

    // Still what the node said about itself, not what we asked for.
    let described = read_self(&context).await.unwrap().unwrap();
    assert_eq!(described.latitude, Some(52.520008));
}

#[tokio::test]
async fn a_coordinate_that_is_not_one_never_goes_on_the_wire() {
    let context = context_with(vec![Step::Drop("nothing should be sent".into())]).await;

    for (latitude, longitude) in [(f64::NAN, 0.0), (0.0, f64::INFINITY), (91.0, 0.0)] {
        assert!(matches!(
            set_position(&context, latitude, longitude).await,
            Err(PositionError::Unusable(_))
        ));
    }
}

#[tokio::test]
async fn a_position_set_before_the_node_introduced_itself_is_not_invented() {
    let context = context_with(vec![
        Step::AwaitSent(1),
        Step::Emit(vec![u8::from(Response::Ok)]),
    ])
    .await;

    set_position(&context, 54.0, 13.0).await.unwrap();

    // The node took it, but a self description without key and name would be
    // a half-truth in front of the operator.
    assert_eq!(read_self(&context).await.unwrap(), None);
}
