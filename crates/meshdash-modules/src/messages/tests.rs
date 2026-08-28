//! Tests for the messages module, against a real database and a mock node.

use crate::query::Window;
use meshdash_core::{
    db::Database,
    event::EventBus,
    link::{self, LinkConfig},
    module::{AppContext, ModuleRegistry},
    settings::Settings,
};
use meshdash_transport::mock::{MockTransport, SentFrames, Step};

use meshdash_proto::opcode::Push;

use super::*;

/// How many commands a test sent, without the link's own session start.
fn commands_sent(record: &SentFrames) -> usize {
    record.len().saturating_sub(1)
}

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
    context_and_record_with(script).await.0
}

/// The same, plus a handle on what was sent to the node.
async fn context_and_record_with(script: Vec<Step>) -> (AppContext, SentFrames) {
    let db = Database::open_in_memory().await.unwrap();
    let events = EventBus::new();
    let transport = MockTransport::new(after_session_start(script));
    let record = transport.sent_frames();
    let (link, _task) = link::spawn(transport, LinkConfig::default(), events.clone());
    let context = AppContext {
        db,
        events,
        link,
        settings: Settings::default(),
    };

    let mut registry = ModuleRegistry::new();
    registry.register(Box::new(MessagesModule)).unwrap();
    registry.start_all(&context).await.unwrap();
    (context, record)
}

/// A V3 message frame as the firmware lays it out.
fn message_frame(text: &str) -> Vec<u8> {
    let mut payload = vec![u8::from(Response::ContactMsgRecvV3)];
    payload.push((5.0_f32 * 4.0) as u8);
    payload.extend_from_slice(&[0, 0]);
    payload.extend_from_slice(&[0xAA; 6]);
    payload.push(2);
    payload.push(0);
    payload.extend_from_slice(&1_700_000_000_u32.to_le_bytes());
    payload.extend_from_slice(text.as_bytes());
    payload
}

fn no_more() -> Vec<u8> {
    vec![u8::from(Response::NoMoreMessages)]
}

/// A script answering each sync request in turn.
fn queue(messages: Vec<&str>) -> Vec<Step> {
    let mut script = Vec::new();
    for (index, text) in messages.iter().enumerate() {
        script.push(Step::AwaitSent(index + 1));
        script.push(Step::Emit(message_frame(text)));
    }
    script.push(Step::AwaitSent(messages.len() + 1));
    script.push(Step::Emit(no_more()));
    script.push(Step::Drop("script finished".into()));
    script
}

#[tokio::test]
async fn starts_with_nothing_stored() {
    let context = context_with(vec![]).await;

    assert!(
        read_messages(&context, &Window::paged(500, None), None)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn fetches_until_the_node_has_no_more() {
    let context = context_with(queue(vec!["Erste", "Zweite", "Dritte"])).await;

    let fetched = drain_messages(&context).await.unwrap();

    assert_eq!(fetched, 3);
    assert_eq!(
        read_messages(&context, &Window::paged(500, None), None)
            .await
            .unwrap()
            .len(),
        3
    );
}

#[tokio::test]
async fn stops_at_once_when_nothing_waits() {
    let context = context_with(queue(vec![])).await;

    assert_eq!(drain_messages(&context).await.unwrap(), 0);
}

#[tokio::test]
async fn keeps_what_the_node_reported() {
    let context = context_with(queue(vec!["Hallo Mesh"])).await;
    drain_messages(&context).await.unwrap();

    let messages = read_messages(&context, &Window::paged(500, None), None)
        .await
        .unwrap();

    assert_eq!(messages[0].text, "Hallo Mesh");
    assert_eq!(messages[0].sender_prefix, "aaaaaaaaaaaa");
    assert_eq!(messages[0].snr, Some(5.0));
    assert_eq!(messages[0].path_len, Some(2));
    assert_eq!(messages[0].sent_at, 1_700_000_000);
}

#[tokio::test]
async fn reports_the_newest_first() {
    let context = context_with(queue(vec!["Alt", "Neu"])).await;
    drain_messages(&context).await.unwrap();

    let messages = read_messages(&context, &Window::paged(500, None), None)
        .await
        .unwrap();

    assert_eq!(messages[0].text, "Neu", "newest first");
    assert_eq!(messages[1].text, "Alt");
}

#[tokio::test]
async fn keeps_history_the_node_has_already_forgotten() {
    // Reading empties the node's queue, so what is stored here is the only
    // record left.
    let context = context_with(queue(vec!["Einmalig"])).await;
    drain_messages(&context).await.unwrap();

    assert_eq!(
        read_messages(&context, &Window::paged(500, None), None)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn fetches_when_the_node_rings_the_bell() {
    let context = context_with(queue(vec!["Angekündigt"])).await;

    context.events.publish(AppEvent::Push {
        payload: vec![u8::from(Push::MsgWaiting)],
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    assert_eq!(
        read_messages(&context, &Window::paged(500, None), None)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn ignores_pushes_that_are_not_about_messages() {
    let context = context_with(queue(vec!["Ungelesen"])).await;

    context.events.publish(AppEvent::Push {
        payload: vec![u8::from(Push::Advert)],
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    assert!(
        read_messages(&context, &Window::paged(500, None), None)
            .await
            .unwrap()
            .is_empty(),
        "an advert is none of this module's business"
    );
}

#[tokio::test]
async fn survives_a_node_that_stops_answering() {
    let context = context_with(vec![Step::Drop("silent".into())]).await;

    assert!(drain_messages(&context).await.is_err());
}

/// A node that answers every sync request with the same unexpected frame.
///
/// Long enough that a loop without an exit would keep going well past the
/// point a test could call it an accident.
fn answers_wrongly_forever(answer: Vec<u8>) -> Vec<Step> {
    (0..50)
        .flat_map(|index| vec![Step::AwaitSent(index + 1), Step::Emit(answer.clone())])
        .collect()
}

#[tokio::test]
async fn stops_asking_when_the_node_answers_something_else() {
    // The node replies to CMD_SYNC_NEXT_MESSAGE with neither a message nor
    // "no more". Carrying on means asking again — and again, forever.
    let (context, sent) =
        context_and_record_with(answers_wrongly_forever(vec![u8::from(Response::Ok)])).await;

    assert_eq!(drain_messages(&context).await.unwrap(), 0);
    assert_eq!(commands_sent(&sent), 1, "asked more than once");
}

#[tokio::test]
async fn stops_asking_when_the_node_reports_an_error() {
    let (context, sent) =
        context_and_record_with(answers_wrongly_forever(vec![u8::from(Response::Err), 2])).await;

    assert_eq!(drain_messages(&context).await.unwrap(), 0);
    assert_eq!(commands_sent(&sent), 1, "asked more than once");
}

#[tokio::test]
async fn gives_up_on_a_node_that_never_runs_out_of_messages() {
    // Every answer is a valid message, so nothing above catches this. A
    // firmware stuck redelivering would otherwise hold the drain open.
    let script = (0..(MAX_MESSAGES_PER_DRAIN + 20))
        .flat_map(|index| {
            vec![
                Step::AwaitSent(index + 1),
                Step::Emit(message_frame("Endlos")),
            ]
        })
        .collect();
    let (context, sent) = context_and_record_with(script).await;

    assert_eq!(
        drain_messages(&context).await.unwrap(),
        MAX_MESSAGES_PER_DRAIN
    );
    assert_eq!(commands_sent(&sent), MAX_MESSAGES_PER_DRAIN);
}

/// A V3 channel message as the firmware lays it out.
fn channel_frame(index: u8, text: &str) -> Vec<u8> {
    let mut payload = vec![u8::from(Response::ChannelMsgRecvV3)];
    payload.push((3.0_f32 * 4.0) as u8);
    payload.extend_from_slice(&[0, 0]);
    payload.push(index);
    payload.push(1);
    payload.push(0);
    payload.extend_from_slice(&1_700_000_000_u32.to_le_bytes());
    payload.extend_from_slice(text.as_bytes());
    payload
}

/// A channel description, key included as the node sends it.
fn channel_info_frame(index: u8, name: &str) -> Vec<u8> {
    let mut payload = vec![0u8; 50];
    payload[0] = u8::from(Response::ChannelInfo);
    payload[1] = index;
    payload[2..2 + name.len()].copy_from_slice(name.as_bytes());
    payload[34..50].copy_from_slice(&[0x99; 16]);
    payload
}

/// A receipt for a direct message.
fn receipt_frame() -> Vec<u8> {
    let mut payload = vec![u8::from(Response::Sent), 1];
    payload.extend_from_slice(&0x1234_5678_u32.to_le_bytes());
    payload.extend_from_slice(&3_000_u32.to_le_bytes());
    payload
}

/// A script answering one request with the given frame.
fn one_answer(frame: Vec<u8>) -> Vec<Step> {
    vec![Step::AwaitSent(1), Step::Emit(frame)]
}

#[tokio::test]
async fn drains_channel_messages_from_the_same_queue() {
    // They arrive through CMD_SYNC_NEXT_MESSAGE exactly like direct ones. A
    // drain that only knows direct messages stops dead at the first of these.
    let script = vec![
        Step::AwaitSent(1),
        Step::Emit(channel_frame(2, "Hallo Kanal")),
        Step::AwaitSent(2),
        Step::Emit(no_more()),
    ];
    let context = context_with(script).await;

    assert_eq!(drain_messages(&context).await.unwrap(), 1);

    let messages = read_channel_messages(&context, &Window::paged(500, None), None)
        .await
        .unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].channel_index, 2);
    assert_eq!(messages[0].text, "Hallo Kanal");
    assert_eq!(messages[0].snr, Some(3.0));
}

#[tokio::test]
async fn keeps_channel_and_direct_messages_apart() {
    let script = vec![
        Step::AwaitSent(1),
        Step::Emit(message_frame("Direkt")),
        Step::AwaitSent(2),
        Step::Emit(channel_frame(0, "Im Kanal")),
        Step::AwaitSent(3),
        Step::Emit(no_more()),
    ];
    let context = context_with(script).await;
    drain_messages(&context).await.unwrap();

    assert_eq!(
        read_messages(&context, &Window::paged(500, None), None)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        read_channel_messages(&context, &Window::paged(500, None), None)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn announces_the_reception_quality_of_every_message() {
    // telemetry listens for this; nothing else connects the two modules.
    let script = vec![
        Step::AwaitSent(1),
        Step::Emit(message_frame("Direkt")),
        Step::AwaitSent(2),
        Step::Emit(channel_frame(0, "Kanal")),
        Step::AwaitSent(3),
        Step::Emit(no_more()),
    ];
    let context = context_with(script).await;
    let mut events = context.events.subscribe();

    drain_messages(&context).await.unwrap();

    let mut signals = Vec::new();
    while let Ok(AppEvent::Module { module, kind, data }) = events.try_recv() {
        assert_eq!(module, "messages");
        assert_eq!(kind, "signal");
        signals.push(data);
    }

    assert_eq!(signals.len(), 2);
    assert_eq!(signals[0]["source"], "direct");
    assert_eq!(signals[0]["snr"], 5.0);
    assert_eq!(signals[1]["source"], "channel");
    assert_eq!(signals[1]["snr"], 3.0);
}

#[tokio::test]
async fn reads_the_channel_list_until_the_node_runs_out() {
    let script = vec![
        Step::AwaitSent(1),
        Step::Emit(channel_info_frame(0, "Allgemein")),
        Step::AwaitSent(2),
        Step::Emit(channel_info_frame(1, "Notfunk")),
        Step::AwaitSent(3),
        Step::Emit(vec![u8::from(Response::Err), 2]),
    ];
    let context = context_with(script).await;

    assert_eq!(sync_channels(&context).await.unwrap(), 2);

    let channels = read_channels(&context).await.unwrap();
    assert_eq!(channels.len(), 2);
    assert_eq!(channels[1].name, "Notfunk");
}

#[tokio::test]
async fn never_stores_a_channel_key() {
    let context = context_with(vec![
        Step::AwaitSent(1),
        Step::Emit(channel_info_frame(0, "Allgemein")),
        Step::AwaitSent(2),
        Step::Emit(vec![u8::from(Response::Err), 2]),
    ])
    .await;
    sync_channels(&context).await.unwrap();

    // Whoever holds the key can read and write the channel. It travels in the
    // frame; it must not survive anywhere here.
    let columns: Vec<(String,)> =
        sqlx::query_as("SELECT name FROM pragma_table_info('messages_channels')")
            .fetch_all(context.db.pool())
            .await
            .unwrap();
    let names: Vec<String> = columns.into_iter().map(|row| row.0).collect();

    assert!(
        !names
            .iter()
            .any(|name| name.contains("key") || name.contains("secret"))
    );
    assert_eq!(names, vec!["channel_index", "name", "seen_at"]);
}

#[tokio::test]
async fn sends_a_direct_message_and_reports_the_receipt() {
    let context = context_with(one_answer(receipt_frame())).await;

    let result = send_message(&context, [0xAA; 6], "Moin").await.unwrap();

    assert!(result.flooded);
    assert_eq!(result.expected_ack.as_deref(), Some("12345678"));
    assert_eq!(result.estimated_timeout_ms, 3_000);
}

#[tokio::test]
async fn records_what_was_sent() {
    let context = context_with(one_answer(receipt_frame())).await;
    send_message(&context, [0xAA; 6], "Moin").await.unwrap();

    let rows: Vec<(String, String)> = sqlx::query_as("SELECT target, text FROM messages_sent")
        .fetch_all(context.db.pool())
        .await
        .unwrap();

    assert_eq!(rows, vec![("aaaaaaaaaaaa".to_string(), "Moin".to_string())]);
}

#[tokio::test]
async fn sends_to_a_channel_without_waiting_for_a_receipt() {
    // A broadcast is not acknowledged; the node answers with a plain OK.
    let context = context_with(one_answer(vec![u8::from(Response::Ok)])).await;

    assert!(send_channel_message(&context, 2, "Hallo").await.is_ok());

    let rows: Vec<(String,)> = sqlx::query_as("SELECT target FROM messages_sent")
        .fetch_all(context.db.pool())
        .await
        .unwrap();
    assert_eq!(rows[0].0, "channel:2");
}

#[tokio::test]
async fn passes_on_a_refusal_from_the_node() {
    let context = context_with(one_answer(vec![u8::from(Response::Err), 3])).await;

    let error = send_message(&context, [0xAA; 6], "Moin").await.unwrap_err();

    assert!(matches!(error, SendFailure::NodeRefused { code: Some(3) }));
    // Nothing went out, so nothing is recorded as sent.
    let rows: Vec<(i64,)> = sqlx::query_as("SELECT COUNT(*) FROM messages_sent")
        .fetch_all(context.db.pool())
        .await
        .unwrap();
    assert_eq!(rows[0].0, 0);
}

#[tokio::test]
async fn refuses_an_empty_message_before_asking_the_node() {
    let context = context_with(vec![]).await;

    let error = send_message(&context, [0xAA; 6], "").await.unwrap_err();

    assert!(matches!(error, SendFailure::Rejected(_)));
}

#[tokio::test]
async fn a_listing_never_returns_more_than_asked_for() {
    // Both tables grow with every message and nothing prunes them; an
    // unbounded read would eventually serialise a year of traffic at once.
    let mut script = Vec::new();
    for index in 0..10 {
        script.push(Step::AwaitSent(index + 1));
        script.push(Step::Emit(message_frame("Viele")));
    }
    script.push(Step::AwaitSent(11));
    script.push(Step::Emit(no_more()));
    let context = context_with(script).await;
    drain_messages(&context).await.unwrap();

    assert_eq!(
        read_messages(&context, &Window::paged(3, None), None)
            .await
            .unwrap()
            .len(),
        3
    );
}

#[tokio::test]
async fn a_channel_listing_is_bounded_too() {
    let mut script = Vec::new();
    for index in 0..10 {
        script.push(Step::AwaitSent(index + 1));
        script.push(Step::Emit(channel_frame(0, "Viele")));
    }
    script.push(Step::AwaitSent(11));
    script.push(Step::Emit(no_more()));
    let context = context_with(script).await;
    drain_messages(&context).await.unwrap();

    assert_eq!(
        read_channel_messages(&context, &Window::paged(4, None), None)
            .await
            .unwrap()
            .len(),
        4
    );
}

#[tokio::test]
async fn a_bounded_listing_still_starts_at_the_newest() {
    let script = vec![
        Step::AwaitSent(1),
        Step::Emit(message_frame("Alt")),
        Step::AwaitSent(2),
        Step::Emit(message_frame("Neu")),
        Step::AwaitSent(3),
        Step::Emit(no_more()),
    ];
    let context = context_with(script).await;
    drain_messages(&context).await.unwrap();

    let messages = read_messages(&context, &Window::paged(1, None), None)
        .await
        .unwrap();
    assert_eq!(messages[0].text, "Neu", "the limit must cut the old end");
}

#[test]
fn caps_what_a_request_may_ask_for() {
    assert_eq!(ListQuery::default().effective_limit(), DEFAULT_LIMIT);
    assert_eq!(
        ListQuery {
            limit: Some(999_999),
            ..Default::default()
        }
        .effective_limit(),
        MAX_LIMIT
    );
    // Zero or negative would return nothing at all, which reads as "no
    // messages" rather than as a bad request.
    assert_eq!(
        ListQuery {
            limit: Some(0),
            ..Default::default()
        }
        .effective_limit(),
        1
    );
    assert_eq!(
        ListQuery {
            limit: Some(-5),
            ..Default::default()
        }
        .effective_limit(),
        1
    );
}

/// The announcement the nodes module publishes for each contact.
fn contact_announcement(key: u8, name: &str) -> AppEvent {
    AppEvent::Module {
        module: "nodes".into(),
        kind: "contact".into(),
        data: serde_json::json!({
            "public_key": format!("{key:02x}").repeat(32),
            "name": name,
        }),
    }
}

#[tokio::test]
async fn learns_sender_names_from_the_nodes_module() {
    // It cannot read that module's tables, so the bus is the only way.
    let context = context_with(vec![]).await;

    context
        .events
        .publish(contact_announcement(0xAA, "Repeater Nord"));
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let identity = identify_sender(&context, "aaaaaaaaaaaa").await.unwrap();

    assert_eq!(identity.name.as_deref(), Some("Repeater Nord"));
    assert_eq!(identity.candidates, 1);
}

#[tokio::test]
async fn puts_the_name_on_a_received_message() {
    let context = context_with(queue(vec!["Hallo"])).await;
    context
        .events
        .publish(contact_announcement(0xAA, "Repeater Nord"));
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    drain_messages(&context).await.unwrap();

    let messages = read_messages(&context, &Window::paged(10, None), None)
        .await
        .unwrap();

    assert_eq!(messages[0].sender_name.as_deref(), Some("Repeater Nord"));
    assert_eq!(messages[0].sender_candidates, 1);
}

#[tokio::test]
async fn shows_no_name_when_two_contacts_share_a_prefix() {
    // Six bytes can collide. Picking one would be a coin toss presented as
    // fact — and on a mesh where messages carry instructions, attributing one
    // to the wrong person is worse than showing a hex prefix.
    let context = context_with(vec![]).await;

    let same_prefix = |suffix: &str, name: &str| AppEvent::Module {
        module: "nodes".into(),
        kind: "contact".into(),
        data: serde_json::json!({
            "public_key": format!("aaaaaaaaaaaa{suffix}"),
            "name": name,
        }),
    };
    context
        .events
        .publish(same_prefix(&"11".repeat(26), "Erster"));
    context
        .events
        .publish(same_prefix(&"22".repeat(26), "Zweiter"));
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let identity = identify_sender(&context, "aaaaaaaaaaaa").await.unwrap();

    assert_eq!(identity.name, None, "no coin toss");
    assert_eq!(identity.candidates, 2, "but the interface can say why");
}

#[tokio::test]
async fn an_unknown_prefix_is_simply_unknown() {
    let context = context_with(vec![]).await;

    let identity = identify_sender(&context, "ffffffffffff").await.unwrap();

    assert_eq!(identity.name, None);
    assert_eq!(identity.candidates, 0);
}

#[tokio::test]
async fn a_renamed_contact_keeps_one_entry() {
    // Nodes rename themselves; the key is what identifies them, not the name.
    let context = context_with(vec![]).await;

    context
        .events
        .publish(contact_announcement(0xBB, "Alter Name"));
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    context
        .events
        .publish(contact_announcement(0xBB, "Neuer Name"));
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let identity = identify_sender(&context, "bbbbbbbbbbbb").await.unwrap();

    assert_eq!(identity.name.as_deref(), Some("Neuer Name"));
    assert_eq!(identity.candidates, 1, "not two rows for one contact");
}

#[tokio::test]
async fn ignores_an_announcement_it_cannot_use() {
    let context = context_with(vec![]).await;

    context.events.publish(AppEvent::Module {
        module: "nodes".into(),
        kind: "contact".into(),
        data: serde_json::json!({ "name": "ohne Schlüssel" }),
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Nothing stored, nothing broken.
    assert_eq!(
        identify_sender(&context, "aaaaaaaaaaaa")
            .await
            .unwrap()
            .candidates,
        0
    );
}

/// A receipt for a sent direct message.
fn sent_receipt() -> Vec<u8> {
    let mut payload = vec![u8::from(Response::Sent), 1];
    payload.extend_from_slice(&0x1234_u32.to_le_bytes());
    payload.extend_from_slice(&3_000_u32.to_le_bytes());
    payload
}

#[tokio::test]
async fn a_conversation_interleaves_what_came_and_what_went() {
    // Two separate lists cannot show that an answer followed a question.
    let mut script = queue(vec!["Frage von draußen"]);
    script.push(Step::AwaitSent(3));
    script.push(Step::Emit(sent_receipt()));
    let context = context_with(script).await;

    drain_messages(&context).await.unwrap();
    send_message(&context, [0xAA; 6], "Antwort von hier")
        .await
        .unwrap();

    let faden = read_conversation(&context, Partner::Contact, "aaaaaaaaaaaa", 50)
        .await
        .unwrap();

    assert_eq!(faden.len(), 2);
    assert_eq!(faden[0].direction, Direction::Received, "oldest first");
    assert_eq!(faden[0].text, "Frage von draußen");
    assert_eq!(faden[1].direction, Direction::Sent);
    assert_eq!(faden[1].text, "Antwort von hier");
}

#[tokio::test]
async fn a_received_message_keeps_its_reception_quality_in_the_thread() {
    let context = context_with(queue(vec!["Mit Signal"])).await;
    drain_messages(&context).await.unwrap();

    let faden = read_conversation(&context, Partner::Contact, "aaaaaaaaaaaa", 50)
        .await
        .unwrap();

    assert_eq!(faden[0].snr, Some(5.0));
    assert_eq!(
        faden[0].flooded, None,
        "that field belongs to sent messages"
    );
}

#[tokio::test]
async fn a_sent_message_keeps_whether_it_was_flooded() {
    let context = context_with(vec![Step::AwaitSent(1), Step::Emit(sent_receipt())]).await;
    send_message(&context, [0xBB; 6], "Hinaus").await.unwrap();

    let faden = read_conversation(&context, Partner::Contact, "bbbbbbbbbbbb", 50)
        .await
        .unwrap();

    assert_eq!(faden[0].flooded, Some(true));
    assert_eq!(faden[0].snr, None, "nothing was received here");
}

#[tokio::test]
async fn channels_are_conversations_too() {
    let mut script = vec![
        Step::AwaitSent(1),
        Step::Emit(channel_frame(2, "Im Kanal")),
        Step::AwaitSent(2),
        Step::Emit(no_more()),
        Step::AwaitSent(3),
        Step::Emit(vec![u8::from(Response::Ok)]),
    ];
    script.push(Step::Drop("fertig".into()));
    let context = context_with(script).await;

    drain_messages(&context).await.unwrap();
    send_channel_message(&context, 2, "Auch hinein")
        .await
        .unwrap();

    let faden = read_conversation(&context, Partner::Channel, "2", 50)
        .await
        .unwrap();

    assert_eq!(faden.len(), 2);
    assert_eq!(faden[1].text, "Auch hinein");
}

#[tokio::test]
async fn the_overview_lists_every_partner_once() {
    let mut script = vec![
        Step::AwaitSent(1),
        Step::Emit(message_frame("Von A")),
        Step::AwaitSent(2),
        Step::Emit(channel_frame(0, "Im Kanal")),
        Step::AwaitSent(3),
        Step::Emit(no_more()),
    ];
    script.push(Step::Drop("fertig".into()));
    let context = context_with(script).await;
    drain_messages(&context).await.unwrap();

    let gespraeche = read_conversations(&context, 50).await.unwrap();

    assert_eq!(gespraeche.len(), 2, "one contact, one channel");
    assert!(gespraeche.iter().any(|g| g.partner == Partner::Channel));
    assert!(gespraeche.iter().any(|g| g.partner == Partner::Contact));
}

#[tokio::test]
async fn the_overview_shows_the_latest_message_of_each_partner() {
    let script = vec![
        Step::AwaitSent(1),
        Step::Emit(message_frame("Alt")),
        Step::AwaitSent(2),
        Step::Emit(message_frame("Neu")),
        Step::AwaitSent(3),
        Step::Emit(no_more()),
    ];
    let context = context_with(script).await;
    drain_messages(&context).await.unwrap();

    let gespraeche = read_conversations(&context, 50).await.unwrap();

    assert_eq!(gespraeche.len(), 1, "same partner, one entry");
    assert_eq!(gespraeche[0].last_text, "Neu");
    assert_eq!(gespraeche[0].messages, 2);
}

#[tokio::test]
async fn a_partner_written_to_but_never_heard_from_still_appears() {
    // Someone you wrote to is a conversation, even before they answer.
    let context = context_with(vec![Step::AwaitSent(1), Step::Emit(sent_receipt())]).await;
    send_message(&context, [0xCC; 6], "Erstkontakt")
        .await
        .unwrap();

    let gespraeche = read_conversations(&context, 50).await.unwrap();

    assert_eq!(gespraeche.len(), 1);
    assert_eq!(gespraeche[0].last_direction, Direction::Sent);
}

#[tokio::test]
async fn a_conversation_carries_the_partners_name_when_it_is_known() {
    let context = context_with(queue(vec!["Hallo"])).await;
    context
        .events
        .publish(contact_announcement(0xAA, "Repeater Nord"));
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    drain_messages(&context).await.unwrap();

    let gespraeche = read_conversations(&context, 50).await.unwrap();

    assert_eq!(gespraeche[0].name.as_deref(), Some("Repeater Nord"));
    assert_eq!(gespraeche[0].candidates, 1);
}

#[tokio::test]
async fn a_conversation_carries_the_key_needed_to_link_to_the_node() {
    // A six-byte prefix does not address a node; a page about one needs the
    // full key.
    let context = context_with(queue(vec!["Hallo"])).await;
    context
        .events
        .publish(contact_announcement(0xAA, "Repeater Nord"));
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    drain_messages(&context).await.unwrap();

    let gespraeche = read_conversations(&context, 50).await.unwrap();

    assert_eq!(
        gespraeche[0].public_key.as_deref(),
        Some("aa".repeat(32)).as_deref()
    );
}

#[tokio::test]
async fn an_ambiguous_prefix_gets_no_key_either() {
    // Linking to a guess would open a page about the wrong node.
    let context = context_with(queue(vec!["Hallo"])).await;
    for suffix in ["11", "22"] {
        context.events.publish(AppEvent::Module {
            module: "nodes".into(),
            kind: "contact".into(),
            data: serde_json::json!({
                "public_key": format!("aaaaaaaaaaaa{}", suffix.repeat(26)),
                "name": format!("Knoten {suffix}"),
            }),
        });
    }
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    drain_messages(&context).await.unwrap();

    let gespraeche = read_conversations(&context, 50).await.unwrap();

    assert_eq!(gespraeche[0].public_key, None);
    assert_eq!(gespraeche[0].candidates, 2);
}

#[tokio::test]
async fn a_cursor_continues_where_the_page_ended() {
    let context = context_with(queue(vec!["Erste", "Zweite", "Dritte"])).await;
    drain_messages(&context).await.unwrap();

    let erste_seite = read_messages(&context, &Window::paged(2, None), None)
        .await
        .unwrap();
    let zweite_seite = read_messages(&context, &Window::paged(2, Some(erste_seite[1].id)), None)
        .await
        .unwrap();

    assert_eq!(
        erste_seite.iter().map(|m| &m.text).collect::<Vec<_>>(),
        vec!["Dritte", "Zweite"],
        "newest first"
    );
    assert_eq!(
        zweite_seite.iter().map(|m| &m.text).collect::<Vec<_>>(),
        vec!["Erste"],
        "the cursor must exclude what the first page already showed"
    );
}

#[tokio::test]
async fn a_search_narrows_the_messages_to_matching_text() {
    let context = context_with(queue(vec![
        "Antenne montiert",
        "Akku leer",
        "antenne getauscht",
    ]))
    .await;
    drain_messages(&context).await.unwrap();

    let found = read_messages(&context, &Window::paged(500, None), Some("antenne"))
        .await
        .unwrap();

    assert_eq!(found.len(), 2, "case must not decide who matches");
    assert!(
        found
            .iter()
            .all(|message| message.text.to_lowercase().contains("antenne"))
    );
}

#[tokio::test]
async fn a_search_takes_wildcards_literally() {
    // Someone searching for a percent sign means a percent sign. Passed
    // through to LIKE it would match every message instead.
    let context = context_with(queue(vec!["Akku 80% voll", "Antenne montiert"])).await;
    drain_messages(&context).await.unwrap();

    let found = read_messages(&context, &Window::paged(500, None), Some("80%"))
        .await
        .unwrap();

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].text, "Akku 80% voll");
}

#[tokio::test]
async fn a_search_also_finds_a_sender_prefix() {
    let context = context_with(queue(vec!["Hallo"])).await;
    drain_messages(&context).await.unwrap();

    let found = read_messages(&context, &Window::paged(500, None), Some("aaaaaa"))
        .await
        .unwrap();

    assert_eq!(
        found.len(),
        1,
        "the prefix is what a message is filed under"
    );
}
