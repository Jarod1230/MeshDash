//! Reading and changing settings through the interface.
//!
//! Mounted by the server rather than by a module: settings belong to every
//! module and to none, and a module may not read another's section.
//!
//! # What the interface may change, and what it may not
//!
//! Only what a running service can honour. Where MeshDash listens, which
//! device the node hangs on, where the database lives — those decide how the
//! process starts, and changing them through a page that the process serves
//! would be a promise it cannot keep. They stay in the file.
//!
//! Module settings are the other kind: they are read while running, so they
//! can be changed while running.

use axum::{
    Json,
    extract::{Path, State},
    response::IntoResponse,
    routing::get,
};
use meshdash_core::{module::AppContext, settings::SetError};
use serde::Serialize;

/// What every module's settings currently are.
#[derive(Debug, Serialize)]
pub struct AllSettings {
    /// One entry per module that has settings, with its effective values.
    pub modules: Vec<ModuleView>,
}

/// One module's settings as they apply right now.
#[derive(Debug, Serialize)]
pub struct ModuleView {
    /// Which module.
    pub module: String,
    /// The values in force: the file, with any changes laid over it.
    pub values: serde_json::Value,
    /// Whether any of them were changed through the interface.
    ///
    /// Worth saying: it is the difference between "this is what the file says"
    /// and "somebody changed this here, and the file still says otherwise".
    pub changed: bool,
}

/// The modules whose settings the interface offers.
///
/// A short list rather than a discovered one: what a section means is the
/// module's business, and the interface has to name the options anyway in
/// order to explain them. A new option is added here and in the page beside
/// it — see `docs/configuration.md`.
const OFFERED: [&str; 2] = ["telemetry", "traffic"];

/// The routes, to be mounted under the API prefix.
pub fn routes() -> axum::Router<AppContext> {
    axum::Router::new()
        .route("/", get(read))
        .route("/{module}", axum::routing::put(change))
}

async fn read(State(context): State<AppContext>) -> Result<Json<AllSettings>, SettingsError> {
    let changed = context.settings.changed_modules();

    let mut modules = Vec::with_capacity(OFFERED.len());
    for module in OFFERED {
        modules.push(ModuleView {
            module: module.to_owned(),
            values: values_of(&context, module)?,
            changed: changed.iter().any(|one| one == module),
        });
    }

    Ok(Json(AllSettings { modules }))
}

/// One module's effective settings, through that module's own type.
///
/// Going through the type rather than handing back the raw section is what
/// fills in the defaults: an option nobody ever wrote down still has a value,
/// and the page should show it.
fn values_of(context: &AppContext, module: &str) -> Result<serde_json::Value, SettingsError> {
    let read = |value: Result<serde_json::Value, _>| {
        value.map_err(|error: meshdash_core::config::ModuleSettingsError| {
            SettingsError::Unreadable(error.to_string())
        })
    };

    match module {
        "telemetry" => read(
            context
                .settings
                .get::<meshdash_modules::telemetry::Settings>(module)
                .map(|settings| serde_json::to_value(settings).unwrap_or(serde_json::Value::Null)),
        ),
        "traffic" => read(
            context
                .settings
                .get::<meshdash_modules::traffic::Settings>(module)
                .map(|settings| serde_json::to_value(settings).unwrap_or(serde_json::Value::Null)),
        ),
        _ => Err(SettingsError::Unknown(module.to_owned())),
    }
}

async fn change(
    State(context): State<AppContext>,
    Path(module): Path<String>,
    Json(patch): Json<serde_json::Value>,
) -> Result<Json<ModuleView>, SettingsError> {
    match module.as_str() {
        "telemetry" => context
            .settings
            .set::<meshdash_modules::telemetry::Settings>(&module, patch)
            .await
            .map_err(SettingsError::Refused)?,
        "traffic" => context
            .settings
            .set::<meshdash_modules::traffic::Settings>(&module, patch)
            .await
            .map_err(SettingsError::Refused)?,
        _ => return Err(SettingsError::Unknown(module)),
    }

    Ok(Json(ModuleView {
        values: values_of(&context, &module)?,
        module,
        changed: true,
    }))
}

/// Why settings could not be read or changed.
#[derive(Debug)]
pub enum SettingsError {
    /// No module by that name offers settings here.
    Unknown(String),
    /// The stored settings do not fit the module's own type.
    Unreadable(String),
    /// The module would not take the change.
    Refused(SetError),
}

impl IntoResponse for SettingsError {
    fn into_response(self) -> axum::response::Response {
        let (status, code, message) = match self {
            Self::Unknown(module) => (
                axum::http::StatusCode::NOT_FOUND,
                "unknown_module",
                format!("{module} has no settings that can be changed here"),
            ),
            Self::Unreadable(reason) => {
                tracing::error!(reason, "stored settings do not fit the module");
                (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "storage_failed",
                    "the stored settings could not be read".to_owned(),
                )
            }
            Self::Refused(error) => (
                axum::http::StatusCode::BAD_REQUEST,
                "invalid_parameter",
                error.to_string(),
            ),
        };

        (
            status,
            Json(serde_json::json!({ "error": { "code": code, "message": message } })),
        )
            .into_response()
    }
}
