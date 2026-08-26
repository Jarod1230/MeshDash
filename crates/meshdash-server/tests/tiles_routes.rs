//! Checks that the tiles module is reachable where the map looks for it.
//!
//! Its own unit tests call the handlers directly, which proves what they do
//! and nothing about where they hang. Mounting is exactly the kind of thing
//! that is right in the module and wrong in the router — a nested `/` route
//! is a well-known place to lose a trailing slash.

// The `allow-unwrap-in-tests` setting only covers functions marked as tests,
// not the helpers they call. In a test file a panic on a broken assumption is
// the point, so the lint has nothing to catch here.
#![allow(clippy::unwrap_used)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use meshdash_core::{
    config::{AuthConfig, ModuleSettings},
    db::Database,
    event::EventBus,
    link::{self, LinkConfig},
    module::{AppContext, ModuleRegistry},
};
use meshdash_modules::tiles::TilesModule;
use meshdash_transport::mock::MockTransport;
use tower::ServiceExt;

/// A router with the tiles module in it, configured as the test wants.
async fn router(settings: serde_json::Value) -> axum::Router {
    let db = Database::open_in_memory().await.unwrap();
    let events = EventBus::new();
    let (link, _task) = link::spawn(
        MockTransport::new(vec![]),
        LinkConfig::default(),
        events.clone(),
    );

    let mut module_settings = ModuleSettings::default();
    module_settings.set("tiles", settings);

    let context = AppContext {
        db,
        events,
        link,
        settings: module_settings,
    };

    let mut registry = ModuleRegistry::new();
    registry.register(Box::new(TilesModule::default())).unwrap();
    registry.start_all(&context).await.unwrap();

    meshdash_server::build_router(&registry, context, AuthConfig::default())
}

/// Asks the router one question.
async fn get(router: &axum::Router, path: &str) -> (StatusCode, String) {
    let response = router
        .clone()
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();

    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();

    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn the_map_can_ask_whether_there_are_tiles() {
    let router = router(serde_json::json!({})).await;

    let (status, body) = get(&router, "/api/v1/tiles").await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("\"available\":false"), "{body}");
}

#[tokio::test]
async fn a_tile_without_a_source_is_a_plain_no() {
    let router = router(serde_json::json!({})).await;

    let (status, body) = get(&router, "/api/v1/tiles/10/550/335").await;

    // Not a broken image and not a 500: there is simply no source, and the
    // map is written to draw without one.
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.contains("no_tile_source"), "{body}");
}

#[tokio::test]
async fn coordinates_that_cannot_be_a_place_are_refused_at_the_door() {
    let router = router(serde_json::json!({
        "source": "https://example.invalid/{z}/{x}/{y}.png",
        "attribution": "Ein Kachelserver",
        "cache_dir": std::env::temp_dir().join("meshdash-tiles-routes"),
    }))
    .await;

    let (status, _) = get(&router, "/api/v1/tiles/2/9/0").await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}
