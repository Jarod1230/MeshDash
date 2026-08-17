//! Assembles the HTTP surface from the module registry.
//!
//! The server knows how to mount and how to answer, not what any route means.
//! A module contributes routes for its own paths; where they end up is decided
//! here, so the `/api/v1/<module>/` convention lives in one place instead of in
//! every module.
//!
//! # Authentication
//!
//! A single bearer token, per ADR-0006. When one is configured, every request
//! under [`API_PREFIX`] needs it; when none is, the API is open — which is only
//! safe because the service then refuses to listen on a public address.
//!
//! # Beyond the API
//!
//! Everything outside [`API_PREFIX`] is the dashboard, served from inside the
//! binary — see [`frontend`]. The live event stream sits at [`EVENTS_PATH`] and
//! authenticates differently, because a browser cannot put a header on a
//! WebSocket; [`events`] explains why.

use axum::{
    Json, Router,
    extract::Request,
    http::{StatusCode, header::AUTHORIZATION},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use meshdash_core::{
    config::AuthConfig,
    module::{AppContext, ModuleRegistry},
};
use serde::Serialize;
use subtle::ConstantTimeEq;

pub mod events;
pub mod frontend;

/// Prefix every module route sits under, per `docs/conventions.md`.
pub const API_PREFIX: &str = "/api/v1";

/// Where the live event stream lives, per `docs/architecture.md`.
///
/// Reserved: a module may not be called `events`, or its routes would collide
/// with this one.
pub const EVENTS_PATH: &str = "/api/v1/events";

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

/// An error together with the status it should be answered with.
struct WithStatus(StatusCode, ApiError);

impl IntoResponse for WithStatus {
    fn into_response(self) -> Response {
        (self.0, Json(self.1)).into_response()
    }
}

/// Rejects a request that carries no valid token.
///
/// Constant-time comparison on purpose: comparing byte by byte and returning
/// early would let an attacker read the token off the response time, one
/// character at a time. See ADR-0006.
async fn require_token(auth: AuthConfig, request: Request, next: Next) -> Response {
    let Some(expected) = auth.configured_token() else {
        // No token configured means an open API — only reachable on loopback,
        // because `Config::check_exposure` refuses anything else at startup.
        return next.run(request).await;
    };

    let presented = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));

    let accepted =
        presented.is_some_and(|token| token.as_bytes().ct_eq(expected.as_bytes()).into());

    if !accepted {
        // Never log the presented token — a near miss would end up on disk.
        tracing::warn!(
            path = %request.uri().path(),
            presented = presented.is_some(),
            "rejected an unauthenticated request"
        );

        return WithStatus(
            StatusCode::UNAUTHORIZED,
            ApiError::new("unauthorized", "a valid bearer token is required"),
        )
        .into_response();
    }

    next.run(request).await
}

/// Builds the router: every module's routes under its own prefix, behind the
/// configured authentication.
pub fn build_router(registry: &ModuleRegistry, context: AppContext, auth: AuthConfig) -> Router {
    let auth_for_events = auth.clone();
    let mut api = Router::new();

    for module in registry.modules() {
        let Some(routes) = module.routes() else {
            continue;
        };

        let mount = format!("/{}", module.name());
        tracing::debug!(module = module.name(), path = %mount, "mounted module routes");
        api = api.nest(&mount, routes);
    }

    // The API answers its own misses, so that an unmatched path inside the API
    // still passes through the guard below. Otherwise an unauthenticated
    // caller could tell a real path (401) from an invented one (404) and map
    // the API without ever holding a token.
    let api = api
        .fallback(|| async { not_found() })
        // Guards the whole API at once — per module, a new one could forget it.
        .layer(middleware::from_fn(move |request, next| {
            require_token(auth.clone(), request, next)
        }));

    Router::new()
        // Registered above the guarded tree on purpose: a browser cannot put a
        // header on a WebSocket, so this endpoint authenticates itself after
        // the upgrade. See `events` for why.
        .route(
            EVENTS_PATH,
            axum::routing::get({
                let auth = auth_for_events;
                move |upgrade, state| events::handle_upgrade(upgrade, state, auth.clone())
            }),
        )
        .nest(API_PREFIX, api)
        // Everything outside the API is the dashboard. No token needed: the
        // frontend is public files, and it asks for one before it shows data.
        .fallback(frontend::serve)
        .with_state(context)
}

/// The standard answer for a path that matches nothing.
fn not_found() -> WithStatus {
    WithStatus(
        StatusCode::NOT_FOUND,
        ApiError::new("not_found", "no route matches this path"),
    )
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
        send(router, path, None).await
    }

    /// Sends a request, optionally with an `Authorization` header.
    async fn send(router: Router, path: &str, authorization: Option<&str>) -> (StatusCode, String) {
        let mut request = Request::builder().uri(path);
        if let Some(value) = authorization {
            request = request.header("authorization", value);
        }

        let response = router
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();

        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, String::from_utf8(bytes.to_vec()).unwrap())
    }

    /// A configuration demanding the given token.
    fn demanding(token: &str) -> AuthConfig {
        AuthConfig {
            token: Some(token.to_owned()),
            allow_unauthenticated: false,
        }
    }

    /// A router with one module, guarded by `auth`.
    async fn guarded(auth: AuthConfig) -> Router {
        let mut registry = ModuleRegistry::new();
        registry
            .register(Box::new(Talkative {
                name: "nodes",
                body: "contacts",
            }))
            .unwrap();

        build_router(&registry, context().await, auth)
    }

    #[tokio::test]
    async fn lets_a_correct_token_through() {
        let router = guarded(demanding("s3cret")).await;

        let (status, body) = send(router, "/api/v1/nodes/things", Some("Bearer s3cret")).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "contacts");
    }

    #[tokio::test]
    async fn rejects_a_request_without_a_token() {
        let router = guarded(demanding("s3cret")).await;

        let (status, body) = call(router, "/api/v1/nodes/things").await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(body.contains("unauthorized"), "got: {body}");
    }

    #[tokio::test]
    async fn rejects_a_wrong_token() {
        let router = guarded(demanding("s3cret")).await;

        let (status, _) = send(router, "/api/v1/nodes/things", Some("Bearer wrong")).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn rejects_a_token_that_is_merely_a_prefix() {
        // A comparison that stops at the first difference would accept this
        // as "matching so far"; length has to count too.
        let router = guarded(demanding("s3cret")).await;

        let (status, _) = send(router, "/api/v1/nodes/things", Some("Bearer s3c")).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn rejects_a_token_sent_without_the_bearer_scheme() {
        let router = guarded(demanding("s3cret")).await;

        let (status, _) = send(router, "/api/v1/nodes/things", Some("s3cret")).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn serves_without_a_token_when_none_is_configured() {
        // Only reachable on loopback — Config::check_exposure sees to that.
        let router = guarded(AuthConfig::default()).await;

        let (status, body) = call(router, "/api/v1/nodes/things").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "contacts");
    }

    #[tokio::test]
    async fn treats_a_blank_token_as_no_protection() {
        // Consistent with Config::check_exposure, which refuses to expose such
        // a configuration in the first place.
        let router = guarded(AuthConfig {
            token: Some("   ".to_owned()),
            allow_unauthenticated: false,
        })
        .await;

        let (status, _) = call(router, "/api/v1/nodes/things").await;

        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn serves_the_dashboard_outside_the_api() {
        let router = guarded(demanding("s3cret")).await;

        // No token: the frontend is public files, and it asks for one itself
        // before it shows any data.
        let (status, body) = call(router, "/").await;

        if cfg!(feature = "embed-frontend") {
            assert_eq!(status, StatusCode::OK);
            assert!(
                body.contains("<html"),
                "expected the dashboard, got: {body}"
            );
        } else {
            // A plain explanation beats a blank page nobody can account for.
            assert_eq!(status, StatusCode::NOT_FOUND);
            assert!(body.contains("No frontend is embedded"), "got: {body}");
        }
    }

    #[tokio::test]
    async fn a_dashboard_path_never_swallows_the_api() {
        // The frontend catches unknown paths, but the API must keep answering
        // in JSON — otherwise a client would get HTML where it expects data.
        let router = guarded(demanding("s3cret")).await;

        let (status, body) = call(router, "/api/v1/nodes/things").await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(body.contains("\"error\""), "got: {body}");
    }

    #[tokio::test]
    async fn does_not_reveal_which_api_paths_exist() {
        // Answering 401 for a real path and 404 for an invented one would let
        // anyone map the API without a token.
        let router = guarded(demanding("s3cret")).await;

        let real = call(router.clone(), "/api/v1/nodes/things").await.0;
        let invented = call(router, "/api/v1/nodes/nothing-here").await.0;

        assert_eq!(real, StatusCode::UNAUTHORIZED);
        assert_eq!(
            invented,
            StatusCode::UNAUTHORIZED,
            "an unknown API path must not be distinguishable from a real one"
        );
    }

    #[tokio::test]
    async fn guards_paths_of_modules_that_do_not_exist() {
        let router = guarded(demanding("s3cret")).await;

        let (status, _) = call(router, "/api/v1/nosuchmodule/things").await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn guards_every_module_at_once() {
        // A second module must not need its own guard — forgetting one would
        // silently open a hole.
        let mut registry = ModuleRegistry::new();
        registry
            .register(Box::new(Talkative {
                name: "nodes",
                body: "a",
            }))
            .unwrap();
        registry
            .register(Box::new(Talkative {
                name: "telemetry",
                body: "b",
            }))
            .unwrap();
        let router = build_router(&registry, context().await, demanding("s3cret"));

        assert_eq!(
            call(router.clone(), "/api/v1/telemetry/things").await.0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            send(router, "/api/v1/telemetry/things", Some("Bearer s3cret"))
                .await
                .0,
            StatusCode::OK
        );
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

        let router = build_router(&registry, context().await, AuthConfig::default());

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

        let router = build_router(&registry, context().await, AuthConfig::default());

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

        let router = build_router(&registry, context().await, AuthConfig::default());

        let (status, _) = call(router, "/api/v1/silent/things").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn answers_an_unknown_path_in_the_agreed_shape() {
        let registry = ModuleRegistry::new();

        let router = build_router(&registry, context().await, AuthConfig::default());

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

        let router = build_router(&registry, context().await, AuthConfig::default());

        let (status, _) = call(router, "/api/v1/nodes/things").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}
