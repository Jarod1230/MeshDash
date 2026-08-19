//! Tests for the telemetry module, against a real database and a mock node.

use meshdash_core::{
    db::Database,
    event::EventBus,
    link::{self, LinkConfig},
    module::{AppContext, ModuleRegistry},
};
use meshdash_proto::opcode::Response;
use meshdash_transport::mock::{MockTransport, Step};

use super::*;

async fn context_with(script: Vec<Step>) -> AppContext {
    let db = Database::open_in_memory().await.unwrap();
    let events = EventBus::new();
    let (link, _task) = link::spawn(
        MockTransport::new(script),
        LinkConfig::default(),
        events.clone(),
    );
    let context = AppContext { db, events, link };

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

    assert!(read_samples(&context, 10).await.unwrap().is_empty());
}

#[tokio::test]
async fn asks_the_node_and_stores_the_reading() {
    let context = context_with(answers(4_100)).await;

    let reading = read_battery(&context).await.unwrap();
    store_sample(&context, &reading).await.unwrap();

    let samples = read_samples(&context, 10).await.unwrap();
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

    assert_eq!(read_samples(&context, 100).await.unwrap().len(), 5);
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

    let samples = read_samples(&context, 10).await.unwrap();

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

    assert_eq!(read_samples(&context, 3).await.unwrap().len(), 3);
}

#[tokio::test]
async fn samples_as_soon_as_the_node_is_reachable() {
    // Waiting out the interval would leave the curve empty for five minutes
    // after every restart.
    let context = context_with(answers(4_050)).await;

    context.events.publish(AppEvent::NodeConnected);
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let samples = read_samples(&context, 10).await.unwrap();
    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0].millivolts, 4_050);
}

#[tokio::test]
async fn a_silent_node_costs_one_reading_not_the_task() {
    let context = context_with(vec![Step::Drop("silent".into())]).await;

    assert!(read_battery(&context).await.is_err());
    assert!(read_samples(&context, 10).await.unwrap().is_empty());
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

    let samples = read_signal_samples(&context, 10).await.unwrap();
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

    let samples = read_signal_samples(&context, 10).await.unwrap();
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

    assert!(read_signal_samples(&context, 10).await.unwrap().is_empty());
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

    assert!(read_signal_samples(&context, 10).await.unwrap().is_empty());
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

    assert!(read_signal_samples(&context, 10).await.unwrap().is_empty());
}
