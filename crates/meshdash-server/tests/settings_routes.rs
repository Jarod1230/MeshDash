//! Reading and changing settings the way the interface does.
//!
//! Through the real router, because that is where the mounting, the guard and
//! the module types meet — and mounting is exactly the sort of thing that is
//! right in the file and wrong in the router.

// The `allow-unwrap-in-tests` setting only covers functions marked as tests,
// not the helpers they call. In a test file a panic on a broken assumption is
// the point, so the lint has nothing to catch here.
#![allow(clippy::unwrap_used)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use meshdash_core::{
    config::AuthConfig,
    db::Database,
    event::EventBus,
    link::{self, LinkConfig},
    module::{AppContext, ModuleRegistry},
    settings::Settings,
};
use meshdash_transport::mock::MockTransport;
use tower::ServiceExt;

/// A router with settings backed by a database, as the binary builds it.
async fn router() -> (axum::Router, AppContext) {
    let db = Database::open_in_memory().await.unwrap();
    let events = EventBus::new();
    let (link, _task) = link::spawn(
        MockTransport::new(vec![]),
        LinkConfig::default(),
        events.clone(),
    );

    let settings = Settings::load(Default::default(), db.clone(), events.clone())
        .await
        .unwrap();

    let context = AppContext {
        db,
        events,
        link,
        settings,
    };

    (
        meshdash_server::build_router(
            &ModuleRegistry::new(),
            context.clone(),
            AuthConfig::default(),
        ),
        context,
    )
}

async fn ask(router: &axum::Router, request: Request<Body>) -> (StatusCode, serde_json::Value) {
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();

    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    )
}

fn get(path: &str) -> Request<Body> {
    Request::builder().uri(path).body(Body::empty()).unwrap()
}

fn put(path: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("PUT")
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[tokio::test]
async fn answers_with_every_option_including_the_ones_nobody_wrote_down() {
    let (router, _context) = router().await;

    let (status, body) = ask(&router, get("/api/v1/settings")).await;

    assert_eq!(status, StatusCode::OK);
    let telemetry = body["modules"]
        .as_array()
        .unwrap()
        .iter()
        .find(|one| one["module"] == "telemetry")
        .unwrap();

    // Nothing is configured here, so every value is the module's own default —
    // and the page still has to be able to show them.
    assert_eq!(telemetry["values"]["neighbours"], false);
    assert_eq!(telemetry["values"]["every_minutes"], 30);
    assert_eq!(telemetry["changed"], false);
}

#[tokio::test]
async fn a_change_takes_effect_for_the_module_that_owns_it() {
    let (router, context) = router().await;

    let (status, body) = ask(
        &router,
        put(
            "/api/v1/settings/telemetry",
            serde_json::json!({ "neighbours": true }),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["values"]["neighbours"], true);
    assert_eq!(body["changed"], true);

    // And the module sees it, which is the whole point.
    let now: meshdash_modules::telemetry::Settings = context.settings.get("telemetry").unwrap();
    assert!(now.neighbours);
}

#[tokio::test]
async fn a_misspelled_option_is_refused_rather_than_kept() {
    let (router, _context) = router().await;

    let (status, body) = ask(
        &router,
        put(
            "/api/v1/settings/traffic",
            serde_json::json!({ "keep_dayz": 7 }),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_parameter");
}

#[tokio::test]
async fn settings_that_decide_how_the_process_starts_are_not_on_offer() {
    // Where MeshDash listens, which device the node hangs on, where the
    // database lives — a page the process serves cannot change those and
    // must not pretend to.
    let (router, _context) = router().await;

    let (status, _) = ask(
        &router,
        put(
            "/api/v1/settings/server",
            serde_json::json!({ "bind": "0.0.0.0:80" }),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}
