//! Serves the dashboard from inside the binary.
//!
//! MeshDash ships as one artefact, so the built frontend is baked in rather
//! than read from disk at runtime — see `docs/architecture.md`.
//!
//! # Only with the `embed-frontend` feature
//!
//! Embedding needs `web/dist`, which exists only after `pnpm build`. Making it
//! unconditional would mean a plain `cargo build` — and the Rust CI job — could
//! not compile without the frontend having been built first. With the feature
//! off, the API works and every other path says plainly that no frontend is
//! embedded, instead of a blank page nobody can explain.
//!
//! # Unknown paths go to the dashboard
//!
//! The frontend routes in the browser, so `/nodes` is a page, not a file. Any
//! path that is not a known asset therefore returns `index.html` and lets the
//! frontend decide. This never affects the API: those routes are matched first.

use axum::{
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
};

/// Answers a request for anything that is not the API.
pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    serve_path(path)
}

#[cfg(feature = "embed-frontend")]
mod embedded {
    use rust_embed::Embed;

    /// The built frontend, as produced by `pnpm build`.
    #[derive(Embed)]
    #[folder = "../../web/dist"]
    pub struct Assets;
}

/// Looks up one path in the embedded assets.
#[cfg(feature = "embed-frontend")]
fn serve_path(path: &str) -> Response {
    let candidate = if path.is_empty() { "index.html" } else { path };

    if let Some(file) = embedded::Assets::get(candidate) {
        // rust-embed derives the type at build time via its `mime-guess`
        // feature, so a stylesheet does not arrive as a byte stream.
        let mime = file.metadata.mimetype().to_owned();
        return ([(header::CONTENT_TYPE, mime)], file.data.into_owned()).into_response();
    }

    // Not an asset: hand it to the frontend's own router.
    match embedded::Assets::get("index.html") {
        Some(index) => (
            [(header::CONTENT_TYPE, "text/html")],
            index.data.into_owned(),
        )
            .into_response(),
        None => not_embedded(),
    }
}

/// Without the feature there is nothing to serve.
#[cfg(not(feature = "embed-frontend"))]
fn serve_path(_path: &str) -> Response {
    not_embedded()
}

/// Says plainly that this binary carries no frontend.
///
/// Better than an empty page: whoever sees it knows the build, not the
/// installation, is the reason.
fn not_embedded() -> Response {
    (
        StatusCode::NOT_FOUND,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        "No frontend is embedded in this build.\n\
         The API is available under /api/v1/.\n\
         Build with `just build` to include the dashboard.\n",
    )
        .into_response()
}
