//! Tests for the messages module, against a real database and a mock node.

use meshdash_core::{
    db::Database,
    event::EventBus,
    link::{self, LinkConfig},
    module::{AppContext, ModuleRegistry},
};
use meshdash_transport::mock::{MockTransport, SentFrames, Step};

use super::*;

async fn context_with(script: Vec<Step>) -> AppContext {
    context_and_record_with(script).await.0
}

/// The same, plus a handle on what was sent to the node.
async fn context_and_record_with(script: Vec<Step>) -> (AppContext, SentFrames) {
    let db = Database::open_in_memory().await.unwrap();
    let events = EventBus::new();
    let transport = MockTransport::new(script);
    let record = transport.sent_frames();
    let (link, _task) = link::spawn(transport, LinkConfig::default(), events.clone());
    let context = AppContext { db, events, link };

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

    assert!(read_messages(&context).await.unwrap().is_empty());
}

#[tokio::test]
async fn fetches_until_the_node_has_no_more() {
    let context = context_with(queue(vec!["Erste", "Zweite", "Dritte"])).await;

    let fetched = drain_messages(&context).await.unwrap();

    assert_eq!(fetched, 3);
    assert_eq!(read_messages(&context).await.unwrap().len(), 3);
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

    let messages = read_messages(&context).await.unwrap();

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

    let messages = read_messages(&context).await.unwrap();

    assert_eq!(messages[0].text, "Neu", "newest first");
    assert_eq!(messages[1].text, "Alt");
}

#[tokio::test]
async fn keeps_history_the_node_has_already_forgotten() {
    // Reading empties the node's queue, so what is stored here is the only
    // record left.
    let context = context_with(queue(vec!["Einmalig"])).await;
    drain_messages(&context).await.unwrap();

    assert_eq!(read_messages(&context).await.unwrap().len(), 1);
}

#[tokio::test]
async fn fetches_when_the_node_rings_the_bell() {
    let context = context_with(queue(vec!["Angekündigt"])).await;

    context.events.publish(AppEvent::Push {
        payload: vec![u8::from(Push::MsgWaiting)],
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    assert_eq!(read_messages(&context).await.unwrap().len(), 1);
}

#[tokio::test]
async fn ignores_pushes_that_are_not_about_messages() {
    let context = context_with(queue(vec!["Ungelesen"])).await;

    context.events.publish(AppEvent::Push {
        payload: vec![u8::from(Push::Advert)],
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    assert!(
        read_messages(&context).await.unwrap().is_empty(),
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
    assert_eq!(sent.len(), 1, "asked more than once");
}

#[tokio::test]
async fn stops_asking_when_the_node_reports_an_error() {
    let (context, sent) =
        context_and_record_with(answers_wrongly_forever(vec![u8::from(Response::Err), 2])).await;

    assert_eq!(drain_messages(&context).await.unwrap(), 0);
    assert_eq!(sent.len(), 1, "asked more than once");
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
    assert_eq!(sent.len(), MAX_MESSAGES_PER_DRAIN);
}
