//! Tests for the telemetry module, against a real database and a mock node.

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
    registry.register(Box::new(TelemetryModule)).unwrap();
    registry.start_all(&context).await.unwrap();
    context
}

/// A battery frame as the firmware lays it out.
fn battery_frame(millivolts: u16) -> Vec<u8> {
    let mut frame = vec![u8::from(Response::BattAndStorage)];
    frame.extend_from_slice(&millivolts.to_le_bytes());
    frame.extend_from_slice(&512_u32.to_le_bytes());
    frame.extend_from_slice(&2048_u32.to_le_bytes());
    frame
}

/// A script answering one battery query.
fn answers(millivolts: u16) -> Vec<Step> {
    vec![
        Step::AwaitSent(1),
        Step::Emit(battery_frame(millivolts)),
        Step::Drop("script finished".into()),
    ]
}

#[tokio::test]
async fn starts_with_no_readings() {
    let context = context_with(vec![]).await;

    assert!(
        read_samples(&context, &Window::paged(10, None))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn asks_the_node_and_stores_the_reading() {
    let context = context_with(answers(4_100)).await;

    let reading = read_battery(&context).await.unwrap();
    store_sample(&context, &reading).await.unwrap();

    let samples = read_samples(&context, &Window::paged(10, None))
        .await
        .unwrap();
    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0].millivolts, 4_100);
    assert_eq!(samples[0].storage_used_kib, 512);
}

#[tokio::test]
async fn keeps_every_reading_to_form_a_curve() {
    // A single number says little; the point is the development over time.
    let context = context_with(vec![]).await;
    let reading = BatteryAndStorage::parse(&battery_frame(4_100)).unwrap();

    for _ in 0..5 {
        store_sample(&context, &reading).await.unwrap();
    }

    assert_eq!(
        read_samples(&context, &Window::paged(100, None))
            .await
            .unwrap()
            .len(),
        5
    );
}

#[tokio::test]
async fn reports_the_newest_first() {
    let context = context_with(vec![]).await;
    store_sample(
        &context,
        &BatteryAndStorage::parse(&battery_frame(4_000)).unwrap(),
    )
    .await
    .unwrap();
    store_sample(
        &context,
        &BatteryAndStorage::parse(&battery_frame(3_900)).unwrap(),
    )
    .await
    .unwrap();

    let samples = read_samples(&context, &Window::paged(10, None))
        .await
        .unwrap();

    assert_eq!(samples[0].millivolts, 3_900, "newest first");
}

#[tokio::test]
async fn honours_the_limit() {
    // The table grows without bound, so an unbounded read would eventually try
    // to serialise a year of readings at once.
    let context = context_with(vec![]).await;
    let reading = BatteryAndStorage::parse(&battery_frame(4_100)).unwrap();
    for _ in 0..10 {
        store_sample(&context, &reading).await.unwrap();
    }

    assert_eq!(
        read_samples(&context, &Window::paged(3, None))
            .await
            .unwrap()
            .len(),
        3
    );
}

#[tokio::test]
async fn samples_as_soon_as_the_node_is_reachable() {
    // Waiting out the interval would leave the curve empty for five minutes
    // after every restart.
    let context = context_with(answers(4_050)).await;

    context.events.publish(AppEvent::NodeConnected);
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let samples = read_samples(&context, &Window::paged(10, None))
        .await
        .unwrap();
    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0].millivolts, 4_050);
}

#[tokio::test]
async fn a_silent_node_costs_one_reading_not_the_task() {
    let context = context_with(vec![Step::Drop("silent".into())]).await;

    assert!(read_battery(&context).await.is_err());
    assert!(
        read_samples(&context, &Window::paged(10, None))
            .await
            .unwrap()
            .is_empty()
    );
}

/// A signal announcement as the messages module publishes it.
fn signal_event(source: &str, snr: Option<f64>, path_len: Option<u64>) -> AppEvent {
    AppEvent::Module {
        module: "messages".into(),
        kind: "signal".into(),
        data: serde_json::json!({
            "source": source,
            "snr": snr,
            "path_len": path_len,
        }),
    }
}

#[tokio::test]
async fn records_reception_quality_announced_by_another_module() {
    let context = context_with(vec![]).await;

    context
        .events
        .publish(signal_event("direct", Some(-2.5), Some(2)));
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let samples = read_signal_samples(&context, &Window::paged(10, None))
        .await
        .unwrap();
    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0].snr, -2.5);
    assert_eq!(samples[0].source, "direct");
    assert_eq!(samples[0].path_len, Some(2));
}

#[tokio::test]
async fn keeps_a_curve_of_readings() {
    let context = context_with(vec![]).await;

    for snr in [1.0, 2.0, 3.0] {
        context
            .events
            .publish(signal_event("channel", Some(snr), None));
    }
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let samples = read_signal_samples(&context, &Window::paged(10, None))
        .await
        .unwrap();
    assert_eq!(samples.len(), 3);
    assert_eq!(samples[0].snr, 3.0, "newest first");
}

#[tokio::test]
async fn skips_an_announcement_without_a_reading() {
    // Older protocol variants carry no SNR. That is not an error, and an
    // invented zero would show up as a real measurement in the curve.
    let context = context_with(vec![]).await;

    context.events.publish(signal_event("direct", None, None));
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    assert!(
        read_signal_samples(&context, &Window::paged(10, None))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn ignores_announcements_from_other_modules() {
    let context = context_with(vec![]).await;

    context.events.publish(AppEvent::Module {
        module: "nodes".into(),
        kind: "signal".into(),
        data: serde_json::json!({ "snr": 9.0 }),
    });
    context.events.publish(AppEvent::Module {
        module: "messages".into(),
        kind: "something_else".into(),
        data: serde_json::json!({ "snr": 9.0 }),
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    assert!(
        read_signal_samples(&context, &Window::paged(10, None))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn survives_a_payload_it_does_not_understand() {
    // The shape of the payload belongs to the other module and may change.
    let context = context_with(vec![]).await;

    context.events.publish(AppEvent::Module {
        module: "messages".into(),
        kind: "signal".into(),
        data: serde_json::json!("not an object"),
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    assert!(
        read_signal_samples(&context, &Window::paged(10, None))
            .await
            .unwrap()
            .is_empty()
    );
}

/// A context whose telemetry module is configured with the given section.
async fn context_configured(script: Vec<Step>, section: serde_json::Value) -> AppContext {
    let db = Database::open_in_memory().await.unwrap();
    let events = EventBus::new();
    let (link, _task) = link::spawn(
        MockTransport::new(after_session_start(script)),
        LinkConfig::default(),
        events.clone(),
    );
    let mut settings = ModuleSettings::default();
    settings.set("telemetry", section);
    let context = AppContext {
        db,
        events,
        link,
        settings,
    };

    let mut registry = ModuleRegistry::new();
    registry.register(Box::new(TelemetryModule)).unwrap();
    registry.start_all(&context).await.unwrap();
    context
}

/// A short advert, which carries the full key a request needs.
fn advert(key: u8) -> Vec<u8> {
    let mut payload = vec![u8::from(meshdash_proto::opcode::Push::Advert)];
    payload.extend_from_slice(&[key; 32]);
    payload
}

/// A binary response carrying one voltage reading.
fn telemetry_response(tag: u32, millivolts_times_hundred: u16) -> Vec<u8> {
    let mut frame = vec![u8::from(meshdash_proto::opcode::Push::BinaryResponse), 0];
    frame.extend_from_slice(&tag.to_le_bytes());
    frame.push(1); // channel: the node itself
    frame.push(116); // LPP_VOLTAGE
    frame.extend_from_slice(&millivolts_times_hundred.to_be_bytes());
    frame
}

#[tokio::test]
async fn asking_other_nodes_is_off_unless_configured() {
    // It transmits into a band the whole mesh shares, so it is the operator's
    // decision, not a default. Nothing is sent without being asked for.
    let context = context_with(vec![]).await;

    context.events.publish(AppEvent::Push {
        payload: advert(0xAA),
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let asked: Vec<(Option<String>,)> =
        sqlx::query_as("SELECT last_asked_at FROM telemetry_neighbours")
            .fetch_all(context.db.pool())
            .await
            .unwrap();

    assert_eq!(asked.len(), 1, "the node was noted");
    assert_eq!(asked[0].0, None, "but never asked");
}

#[tokio::test]
async fn an_advert_makes_a_node_worth_asking() {
    let context = context_with(vec![]).await;

    context.events.publish(AppEvent::Push {
        payload: advert(0xBB),
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let rows: Vec<(String,)> = sqlx::query_as("SELECT public_key FROM telemetry_neighbours")
        .fetch_all(context.db.pool())
        .await
        .unwrap();

    assert_eq!(rows[0].0, "bb".repeat(32));
}

#[tokio::test]
async fn an_answer_without_a_remembered_question_is_dropped() {
    // Only the tag comes back. After a restart nobody knows whose it was, and
    // attributing it to a guess would be worse than losing it.
    let context = context_with(vec![]).await;

    context.events.publish(AppEvent::Push {
        payload: telemetry_response(0xDEAD, 402),
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    assert!(
        read_neighbour_samples(&context, None, &Window::paged(10, None))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn stores_what_a_neighbour_reported() {
    let context = context_with(vec![]).await;

    store_neighbour_readings(
        &context,
        &"cc".repeat(32),
        &[
            meshdash_proto::lpp::Reading {
                channel: 1,
                type_code: 116,
                value: meshdash_proto::lpp::Value::Number(4.02),
            },
            meshdash_proto::lpp::Reading {
                channel: 2,
                type_code: 136,
                value: meshdash_proto::lpp::Value::Position {
                    latitude: 52.5608,
                    longitude: 13.2878,
                    altitude: 30.0,
                },
            },
        ],
    )
    .await
    .unwrap();

    let samples = read_neighbour_samples(&context, None, &Window::paged(10, None))
        .await
        .unwrap();

    assert_eq!(samples.len(), 2);
    // Newest first: the position was stored last.
    assert_eq!(samples[0].position, Some([52.5608, 13.2878, 30.0]));
    assert_eq!(samples[0].value, None, "a position is not a single number");
    assert_eq!(samples[1].value, Some(4.02));
}

#[tokio::test]
async fn a_configured_module_asks_a_node_it_has_heard() {
    // The receipt's acknowledgement field carries the tag for a request.
    let mut receipt = vec![u8::from(Response::Sent), 1];
    receipt.extend_from_slice(&0x00C0_FFEE_u32.to_le_bytes());
    receipt.extend_from_slice(&3_000_u32.to_le_bytes());

    let context = context_configured(
        vec![Step::AwaitSent(1), Step::Emit(receipt)],
        serde_json::json!({ "neighbours": true, "every_minutes": 1 }),
    )
    .await;

    context.events.publish(AppEvent::Push {
        payload: advert(0xDD),
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let settings = Settings {
        neighbours: true,
        every_minutes: 1,
        silent_after_hours: 24,
    };
    let pending = PendingRequests::default();
    ask_one_neighbour(&context, &settings, &pending).await;

    let asked: Vec<(Option<String>,)> =
        sqlx::query_as("SELECT last_asked_at FROM telemetry_neighbours")
            .fetch_all(context.db.pool())
            .await
            .unwrap();
    assert!(asked[0].0.is_some(), "the turn was recorded");

    // And the answer to that tag is now attributable.
    assert_eq!(
        pending.take(0x00C0_FFEE).as_deref(),
        Some("dd".repeat(32)).as_deref()
    );
}

#[tokio::test]
async fn a_node_silent_too_long_is_not_worth_transmitting_at() {
    let context = context_with(vec![]).await;
    sqlx::query("INSERT INTO telemetry_neighbours (public_key, last_heard_at) VALUES (?, ?)")
        .bind("ee".repeat(32))
        .bind((Utc::now() - chrono::Duration::hours(48)).to_rfc3339())
        .execute(context.db.pool())
        .await
        .unwrap();

    let settings = Settings {
        neighbours: true,
        every_minutes: 1,
        silent_after_hours: 24,
    };
    ask_one_neighbour(&context, &settings, &PendingRequests::default()).await;

    let asked: Vec<(Option<String>,)> =
        sqlx::query_as("SELECT last_asked_at FROM telemetry_neighbours")
            .fetch_all(context.db.pool())
            .await
            .unwrap();
    assert_eq!(asked[0].0, None, "nothing was sent");
}

#[test]
fn two_requests_in_a_row_carry_different_nonces() {
    // Identical packets hash the same and the second is dropped, so polling
    // would work exactly once.
    let first = nonce_from_clock();
    std::thread::sleep(std::time::Duration::from_micros(50));
    let second = nonce_from_clock();

    assert_ne!(first, second);
}

#[tokio::test]
async fn neighbour_readings_can_be_asked_for_one_node_only() {
    let context = context_with(vec![]).await;
    let reading = |value: f64| meshdash_proto::lpp::Reading {
        channel: 1,
        type_code: 116,
        value: meshdash_proto::lpp::Value::Number(value),
    };
    store_neighbour_readings(&context, &"aa".repeat(32), &[reading(4.0)])
        .await
        .unwrap();
    store_neighbour_readings(&context, &"bb".repeat(32), &[reading(3.9)])
        .await
        .unwrap();

    let alle = read_neighbour_samples(&context, None, &Window::paged(50, None))
        .await
        .unwrap();
    let nur_einer =
        read_neighbour_samples(&context, Some(&"aa".repeat(32)), &Window::paged(50, None))
            .await
            .unwrap();

    assert_eq!(alle.len(), 2);
    assert_eq!(nur_einer.len(), 1);
    assert_eq!(nur_einer[0].value, Some(4.0));
}

#[tokio::test]
async fn a_cursor_continues_the_signal_list() {
    let context = context_with(vec![]).await;
    for snr in [1.0, 2.0, 3.0] {
        context
            .events
            .publish(signal_event("channel", Some(snr), None));
    }
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let erste_seite = read_signal_samples(&context, &Window::paged(2, None))
        .await
        .unwrap();
    let zweite_seite = read_signal_samples(&context, &Window::paged(2, Some(erste_seite[1].id)))
        .await
        .unwrap();

    assert_eq!(erste_seite.len(), 2);
    assert_eq!(zweite_seite.len(), 1);
    assert!(
        zweite_seite[0].id < erste_seite[1].id,
        "the cursor must exclude what the first page already showed"
    );
}

/// Stores one reception quality with a timestamp of our choosing.
///
/// Written straight into the table: the recording path stamps `Utc::now()`,
/// and a test about time ranges needs rows that lie apart by more than the
/// microseconds a test run takes.
async fn signal_at(context: &AppContext, at: &str, snr: f64) {
    sqlx::query(
        "INSERT INTO telemetry_signal_samples (at, source, snr, path_len) VALUES (?, ?, ?, NULL)",
    )
    .bind(at)
    .bind("direct")
    .bind(snr)
    .execute(context.db.pool())
    .await
    .unwrap();
}

/// The same moment the database would have written.
fn stored(text: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(text)
        .unwrap()
        .with_timezone(&chrono::Utc)
        .to_rfc3339()
}

#[tokio::test]
async fn a_time_range_cuts_both_ends() {
    let context = context_with(vec![]).await;
    signal_at(&context, &stored("2026-08-20T10:00:00Z"), 1.0).await;
    // Fractional seconds, as the recording path writes them.
    signal_at(&context, "2026-08-21T10:00:00.123456+00:00", 2.0).await;
    signal_at(&context, &stored("2026-08-22T10:00:00Z"), 3.0).await;

    let mittendrin = read_signal_samples(
        &context,
        &Window {
            limit: 50,
            before: None,
            since: Some(stored("2026-08-21T00:00:00Z")),
            until: Some(stored("2026-08-22T00:00:00Z")),
        },
    )
    .await
    .unwrap();

    assert_eq!(mittendrin.len(), 1, "only the middle reading lies in range");
    assert_eq!(mittendrin[0].snr, 2.0);
}

#[tokio::test]
async fn an_open_end_reaches_to_the_edge() {
    let context = context_with(vec![]).await;
    signal_at(&context, &stored("2026-08-20T10:00:00Z"), 1.0).await;
    signal_at(&context, &stored("2026-08-22T10:00:00Z"), 3.0).await;

    let ab = read_signal_samples(
        &context,
        &Window {
            limit: 50,
            before: None,
            since: Some(stored("2026-08-21T00:00:00Z")),
            until: None,
        },
    )
    .await
    .unwrap();
    let bis = read_signal_samples(
        &context,
        &Window {
            limit: 50,
            before: None,
            since: None,
            until: Some(stored("2026-08-21T00:00:00Z")),
        },
    )
    .await
    .unwrap();

    assert_eq!(ab.len(), 1);
    assert_eq!(ab[0].snr, 3.0);
    assert_eq!(bis.len(), 1);
    assert_eq!(bis[0].snr, 1.0);
}

#[test]
fn a_request_query_carries_both_paging_and_range() {
    // Through the very parser axum uses. `#[serde(flatten)]` and
    // `serde_urlencoded` do not always get along, and the failure would only
    // show as a 400 on a live request — no test here compiles it away.
    let query: crate::telemetry::ListQuery =
        serde_urlencoded::from_str("limit=5&before=42&since=2026-08-21T00:00:00Z")
            .expect("axum parses this query");

    let window = query.window().unwrap();

    assert_eq!(window.limit, 5);
    assert_eq!(window.before, Some(42));
    assert_eq!(window.since.as_deref(), Some("2026-08-21T00:00:00+00:00"));
}
