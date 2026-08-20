//! Tests for the nodes module, against a real database and a mock node.

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

/// Builds a context whose node replies with the given script.
async fn context_with(script: Vec<Step>) -> AppContext {
    let db = Database::open_in_memory().await.unwrap();
    let events = EventBus::new();
    let (link, _task) = link::spawn(
        MockTransport::new(script),
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

    let sightings = read_adverts(&context).await.unwrap();
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
    assert_eq!(read_adverts(&context).await.unwrap().len(), 1);
    assert!(read_contacts(&context).await.unwrap().is_empty());
}

#[tokio::test]
async fn ignores_pushes_that_are_not_adverts() {
    let context = context_with(vec![]).await;

    push(&context, vec![0x83, 0x00]).await;

    assert!(read_adverts(&context).await.unwrap().is_empty());
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
