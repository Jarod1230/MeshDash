//! Map tiles, fetched by MeshDash and kept on disk.
//!
//! The ground surface draws nodes and connections itself, but whether two
//! stations have a hill or a flat meadow between them is half the answer with
//! LoRa — and that comes from a map underneath. This module is where that map
//! comes from.
//!
//! # Why the detour through the service
//!
//! The browser could ask a tile server directly, and it would be less code.
//! It would also send the mesh's bounding box to that server on every glance,
//! from every viewer. Going through MeshDash means the tile server sees
//! MeshDash once per tile, the copy on disk is shared by everyone looking, and
//! the terms of the source — attribution, a `User-Agent`, a limit on how many
//! requests are in flight — live in one place. See ADR-0011.
//!
//! # Nothing is fetched unless somebody names a source
//!
//! There is no default tile server. MeshDash runs in places without an uplink,
//! and picking a public server on the operator's behalf would both break there
//! and quietly hand a stranger the location of their mesh. Without
//! `[modules.tiles] source` this module answers "no source" and fetches
//! nothing.
//!
//! # The cache is a tree of files
//!
//! `<cache_dir>/<z>/<x>/<y>.<ext>`, which is what raster tiles are everywhere.
//! Warming a region for a deployment without an uplink is then a matter of
//! filling that tree, not a second architecture.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

use async_trait::async_trait;
use axum::{
    Extension, Json, Router,
    extract::Path as UrlPath,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use meshdash_core::module::{AppContext, Module};
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;

/// How this module may be configured, under `[modules.tiles]`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// Where tiles come from, as a template with `{z}`, `{x}` and `{y}`.
    ///
    /// Empty means no tiles. That is the shipped state, on purpose — see the
    /// note at the top of this file.
    pub source: String,
    /// Who the map is by, shown on the map.
    ///
    /// Required as soon as a source is named: every tile service worth using
    /// asks for it in its terms, and a service that shows a map without
    /// saying whose it is puts its operator in the wrong.
    pub attribution: String,
    /// Where fetched tiles are kept.
    pub cache_dir: PathBuf,
    /// The deepest zoom that is passed on.
    ///
    /// Not a matter of taste: asking a source for a level it does not have
    /// earns a 404 per tile, and the requests still cost the source something.
    pub max_zoom: u8,
    /// What MeshDash calls itself when fetching.
    ///
    /// Tile services ask for a real identifier and block generic ones. Names
    /// the project, not the operator.
    pub user_agent: String,
    /// How many fetches may be in flight at once.
    ///
    /// A map being dragged around asks for tiles faster than any source wants
    /// to answer. The rest wait rather than going out all at once.
    pub max_concurrent_fetches: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            source: String::new(),
            attribution: String::new(),
            cache_dir: PathBuf::from("data/tiles"),
            // OpenStreetMap's raster tiles end here, and it is finer than any
            // position a mesh reports.
            max_zoom: 19,
            user_agent: concat!(
                "MeshDash/",
                env!("CARGO_PKG_VERSION"),
                " (+https://github.com/Jarod1230/MeshDash)"
            )
            .to_owned(),
            max_concurrent_fetches: 4,
        }
    }
}

/// Serves map tiles from a cache on disk, filling it from a source.
#[derive(Debug, Default)]
pub struct TilesModule {
    /// Filled in `start`, read by the handlers. Behind a `OnceLock` because
    /// the settings are not known when the module is registered.
    service: Arc<OnceLock<TileService>>,
}

/// Everything a tile request needs, assembled once.
#[derive(Debug)]
struct TileService {
    settings: Settings,
    client: reqwest::Client,
    upstream: Semaphore,
}

/// What `/api/v1/tiles` answers, so the map knows whether to draw a base.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct TileInfo {
    /// Whether a source is configured at all.
    pub available: bool,
    /// Whose map it is. Empty when there is none.
    pub attribution: String,
    /// The deepest zoom that will be answered.
    pub max_zoom: u8,
}

#[async_trait]
impl Module for TilesModule {
    fn name(&self) -> &'static str {
        "tiles"
    }

    fn routes(&self) -> Option<Router<AppContext>> {
        Some(
            Router::new()
                .route("/", get(describe))
                .route("/{z}/{x}/{y}", get(tile))
                // The service travels as an extension rather than as router
                // state: the state is the AppContext every module shares.
                .layer(Extension(Arc::clone(&self.service))),
        )
    }

    async fn start(&self, context: &AppContext) -> Result<(), String> {
        let settings: Settings = context
            .settings
            .get("tiles")
            .map_err(|error| error.to_string())?;

        if settings.source.is_empty() {
            tracing::info!("no tile source configured; the map draws without a base");
        } else {
            // Refused rather than warned about: the operator is the one the
            // source's terms bind, and a map that quietly drops the credit
            // leaves them in breach without anyone noticing.
            if settings.attribution.trim().is_empty() {
                return Err(
                    "[modules.tiles] names a source but no attribution; every tile service \
                     requires the credit to be shown"
                        .to_owned(),
                );
            }

            tokio::fs::create_dir_all(&settings.cache_dir)
                .await
                .map_err(|error| {
                    format!(
                        "could not create the tile cache at {}: {error}",
                        settings.cache_dir.display()
                    )
                })?;

            tracing::info!(
                cache = %settings.cache_dir.display(),
                "tiles are served from a source and kept on disk"
            );
        }

        let client = reqwest::Client::builder()
            .user_agent(settings.user_agent.clone())
            .build()
            .map_err(|error| format!("could not build the tile client: {error}"))?;

        let permits = settings.max_concurrent_fetches.max(1);
        let _ = self.service.set(TileService {
            settings,
            client,
            upstream: Semaphore::new(permits),
        });

        Ok(())
    }
}

async fn describe(Extension(service): Extension<Arc<OnceLock<TileService>>>) -> Json<TileInfo> {
    let Some(service) = service.get() else {
        return Json(TileInfo {
            available: false,
            attribution: String::new(),
            max_zoom: 0,
        });
    };

    Json(TileInfo {
        available: !service.settings.source.is_empty(),
        attribution: service.settings.attribution.clone(),
        max_zoom: service.settings.max_zoom,
    })
}

async fn tile(
    Extension(service): Extension<Arc<OnceLock<TileService>>>,
    UrlPath((z, x, y)): UrlPath<(u8, u32, u32)>,
) -> Result<Response, TileError> {
    let service = service.get().ok_or(TileError::NoSource)?;

    if service.settings.source.is_empty() {
        return Err(TileError::NoSource);
    }
    if !is_a_tile(z, x, y, service.settings.max_zoom) {
        return Err(TileError::OutsideTheGrid { z, x, y });
    }

    if let Some((bytes, kind)) = from_cache(&service.settings.cache_dir, z, x, y).await {
        return Ok(answer(bytes, kind));
    }

    let (bytes, kind) = fetch(service, z, x, y).await?;
    // A cache that could not be written is not a reason to withhold the tile
    // the reader is waiting for.
    if let Err(error) = store(&service.settings.cache_dir, z, x, y, &kind, &bytes).await {
        tracing::warn!(%error, z, x, y, "could not keep a tile");
    }

    Ok(answer(bytes, kind))
}

/// The image types a tile may be, and the extension each is stored under.
const KINDS: [(&str, &str); 3] = [
    ("image/png", "png"),
    ("image/jpeg", "jpg"),
    ("image/webp", "webp"),
];

/// Is this a tile that can exist?
///
/// At zoom `z` the world is a 2^z square. Anything outside cannot be a place,
/// and passing it on would earn a 404 from the source on somebody else's
/// budget.
fn is_a_tile(z: u8, x: u32, y: u32, max_zoom: u8) -> bool {
    if z > max_zoom || z > 30 {
        return false;
    }

    let edge = 1u32 << z;
    x < edge && y < edge
}

/// Where a tile of this kind lives on disk.
fn tile_path(cache: &Path, z: u8, x: u32, y: u32, extension: &str) -> PathBuf {
    cache
        .join(z.to_string())
        .join(x.to_string())
        .join(format!("{y}.{extension}"))
}

/// Reads a tile that was fetched before, whichever type it was stored as.
async fn from_cache(cache: &Path, z: u8, x: u32, y: u32) -> Option<(Vec<u8>, String)> {
    for (kind, extension) in KINDS {
        if let Ok(bytes) = tokio::fs::read(tile_path(cache, z, x, y, extension)).await {
            return Some((bytes, kind.to_owned()));
        }
    }

    None
}

/// Keeps a tile for the next reader.
async fn store(
    cache: &Path,
    z: u8,
    x: u32,
    y: u32,
    kind: &str,
    bytes: &[u8],
) -> std::io::Result<()> {
    let extension = KINDS
        .iter()
        .find(|(known, _)| *known == kind)
        .map_or("png", |(_, extension)| *extension);
    let path = tile_path(cache, z, x, y, extension);

    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    tokio::fs::write(path, bytes).await
}

/// Fetches one tile from the configured source.
async fn fetch(
    service: &TileService,
    z: u8,
    x: u32,
    y: u32,
) -> Result<(Vec<u8>, String), TileError> {
    let url = service
        .settings
        .source
        .replace("{z}", &z.to_string())
        .replace("{x}", &x.to_string())
        .replace("{y}", &y.to_string());

    // Waiting here is the point: a map being dragged asks for tiles far faster
    // than any source wants to answer.
    let _permit = service
        .upstream
        .acquire()
        .await
        .map_err(|_| TileError::Upstream("the tile service is shutting down".to_owned()))?;

    let response = service
        .client
        .get(&url)
        .send()
        .await
        .map_err(|error| TileError::Upstream(error.to_string()))?;

    if !response.status().is_success() {
        return Err(TileError::UpstreamStatus(response.status().as_u16()));
    }

    let kind = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(';').next().unwrap_or(value).trim().to_owned())
        .unwrap_or_default();

    // Only images are passed on. Whatever else a source might answer with —
    // an error page, a redirect notice — has no business being handed to a
    // browser as a map tile.
    if !KINDS.iter().any(|(known, _)| *known == kind) {
        return Err(TileError::NotAnImage { kind });
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|error| TileError::Upstream(error.to_string()))?;

    Ok((bytes.to_vec(), kind))
}

/// A tile, with the caching a tile deserves.
///
/// Tiles for a given zoom and place do not change from one week to the next,
/// and the browser holding on to them is what keeps a dragged map from asking
/// MeshDash the same question twice.
fn answer(bytes: Vec<u8>, kind: String) -> Response {
    (
        [
            (header::CONTENT_TYPE, kind),
            (
                header::CACHE_CONTROL,
                "public, max-age=604800, immutable".to_owned(),
            ),
        ],
        bytes,
    )
        .into_response()
}

/// Why there is no tile.
#[derive(Debug)]
pub enum TileError {
    /// No source is configured, so nothing can be fetched.
    NoSource,
    /// The coordinates do not describe a tile that can exist.
    OutsideTheGrid { z: u8, x: u32, y: u32 },
    /// The source could not be reached.
    Upstream(String),
    /// The source answered, but not with success.
    UpstreamStatus(u16),
    /// The source answered with something that is not an image.
    NotAnImage { kind: String },
}

impl IntoResponse for TileError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::NoSource => (
                StatusCode::NOT_FOUND,
                "no_tile_source",
                "no tile source is configured".to_owned(),
            ),
            Self::OutsideTheGrid { z, x, y } => (
                StatusCode::BAD_REQUEST,
                "invalid_parameter",
                format!("{z}/{x}/{y} is not a tile that can exist"),
            ),
            Self::Upstream(error) => {
                tracing::warn!(%error, "the tile source could not be reached");
                (
                    StatusCode::BAD_GATEWAY,
                    "tile_source_failed",
                    "the tile source could not be reached".to_owned(),
                )
            }
            Self::UpstreamStatus(status) => {
                tracing::warn!(status, "the tile source refused");
                (
                    StatusCode::BAD_GATEWAY,
                    "tile_source_failed",
                    format!("the tile source answered with {status}"),
                )
            }
            Self::NotAnImage { kind } => {
                tracing::warn!(kind, "the tile source answered with something else");
                (
                    StatusCode::BAD_GATEWAY,
                    "tile_source_failed",
                    "the tile source did not answer with an image".to_owned(),
                )
            }
        };

        (
            status,
            Json(serde_json::json!({ "error": { "code": code, "message": message } })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests;
