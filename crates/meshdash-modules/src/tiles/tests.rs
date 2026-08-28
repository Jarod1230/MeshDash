//! Tests for the tiles module.
//!
//! The fetching path runs against a real HTTP server on a loopback port
//! rather than a stubbed client: what is worth checking here is the round trip
//! — request, content type, what lands on disk, what comes back the second
//! time — and a stub would only confirm the stub.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use meshdash_core::{
    config::ModuleSettings, db::Database, event::EventBus, link, module::AppContext,
    settings::Settings as RuntimeSettings,
};
use meshdash_transport::mock::{MockTransport, Step};

use super::*;

/// A directory that removes itself, so a test leaves nothing behind.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("meshdash-tiles-{name}"));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A context whose `[modules.tiles]` section is what the test wants.
async fn context_with(settings: serde_json::Value) -> AppContext {
    let db = Database::open_in_memory().await.unwrap();
    let events = EventBus::new();
    let (handle, _task) = link::spawn(
        MockTransport::new(vec![Step::Drop("no node needed here".into())]),
        link::LinkConfig::default(),
        events.clone(),
    );

    let mut module_settings = ModuleSettings::default();
    module_settings.set("tiles", settings);

    AppContext {
        db,
        events,
        link: handle,
        settings: RuntimeSettings::from_file(module_settings),
    }
}

/// A tile server that answers a fixed body, and counts how often it was asked.
async fn source(body: &'static [u8], kind: &'static str) -> (SocketAddr, Arc<AtomicUsize>) {
    let asked = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&asked);

    // A fallback rather than a route: axum takes one parameter per path
    // segment, and what this server answers does not depend on the path
    // anyway.
    let app = Router::new().fallback(move || {
        let counter = Arc::clone(&counter);
        async move {
            counter.fetch_add(1, Ordering::SeqCst);
            ([(header::CONTENT_TYPE, kind)], body)
        }
    });

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    (address, asked)
}

/// Brings a module up against a source and hands back its service.
async fn started(settings: serde_json::Value) -> (TilesModule, AppContext) {
    let context = context_with(settings).await;
    let module = TilesModule::default();
    module.start(&context).await.unwrap();
    (module, context)
}

#[test]
fn ships_without_a_source() {
    // The shipped state fetches nothing: MeshDash runs where there is no
    // uplink, and choosing a public server for the operator would both fail
    // there and hand a stranger the location of their mesh.
    let settings = Settings::default();

    assert_eq!(settings.source, "");
    assert_eq!(settings.attribution, "");
}

#[test]
fn refuses_coordinates_that_cannot_be_a_place() {
    // At zoom 2 the world is four tiles across, so 4 is one past the edge.
    assert!(is_a_tile(2, 3, 3, 19));
    assert!(!is_a_tile(2, 4, 0, 19));
    assert!(!is_a_tile(2, 0, 4, 19));
    // Deeper than the source has, and deeper than any shift could describe.
    assert!(!is_a_tile(20, 0, 0, 19));
    assert!(!is_a_tile(31, 0, 0, 40));
}

#[tokio::test]
async fn a_source_without_a_credit_does_not_start() {
    let context = context_with(serde_json::json!({
        "source": "https://example.invalid/{z}/{x}/{y}.png"
    }))
    .await;

    let refused = TilesModule::default().start(&context).await.unwrap_err();

    assert!(refused.contains("attribution"), "{refused}");
}

#[tokio::test]
async fn without_a_source_it_says_so_instead_of_pretending() {
    let (module, _context) = started(serde_json::json!({})).await;
    let service = Arc::clone(&module.service);

    let Json(info) = describe(Extension(Arc::clone(&service))).await;
    assert!(!info.available);

    let refused = tile(Extension(service), UrlPath((10, 550, 335))).await;
    assert!(matches!(refused, Err(TileError::NoSource)));
}

#[tokio::test]
async fn fetches_a_tile_once_and_keeps_it() {
    let scratch = Scratch::new("keeps-it");
    let (address, asked) = source(b"a fake png", "image/png").await;
    let (module, _context) = started(serde_json::json!({
        "source": format!("http://{address}/{{z}}/{{x}}/{{y}}.png"),
        "attribution": "Ein Kachelserver",
        "cache_dir": scratch.0,
    }))
    .await;

    let first = tile(
        Extension(Arc::clone(&module.service)),
        UrlPath((10, 550, 335)),
    )
    .await;
    assert!(first.is_ok());

    // On disk where a tile tree keeps tiles, so warming a region later is a
    // matter of filling this tree rather than a second architecture.
    let kept = scratch.0.join("10").join("550").join("335.png");
    assert_eq!(tokio::fs::read(&kept).await.unwrap(), b"a fake png");

    let second = tile(
        Extension(Arc::clone(&module.service)),
        UrlPath((10, 550, 335)),
    )
    .await;
    assert!(second.is_ok());
    assert_eq!(
        asked.load(Ordering::SeqCst),
        1,
        "the second read went out again"
    );
}

#[tokio::test]
async fn refuses_to_hand_a_browser_something_that_is_not_a_map() {
    let scratch = Scratch::new("not-a-map");
    // What a source answers with when it is unhappy: a page, not a tile.
    let (address, _asked) = source(b"<html>rate limited</html>", "text/html").await;
    let (module, _context) = started(serde_json::json!({
        "source": format!("http://{address}/{{z}}/{{x}}/{{y}}.png"),
        "attribution": "Ein Kachelserver",
        "cache_dir": scratch.0,
    }))
    .await;

    let refused = tile(
        Extension(Arc::clone(&module.service)),
        UrlPath((10, 550, 335)),
    )
    .await;

    assert!(matches!(refused, Err(TileError::NotAnImage { .. })));
    // And nothing of it was kept, so the next reader does not get it either.
    assert!(!scratch.0.join("10").exists());
}

#[tokio::test]
async fn a_tile_outside_the_grid_never_reaches_the_source() {
    let scratch = Scratch::new("outside");
    let (address, asked) = source(b"a fake png", "image/png").await;
    let (module, _context) = started(serde_json::json!({
        "source": format!("http://{address}/{{z}}/{{x}}/{{y}}.png"),
        "attribution": "Ein Kachelserver",
        "cache_dir": scratch.0,
    }))
    .await;

    let refused = tile(Extension(Arc::clone(&module.service)), UrlPath((2, 9, 0))).await;

    assert!(matches!(refused, Err(TileError::OutsideTheGrid { .. })));
    assert_eq!(
        asked.load(Ordering::SeqCst),
        0,
        "somebody else paid for that request"
    );
}

#[tokio::test]
async fn tells_the_map_whose_it_is() {
    let scratch = Scratch::new("credit");
    let (module, _context) = started(serde_json::json!({
        "source": "https://example.invalid/{z}/{x}/{y}.png",
        "attribution": "© OpenStreetMap-Mitwirkende",
        "cache_dir": scratch.0,
        "max_zoom": 17,
    }))
    .await;

    let Json(info) = describe(Extension(Arc::clone(&module.service))).await;

    assert!(info.available);
    assert_eq!(info.attribution, "© OpenStreetMap-Mitwirkende");
    assert_eq!(info.max_zoom, 17);
}
