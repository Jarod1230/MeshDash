//! Checks that the alerts module is reachable where the interface looks for it.
//!
//! The module's own tests call its functions directly; this is about the
//! mounting — method, path and the key in it.

// See tiles_routes.rs: a panic on a broken assumption is the point here.
#![allow(clippy::unwrap_used)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use meshdash_core::{
    config::{AuthConfig, ModuleSettings},
    db::Database,
    event::EventBus,
    link::{self, LinkConfig},
    module::{AppContext, ModuleRegistry},
    settings::Settings,
};
use meshdash_modules::alerts::AlertsModule;
use meshdash_transport::mock::MockTransport;
use tower::ServiceExt;

const KEY: &str = "fb074f360c483adf77f24f46b8f7e06b970d9e7a8b2c0f409e140fe9c3e13e05";

async fn router() -> axum::Router {
    let db = Database::open_in_memory().await.unwrap();
    let events = EventBus::new();
    let (link, _task) = link::spawn(
        MockTransport::new(vec![]),
        LinkConfig::default(),
        events.clone(),
    );

    let context = AppContext {
        db,
        events,
        link,
        settings: Settings::from_file(ModuleSettings::default()),
    };

    let mut registry = ModuleRegistry::new();
    registry.register(Box::new(AlertsModule)).unwrap();
    registry.start_all(&context).await.unwrap();

    meshdash_server::build_router(&registry, context, AuthConfig::default())
}

async fn call(router: &axum::Router, method: &str, path: &str) -> (StatusCode, String) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();

    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn a_node_can_be_watched_and_let_go() {
    let router = router().await;
    let path = format!("/api/v1/alerts/watched/{KEY}");

    let (status, body) = call(&router, "PUT", &path).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");

    let (status, body) = call(&router, "GET", "/api/v1/alerts/watched").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains(KEY), "{body}");

    let (status, body) = call(&router, "DELETE", &path).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");

    let (_, body) = call(&router, "GET", "/api/v1/alerts/watched").await;
    assert_eq!(body, "[]");
}

#[tokio::test]
async fn a_prefix_cannot_be_watched() {
    let router = router().await;

    let (status, body) = call(&router, "PUT", "/api/v1/alerts/watched/fb07").await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.contains("not_a_public_key"), "{body}");
}

#[tokio::test]
async fn the_log_starts_empty() {
    let router = router().await;

    let (status, body) = call(&router, "GET", "/api/v1/alerts/log").await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, "[]");
}
