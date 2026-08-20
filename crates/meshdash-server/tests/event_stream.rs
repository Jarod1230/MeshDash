//! Exercises the live event stream the way a browser uses it.
//!
//! Runs a real server on an ephemeral port and connects a real WebSocket
//! client. A stand-in would not prove that the upgrade, the authentication
//! handshake and the router all fit together — which is exactly where the
//! mistakes live.

// The `allow-unwrap-in-tests` setting only covers functions marked as tests,
// not the helpers they call. In a test file a panic on a broken assumption is
// the point, so the lint has nothing to catch here.
#![allow(clippy::unwrap_used)]

use std::{net::SocketAddr, time::Duration};

use futures::{SinkExt, StreamExt};
use meshdash_core::{
    config::AuthConfig,
    config::ModuleSettings,
    db::Database,
    event::{AppEvent, EventBus},
    link::{self, LinkConfig},
    module::{AppContext, ModuleRegistry},
};
use meshdash_transport::mock::MockTransport;
use tokio_tungstenite::tungstenite::Message;

/// Starts a server and returns its address plus the bus feeding it.
async fn serve(auth: AuthConfig) -> (SocketAddr, EventBus) {
    let db = Database::open_in_memory().await.unwrap();
    let events = EventBus::new();
    let (link, _task) = link::spawn(
        MockTransport::new(vec![]),
        LinkConfig::default(),
        events.clone(),
    );

    let context = AppContext {
        db,
        events: events.clone(),
        link,
        settings: ModuleSettings::default(),
    };
    let router = meshdash_server::build_router(&ModuleRegistry::new(), context, auth);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    (address, events)
}

/// Opens the event stream.
async fn connect(
    address: SocketAddr,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let url = format!("ws://{address}/api/v1/events");
    let (socket, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    socket
}

/// Demands the given token.
fn demanding(token: &str) -> AuthConfig {
    AuthConfig {
        token: Some(token.to_owned()),
        allow_unauthenticated: false,
    }
}

/// Waits briefly for the next text message.
async fn next_text(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> Option<String> {
    let received = tokio::time::timeout(Duration::from_secs(2), socket.next()).await;

    match received {
        Ok(Some(Ok(Message::Text(text)))) => Some(text.to_string()),
        _ => None,
    }
}

#[tokio::test]
async fn delivers_events_to_a_connected_client() {
    let (address, bus) = serve(AuthConfig::default()).await;
    let mut socket = connect(address).await;

    // Give the server a moment to subscribe before publishing.
    tokio::time::sleep(Duration::from_millis(100)).await;
    bus.publish(AppEvent::NodeConnected);

    let message = next_text(&mut socket).await.expect("expected an event");
    assert_eq!(message, r#"{"type":"node_connected"}"#);
}

#[tokio::test]
async fn delivers_a_push_with_its_payload_as_hex() {
    let (address, bus) = serve(AuthConfig::default()).await;
    let mut socket = connect(address).await;

    tokio::time::sleep(Duration::from_millis(100)).await;
    bus.publish(AppEvent::Push {
        payload: vec![0x83, 0x01],
    });

    let message = next_text(&mut socket).await.expect("expected an event");
    assert_eq!(message, r#"{"type":"push","payload":"8301"}"#);
}

#[tokio::test]
async fn keeps_delivering_more_than_one_event() {
    let (address, bus) = serve(AuthConfig::default()).await;
    let mut socket = connect(address).await;

    tokio::time::sleep(Duration::from_millis(100)).await;
    bus.publish(AppEvent::NodeConnected);
    bus.publish(AppEvent::NodeDisconnected {
        reason: "cable pulled".into(),
    });

    assert_eq!(
        next_text(&mut socket).await.unwrap(),
        r#"{"type":"node_connected"}"#
    );
    assert_eq!(
        next_text(&mut socket).await.unwrap(),
        r#"{"type":"node_disconnected","reason":"cable pulled"}"#
    );
}

#[tokio::test]
async fn streams_after_the_token_is_sent() {
    let (address, bus) = serve(demanding("s3cret")).await;
    let mut socket = connect(address).await;

    socket.send(Message::Text("s3cret".into())).await.unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;
    bus.publish(AppEvent::NodeConnected);

    assert_eq!(
        next_text(&mut socket).await.expect("expected an event"),
        r#"{"type":"node_connected"}"#
    );
}

#[tokio::test]
async fn accepts_the_bearer_spelling_too() {
    // So a caller can send whatever it uses on ordinary requests.
    let (address, bus) = serve(demanding("s3cret")).await;
    let mut socket = connect(address).await;

    socket
        .send(Message::Text("Bearer s3cret".into()))
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;
    bus.publish(AppEvent::NodeConnected);

    assert!(next_text(&mut socket).await.is_some());
}

#[tokio::test]
async fn sends_nothing_before_the_token_arrives() {
    let (address, bus) = serve(demanding("s3cret")).await;
    let mut socket = connect(address).await;

    // Publish without authenticating first.
    tokio::time::sleep(Duration::from_millis(100)).await;
    bus.publish(AppEvent::NodeConnected);

    assert_eq!(
        next_text(&mut socket).await,
        None,
        "an unauthenticated client must not see events"
    );
}

#[tokio::test]
async fn closes_a_connection_that_sends_a_wrong_token() {
    let (address, bus) = serve(demanding("s3cret")).await;
    let mut socket = connect(address).await;

    socket.send(Message::Text("wrong".into())).await.unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;
    bus.publish(AppEvent::NodeConnected);

    assert_eq!(
        next_text(&mut socket).await,
        None,
        "a wrong token must not be served"
    );
}

#[tokio::test]
async fn does_not_expect_a_token_when_none_is_configured() {
    // Only reachable on loopback, which Config::check_exposure enforces.
    let (address, bus) = serve(AuthConfig::default()).await;
    let mut socket = connect(address).await;

    tokio::time::sleep(Duration::from_millis(100)).await;
    bus.publish(AppEvent::NodeConnected);

    assert!(next_text(&mut socket).await.is_some());
}
