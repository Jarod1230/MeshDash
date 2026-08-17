//! Assembles the HTTP surface from the module registry.
//!
//! The server knows how to mount and how to answer, not what any route means.
//! A module contributes routes for its own paths; where they end up is decided
//! here, so the `/api/v1/<module>/` convention lives in one place instead of in
//! every module.
//!
//! # Not here yet
//!
//! Authentication is missing on purpose: `docs/roadmap.md` requires an ADR
//! before it is built, and that decision is open. The WebSocket stream and the
//! embedded frontend follow in the same step.

use axum::{
    Json, Router,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use meshdash_core::module::{AppContext, ModuleRegistry};
use serde::Serialize;

/// Prefix every module route sits under, per `docs/conventions.md`.
pub const API_PREFIX: &str = "/api/v1";

/// The body shape for every error the API returns.
///
/// Fixed by `docs/conventions.md`: a caller can rely on `error.code` without
/// parsing prose, and `error.message` stays free for humans.
#[derive(Debug, Serialize)]
pub struct ApiError {
    error: ApiErrorBody,
}

/// Inner part of [`ApiError`].
#[derive(Debug, Serialize)]
struct ApiErrorBody {
    code: String,
    message: String,
}

impl ApiError {
    /// Builds an error body from a stable code and a readable message.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error: ApiErrorBody {
                code: code.into(),
                message: message.into(),
            },
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        // Only used for "not found" so far; a status argument arrives when
        // there is a second case.
        (StatusCode::NOT_FOUND, Json(self)).into_response()
    }
}

/// Builds the router: every module's routes under its own prefix.
pub fn build_router(registry: &ModuleRegistry, context: AppContext) -> Router {
    let mut api = Router::new();

    for module in registry.modules() {
        let Some(routes) = module.routes() else {
            continue;
        };

        let mount = format!("/{}", module.name());
        tracing::debug!(module = module.name(), path = %mount, "mounted module routes");
        api = api.nest(&mount, routes);
    }

    Router::new()
        .nest(API_PREFIX, api)
        // Anything unrouted answers in the agreed error shape, so a client
        // never has to tell an API error from a stray HTML page.
        .fallback(|| async { ApiError::new("not_found", "no route matches this path") })
        .with_state(context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request, routing::get};
    use meshdash_core::{
        db::Database,
        event::EventBus,
        link::{self, LinkConfig},
        module::Module,
    };
    use meshdash_transport::mock::MockTransport;
    use tower::ServiceExt;

    async fn context() -> AppContext {
        let db = Database::open_in_memory().await.unwrap();
        let events = EventBus::new();
        let (link, _task) = link::spawn(
            MockTransport::new(vec![]),
            LinkConfig::default(),
            events.clone(),
        );
        AppContext { db, events, link }
    }

    /// A module offering one route.
    struct Talkative {
        name: &'static str,
        body: &'static str,
    }

    #[async_trait::async_trait]
    impl Module for Talkative {
        fn name(&self) -> &'static str {
            self.name
        }

        fn routes(&self) -> Option<Router<AppContext>> {
            let body = self.body;
            Some(Router::new().route("/things", get(move || async move { body })))
        }
    }

    /// A module without an API.
    struct Silent;

    #[async_trait::async_trait]
    impl Module for Silent {
        fn name(&self) -> &'static str {
            "silent"
        }
    }

    /// Sends one request through the router and returns status plus body.
    async fn call(router: Router, path: &str) -> (StatusCode, String) {
        let response = router
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();

        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, String::from_utf8(bytes.to_vec()).unwrap())
    }

    #[tokio::test]
    async fn mounts_a_module_under_its_own_prefix() {
        let mut registry = ModuleRegistry::new();
        registry
            .register(Box::new(Talkative {
                name: "nodes",
                body: "contacts",
            }))
            .unwrap();

        let router = build_router(&registry, context().await);

        let (status, body) = call(router, "/api/v1/nodes/things").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "contacts");
    }

    #[tokio::test]
    async fn keeps_modules_on_separate_paths() {
        let mut registry = ModuleRegistry::new();
        registry
            .register(Box::new(Talkative {
                name: "nodes",
                body: "from nodes",
            }))
            .unwrap();
        registry
            .register(Box::new(Talkative {
                name: "telemetry",
                body: "from telemetry",
            }))
            .unwrap();

        let router = build_router(&registry, context().await);

        assert_eq!(
            call(router.clone(), "/api/v1/nodes/things").await.1,
            "from nodes"
        );
        assert_eq!(
            call(router, "/api/v1/telemetry/things").await.1,
            "from telemetry"
        );
    }

    #[tokio::test]
    async fn a_module_without_routes_adds_none() {
        let mut registry = ModuleRegistry::new();
        registry.register(Box::new(Silent)).unwrap();

        let router = build_router(&registry, context().await);

        let (status, _) = call(router, "/api/v1/silent/things").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn answers_an_unknown_path_in_the_agreed_shape() {
        let registry = ModuleRegistry::new();

        let router = build_router(&registry, context().await);

        let (status, body) = call(router, "/api/v1/nowhere").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        // The shape is fixed by docs/conventions.md, so a client can rely on it.
        assert!(body.contains("\"error\""), "got: {body}");
        assert!(body.contains("\"code\""), "got: {body}");
        assert!(body.contains("\"message\""), "got: {body}");
    }

    #[tokio::test]
    async fn serves_nothing_when_no_module_is_registered() {
        // MeshDash must come up with every module switched off.
        let registry = ModuleRegistry::new();

        let router = build_router(&registry, context().await);

        let (status, _) = call(router, "/api/v1/nodes/things").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}
