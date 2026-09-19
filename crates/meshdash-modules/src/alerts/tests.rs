//! Tests for the alerts module.
//!
//! Against a real in-memory database and the real push decoding. Time is
//! passed in rather than waited for: a grace of a day is not something a test
//! sits through.

use meshdash_core::{
    config::ModuleSettings, db::Database, event::EventBus, link,
    settings::Settings as RuntimeSettings,
};
use meshdash_transport::mock::{MockTransport, Step};

use super::*;

async fn context() -> AppContext {
    let db = Database::open_in_memory().await.unwrap();
    let events = EventBus::new();
    let (handle, _task) = link::spawn(
        MockTransport::new(vec![Step::Drop("no node needed here".into())]),
        link::LinkConfig::default(),
        events.clone(),
    );

    let context = AppContext {
        db,
        events,
        link: handle,
        settings: RuntimeSettings::from_file(ModuleSettings::default()),
    };

    // Migrations only; the loops are driven by hand below.
    context.db.migrate("alerts", MIGRATIONS).await.unwrap();

    context
}

fn at(text: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(text)
        .unwrap()
        .with_timezone(&Utc)
}

const REPEATER: &str = "fb074f360c483adf77f24f46b8f7e06b970d9e7a8b2c0f409e140fe9c3e13e05";

fn repeater_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    for (index, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&REPEATER[index * 2..index * 2 + 2], 16).unwrap();
    }
    key
}

fn connected() -> LinkState {
    LinkState {
        gone_since: Mutex::new(None),
    }
}

async fn open(context: &AppContext) -> Vec<Alert> {
    read_alerts(context, 100)
        .await
        .unwrap()
        .into_iter()
        .filter(|alert| alert.cleared_at.is_none())
        .collect()
}

#[tokio::test]
async fn a_watched_node_that_stays_silent_is_raised_once() {
    let context = context().await;
    let settings = Settings::default();
    watch(&context, REPEATER, at("2026-09-18T12:00:00Z"))
        .await
        .unwrap();

    // Within the day: nothing.
    evaluate(
        &context,
        &settings,
        &connected(),
        at("2026-09-19T11:00:00Z"),
    )
    .await
    .unwrap();
    assert!(open(&context).await.is_empty());

    // Past it: one warning, however often it is looked at.
    for now in ["2026-09-19T13:00:00Z", "2026-09-19T14:00:00Z"] {
        evaluate(&context, &settings, &connected(), at(now))
            .await
            .unwrap();
    }
    let raised = open(&context).await;
    assert_eq!(raised.len(), 1);
    assert_eq!(raised[0].kind, SILENT);
    assert_eq!(raised[0].subject, REPEATER);
    // Silent since it was watched — nothing was heard before that.
    assert_eq!(raised[0].since, at("2026-09-18T12:00:00Z"));
}

#[tokio::test]
async fn an_advert_ends_the_silence() {
    let context = context().await;
    watch(&context, REPEATER, at("2026-09-17T12:00:00Z"))
        .await
        .unwrap();
    evaluate(
        &context,
        &Settings::default(),
        &connected(),
        at("2026-09-19T12:00:00Z"),
    )
    .await
    .unwrap();
    assert_eq!(open(&context).await.len(), 1);

    heard(&context, REPEATER, at("2026-09-19T12:05:00Z"))
        .await
        .unwrap();

    assert!(open(&context).await.is_empty());
    let all = read_alerts(&context, 100).await.unwrap();
    assert_eq!(all[0].cleared_at, Some(at("2026-09-19T12:05:00Z")));
}

#[tokio::test]
async fn silence_is_measured_from_the_last_advert() {
    let context = context().await;
    watch(&context, REPEATER, at("2026-09-10T00:00:00Z"))
        .await
        .unwrap();
    heard(&context, REPEATER, at("2026-09-19T08:00:00Z"))
        .await
        .unwrap();

    evaluate(
        &context,
        &Settings::default(),
        &connected(),
        at("2026-09-19T20:00:00Z"),
    )
    .await
    .unwrap();

    assert!(open(&context).await.is_empty());
}

#[tokio::test]
async fn nodes_nobody_watches_never_warn() {
    let context = context().await;
    heard(&context, REPEATER, at("2026-09-01T00:00:00Z"))
        .await
        .unwrap();

    evaluate(
        &context,
        &Settings::default(),
        &connected(),
        at("2026-09-19T00:00:00Z"),
    )
    .await
    .unwrap();

    assert!(open(&context).await.is_empty());
    assert!(read_watched(&context).await.unwrap().is_empty());
}

#[tokio::test]
async fn unwatching_ends_the_warning() {
    let context = context().await;
    watch(&context, REPEATER, at("2026-09-17T00:00:00Z"))
        .await
        .unwrap();
    evaluate(
        &context,
        &Settings::default(),
        &connected(),
        at("2026-09-19T00:00:00Z"),
    )
    .await
    .unwrap();

    unwatch(&context, REPEATER, at("2026-09-19T01:00:00Z"))
        .await
        .unwrap();

    assert!(open(&context).await.is_empty());
}

#[tokio::test]
async fn an_advert_push_counts_as_heard() {
    let context = context().await;
    watch(&context, REPEATER, at("2026-09-19T00:00:00Z"))
        .await
        .unwrap();

    // PUSH_CODE_ADVERT: opcode 0x80, then the key.
    let mut payload = vec![0x80];
    payload.extend_from_slice(&repeater_key());
    handle_event(&context, &connected(), &AppEvent::Push { payload })
        .await
        .unwrap();

    let watched = read_watched(&context).await.unwrap();
    assert!(watched[0].last_heard_at.is_some());
}

#[tokio::test]
async fn a_heard_advert_packet_counts_as_heard() {
    let context = context().await;
    watch(&context, REPEATER, at("2026-09-19T00:00:00Z"))
        .await
        .unwrap();

    // PUSH_CODE_LOG_RX_DATA with a zero-hop advert inside.
    let mut payload = vec![0x88, 0x10, 0xA0, (4 << 2) | 2, 0x00];
    payload.extend_from_slice(&repeater_key());
    payload.extend_from_slice(&[0x11; 4 + 64 + 3]);
    handle_event(&context, &connected(), &AppEvent::Push { payload })
        .await
        .unwrap();

    let watched = read_watched(&context).await.unwrap();
    assert!(watched[0].last_heard_at.is_some());
}

#[tokio::test]
async fn a_path_prefix_is_not_a_sign_of_life() {
    // A flooded text over a station whose first byte matches the watched
    // node. With one byte that could be anyone.
    let context = context().await;
    watch(&context, REPEATER, at("2026-09-19T00:00:00Z"))
        .await
        .unwrap();

    let payload = vec![0x88, 0x10, 0xA0, 0b0000_1001, 0x01, 0xFB, 0xDE, 0xAD];
    handle_event(&context, &connected(), &AppEvent::Push { payload })
        .await
        .unwrap();

    assert!(
        read_watched(&context).await.unwrap()[0]
            .last_heard_at
            .is_none()
    );
}

#[tokio::test]
async fn a_node_gone_for_minutes_is_raised_and_ends_on_reconnect() {
    let context = context().await;
    let link = LinkState::gone_since(at("2026-09-19T12:00:00Z"));

    evaluate(
        &context,
        &Settings::default(),
        &link,
        at("2026-09-19T12:03:00Z"),
    )
    .await
    .unwrap();
    assert!(
        open(&context).await.is_empty(),
        "a short gap is a reconnect"
    );

    evaluate(
        &context,
        &Settings::default(),
        &link,
        at("2026-09-19T12:06:00Z"),
    )
    .await
    .unwrap();
    let raised = open(&context).await;
    assert_eq!(raised.len(), 1);
    assert_eq!(raised[0].kind, DISCONNECTED);
    assert!(raised[0].subject.is_empty());

    handle_event(&context, &link, &AppEvent::NodeConnected)
        .await
        .unwrap();
    assert!(open(&context).await.is_empty());
}

#[tokio::test]
async fn raising_and_clearing_go_out_on_the_bus() {
    let context = context().await;
    let mut events = context.events.subscribe();
    watch(&context, REPEATER, at("2026-09-17T00:00:00Z"))
        .await
        .unwrap();

    evaluate(
        &context,
        &Settings::default(),
        &connected(),
        at("2026-09-19T00:00:00Z"),
    )
    .await
    .unwrap();
    heard(&context, REPEATER, at("2026-09-19T00:01:00Z"))
        .await
        .unwrap();

    let mut kinds = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let AppEvent::Module { module, kind, data } = event {
            assert_eq!(module, "alerts");
            assert_eq!(data["subject"], REPEATER);
            kinds.push(kind);
        }
    }
    assert_eq!(kinds, ["raised", "cleared"]);
}

#[test]
fn only_a_full_key_can_be_watched() {
    assert!(is_public_key(REPEATER));
    assert!(!is_public_key("fb07"));
    assert!(!is_public_key(&REPEATER.to_uppercase()));
    assert!(!is_public_key(&format!("{}zz", &REPEATER[..62])));
}
