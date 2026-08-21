//! What happens *between* modules.
//!
//! Each module's own tests register that module alone, which is right for
//! testing it — and blind to the seam between two of them. This file exists
//! because of a bug that slipped through exactly there: the receiving half of
//! sender resolution was tested and worked, while the sending half was never
//! written. Both suites were green; the feature did nothing.
//!
//! Anything that travels over the bus from one module to another belongs here.

// Same reason as in the server's integration test: `allow-unwrap-in-tests`
// covers functions marked as tests, not the helpers they call, and in a test
// file a panic on a broken assumption is the point.
#![allow(clippy::unwrap_used)]

use meshdash_core::{
    config::ModuleSettings,
    db::Database,
    event::EventBus,
    link::{self, LinkConfig},
    module::{AppContext, ModuleRegistry},
};
use meshdash_modules::{messages, messages::MessagesModule, nodes, nodes::NodesModule};
use meshdash_proto::{contact::Contact, opcode::Response};
use meshdash_transport::mock::{MockTransport, Step};

/// A context with **both** modules running, which is the point.
async fn context_with(script: Vec<Step>) -> AppContext {
    let db = Database::open_in_memory().await.unwrap();
    let events = EventBus::new();
    let (link, task) = link::spawn(
        MockTransport::new(script),
        LinkConfig::default(),
        events.clone(),
    );
    // The link task must outlive this function or the handle goes dead.
    std::mem::forget(task);

    let context = AppContext {
        db,
        events,
        link,
        settings: ModuleSettings::default(),
    };

    let mut registry = ModuleRegistry::new();
    registry.register(Box::new(NodesModule)).unwrap();
    registry.register(Box::new(MessagesModule)).unwrap();
    registry.start_all(&context).await.unwrap();
    context
}

fn contact_frame(key: u8, name: &str) -> Vec<u8> {
    let mut payload = vec![0u8; 148];
    payload[0] = u8::from(Response::Contact);
    payload[1..33].copy_from_slice(&[key; 32]);
    payload[33] = 2;
    payload[35] = 0xFF; // no known route
    payload[100..100 + name.len()].copy_from_slice(name.as_bytes());
    payload
}

fn listing(contacts: Vec<Vec<u8>>) -> Vec<Step> {
    let mut script = vec![Step::AwaitSent(1)];
    let mut start = vec![0u8; 5];
    start[0] = u8::from(Response::ContactsStart);
    start[1..5].copy_from_slice(&(contacts.len() as u32).to_le_bytes());
    script.push(Step::Emit(start));
    for contact in contacts {
        script.push(Step::Emit(contact));
    }
    let mut end = vec![0u8; 5];
    end[0] = u8::from(Response::EndOfContacts);
    script.push(Step::Emit(end));
    script
}

#[tokio::test]
async fn a_contact_fetched_by_nodes_becomes_a_name_in_messages() {
    // The seam: nodes stores a contact and announces it, messages hears the
    // announcement and can then name a sender prefix. Neither module knows
    // the other exists.
    let context = context_with(listing(vec![contact_frame(0xA1, "Repeater Nord")])).await;

    nodes::sync_contacts(&context).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let identity = messages::identify_sender(&context, "a1a1a1a1a1a1")
        .await
        .unwrap();

    assert_eq!(identity.name.as_deref(), Some("Repeater Nord"));
}

#[tokio::test]
async fn a_contact_learned_from_an_advert_also_reaches_messages() {
    // The other way a contact arrives: a long advert carries a whole contact,
    // and it must announce itself just like a fetched one.
    let context = context_with(vec![]).await;

    let contact = Contact {
        public_key: [0xB2; 32],
        contact_type: 2,
        flags: 0,
        path: None,
        name: "Notfunk Ost".into(),
        last_advert: 0,
        latitude: None,
        longitude: None,
        last_modified: 0,
    };
    nodes::store_contact(&context, &contact).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let identity = messages::identify_sender(&context, "b2b2b2b2b2b2")
        .await
        .unwrap();

    assert_eq!(identity.name.as_deref(), Some("Notfunk Ost"));
}

#[tokio::test]
async fn messages_survives_without_the_nodes_module() {
    // Modules must stay individually switchable: without nodes, senders
    // simply stay unnamed.
    let db = Database::open_in_memory().await.unwrap();
    let events = EventBus::new();
    let (link, task) = link::spawn(
        MockTransport::new(vec![]),
        LinkConfig::default(),
        events.clone(),
    );
    std::mem::forget(task);
    let context = AppContext {
        db,
        events,
        link,
        settings: ModuleSettings::default(),
    };

    let mut registry = ModuleRegistry::new();
    registry.register(Box::new(MessagesModule)).unwrap();
    registry.start_all(&context).await.unwrap();

    let identity = messages::identify_sender(&context, "a1a1a1a1a1a1")
        .await
        .unwrap();

    assert_eq!(identity.candidates, 0, "nobody known, and nothing broken");
}
