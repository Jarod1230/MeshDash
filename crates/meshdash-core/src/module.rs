//! The contract a domain module fulfils, and the registry that runs them.
//!
//! Everything MeshDash does for an operator lives in modules; the core only
//! provides what all of them need. This file is where that split is decided —
//! if the contract leaks domain concepts, the modularity is claimed rather than
//! real.
//!
//! # What a module gets, and what it must not do
//!
//! It receives an [`AppContext`]: the database, the event bus and a handle to
//! the node. With those it owns its tables, listens to events and sends
//! commands. What it must **not** do is call another module, read another
//! module's tables, or require another module to be present — the rules in
//! `docs/module-system.md`. None of that is enforced by the compiler; the
//! contract is deliberately narrow so that breaking those rules takes effort.
//!
//! # Routes belong to the module, mounting belongs to the server
//!
//! A module hands over a router for its own paths and never learns where it is
//! mounted. `meshdash-server` puts it under `/api/v1/<name>/`, which keeps the
//! prefix in one place instead of in every module.

use axum::Router;

use crate::{
    config::ModuleSettings, db::Database, db::DatabaseError, db::Migration, event::EventBus,
    link::LinkHandle,
};

/// What the core hands a module so it can do its work.
///
/// Cloning is cheap and shares the same database, bus and link — every module
/// works against the same instances.
#[derive(Debug, Clone)]
pub struct AppContext {
    /// Storage, including the module's own migrations.
    pub db: Database,
    /// Where events are published and listened for.
    pub events: EventBus,
    /// Sends commands to the node.
    pub link: LinkHandle,
    /// The `[modules.<name>]` sections, for a module to read its own.
    ///
    /// Untyped on purpose: the core carries these and does not interpret
    /// them. A module reads its section with [`ModuleSettings::get`].
    pub settings: ModuleSettings,
}

/// Why a module could not be brought up.
#[derive(Debug, thiserror::Error)]
pub enum ModuleError {
    /// The module's migrations failed.
    #[error("migrations for module {module} failed")]
    Migration {
        /// Which module.
        module: String,
        /// The underlying cause.
        #[source]
        source: DatabaseError,
    },

    /// The module refused to start.
    #[error("module {module} failed to start: {reason}")]
    Start {
        /// Which module.
        module: String,
        /// What it reported.
        reason: String,
    },

    /// Two modules claim the same name.
    ///
    /// Fatal rather than tolerable: the name decides table prefixes, the route
    /// path and which migrations belong to whom. Two claimants would migrate
    /// each other's schema.
    #[error("module name {name} is registered twice")]
    DuplicateName {
        /// The contested name.
        name: String,
    },
}

/// One self-contained piece of domain functionality.
#[async_trait::async_trait]
pub trait Module: Send + Sync {
    /// Identifies the module and prefixes everything belonging to it.
    ///
    /// Its tables are named `<name>_<thing>`, its routes live under
    /// `/api/v1/<name>/`, and its migrations are recorded under it. Must be
    /// stable: renaming a module orphans its tables and its schema version.
    fn name(&self) -> &'static str;

    /// The module's schema history, in ascending version order.
    ///
    /// Counted per module and starting at 1 — see [`crate::db`]. An empty list
    /// is fine for a module that stores nothing.
    fn migrations(&self) -> &'static [Migration] {
        &[]
    }

    /// The module's HTTP routes, relative to its own prefix.
    ///
    /// A path is written as `/contacts`, not `/api/v1/nodes/contacts` — the
    /// server mounts it. `None` is right for a module that offers no API.
    fn routes(&self) -> Option<Router<AppContext>> {
        None
    }

    /// Starts whatever the module needs running: event handlers, background
    /// jobs, timers.
    ///
    /// Called once, after migrations have been applied. Long-running work
    /// belongs in a spawned task — returning here means "up and running", not
    /// "finished".
    async fn start(&self, context: &AppContext) -> Result<(), String> {
        let _ = context;
        Ok(())
    }
}

/// Holds the modules and brings them up.
///
/// Removing a module means deleting its line here — the test from
/// `docs/module-system.md` for whether the cut is right.
#[derive(Default)]
pub struct ModuleRegistry {
    modules: Vec<Box<dyn Module>>,
}

impl std::fmt::Debug for ModuleRegistry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ModuleRegistry")
            .field("modules", &self.names())
            .finish()
    }
}

impl ModuleRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a module.
    ///
    /// Rejects a name that is already taken, because the name governs table
    /// prefixes and migration bookkeeping.
    pub fn register(&mut self, module: Box<dyn Module>) -> Result<(), ModuleError> {
        let name = module.name();

        if self.names().contains(&name) {
            return Err(ModuleError::DuplicateName {
                name: name.to_owned(),
            });
        }

        self.modules.push(module);
        Ok(())
    }

    /// The registered names, in registration order.
    pub fn names(&self) -> Vec<&'static str> {
        self.modules.iter().map(|module| module.name()).collect()
    }

    /// The registered modules, in registration order.
    ///
    /// For whoever assembles the HTTP surface — the server needs each module's
    /// name and routes together to mount them.
    pub fn modules(&self) -> impl Iterator<Item = &dyn Module> {
        self.modules.iter().map(AsRef::as_ref)
    }

    /// Whether any module is registered.
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    /// Applies every module's migrations, then starts every module.
    ///
    /// A failing module aborts the start: running with half a schema would
    /// produce wrong data rather than an error, and that is worse than not
    /// starting at all.
    pub async fn start_all(&self, context: &AppContext) -> Result<(), ModuleError> {
        // Migrate everything first: a module's start may already query, and it
        // should not find a schema that is only partly there.
        for module in &self.modules {
            let migrations = module.migrations();
            if migrations.is_empty() {
                continue;
            }

            context
                .db
                .migrate(module.name(), migrations)
                .await
                .map_err(|source| ModuleError::Migration {
                    module: module.name().to_owned(),
                    source,
                })?;
        }

        for module in &self.modules {
            module
                .start(context)
                .await
                .map_err(|reason| ModuleError::Start {
                    module: module.name().to_owned(),
                    reason,
                })?;

            tracing::info!(module = module.name(), "module started");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use meshdash_transport::mock::MockTransport;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    use crate::link::{self, LinkConfig};

    /// Builds a context backed by an in-memory database and a mock node.
    async fn context() -> AppContext {
        let db = Database::open_in_memory().await.unwrap();
        let events = EventBus::new();
        let (link, _task) = link::spawn(
            MockTransport::new(vec![]),
            LinkConfig::default(),
            events.clone(),
        );

        AppContext {
            db,
            events,
            link,
            settings: ModuleSettings::default(),
        }
    }

    /// A module that records whether it was started.
    struct Recorder {
        name: &'static str,
        migrations: &'static [Migration],
        started: Arc<AtomicBool>,
    }

    impl Recorder {
        fn new(name: &'static str, migrations: &'static [Migration]) -> (Self, Arc<AtomicBool>) {
            let started = Arc::new(AtomicBool::new(false));
            let module = Self {
                name,
                migrations,
                started: Arc::clone(&started),
            };
            (module, started)
        }
    }

    #[async_trait::async_trait]
    impl Module for Recorder {
        fn name(&self) -> &'static str {
            self.name
        }

        fn migrations(&self) -> &'static [Migration] {
            self.migrations
        }

        async fn start(&self, _context: &AppContext) -> Result<(), String> {
            self.started.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    /// A module that refuses to come up.
    struct Refuses;

    #[async_trait::async_trait]
    impl Module for Refuses {
        fn name(&self) -> &'static str {
            "refuses"
        }

        async fn start(&self, _context: &AppContext) -> Result<(), String> {
            Err("cannot reach its sensor".to_owned())
        }
    }

    const NODES_MIGRATIONS: &[Migration] = &[Migration {
        version: 1,
        description: "contacts table",
        sql: "CREATE TABLE nodes_contacts (id INTEGER PRIMARY KEY)",
    }];

    const TELEMETRY_MIGRATIONS: &[Migration] = &[Migration {
        version: 1,
        description: "samples table",
        sql: "CREATE TABLE telemetry_samples (id INTEGER PRIMARY KEY)",
    }];

    #[tokio::test]
    async fn starts_out_empty() {
        let registry = ModuleRegistry::new();

        assert!(registry.is_empty());
        assert!(registry.names().is_empty());
    }

    #[tokio::test]
    async fn keeps_modules_in_registration_order() {
        let mut registry = ModuleRegistry::new();
        let (nodes, _) = Recorder::new("nodes", &[]);
        let (telemetry, _) = Recorder::new("telemetry", &[]);

        registry.register(Box::new(nodes)).unwrap();
        registry.register(Box::new(telemetry)).unwrap();

        assert_eq!(registry.names(), vec!["nodes", "telemetry"]);
    }

    #[tokio::test]
    async fn refuses_a_name_that_is_taken() {
        let mut registry = ModuleRegistry::new();
        let (first, _) = Recorder::new("nodes", &[]);
        let (second, _) = Recorder::new("nodes", &[]);
        registry.register(Box::new(first)).unwrap();

        let result = registry.register(Box::new(second));

        assert!(matches!(result, Err(ModuleError::DuplicateName { .. })));
    }

    #[tokio::test]
    async fn migrates_and_starts_every_module() {
        let context = context().await;
        let mut registry = ModuleRegistry::new();
        let (nodes, nodes_started) = Recorder::new("nodes", NODES_MIGRATIONS);
        let (telemetry, telemetry_started) = Recorder::new("telemetry", TELEMETRY_MIGRATIONS);
        registry.register(Box::new(nodes)).unwrap();
        registry.register(Box::new(telemetry)).unwrap();

        registry.start_all(&context).await.unwrap();

        assert!(nodes_started.load(Ordering::SeqCst));
        assert!(telemetry_started.load(Ordering::SeqCst));
        assert_eq!(context.db.schema_version("nodes").await.unwrap(), Some(1));
        assert_eq!(
            context.db.schema_version("telemetry").await.unwrap(),
            Some(1)
        );
    }

    #[tokio::test]
    async fn a_module_without_migrations_is_fine() {
        // Not every module stores something; requiring a schema would be
        // ceremony without purpose.
        let context = context().await;
        let mut registry = ModuleRegistry::new();
        let (module, started) = Recorder::new("system", &[]);
        registry.register(Box::new(module)).unwrap();

        registry.start_all(&context).await.unwrap();

        assert!(started.load(Ordering::SeqCst));
        assert_eq!(context.db.schema_version("system").await.unwrap(), None);
    }

    #[tokio::test]
    async fn names_the_module_that_refused_to_start() {
        let context = context().await;
        let mut registry = ModuleRegistry::new();
        registry.register(Box::new(Refuses)).unwrap();

        let error = registry.start_all(&context).await.unwrap_err();

        // An operator has to be able to tell which module is at fault.
        let message = error.to_string();
        assert!(message.contains("refuses"), "got: {message}");
        assert!(
            message.contains("cannot reach its sensor"),
            "got: {message}"
        );
    }

    #[tokio::test]
    async fn stops_the_start_when_a_module_fails() {
        let context = context().await;
        let mut registry = ModuleRegistry::new();
        registry.register(Box::new(Refuses)).unwrap();
        let (later, later_started) = Recorder::new("later", &[]);
        registry.register(Box::new(later)).unwrap();

        assert!(registry.start_all(&context).await.is_err());
        assert!(
            !later_started.load(Ordering::SeqCst),
            "running on with a broken module would produce wrong data"
        );
    }

    #[tokio::test]
    async fn reports_which_module_had_a_bad_migration() {
        const BROKEN: &[Migration] = &[Migration {
            version: 1,
            description: "broken",
            sql: "THIS IS NOT SQL",
        }];
        let context = context().await;
        let mut registry = ModuleRegistry::new();
        let (module, _) = Recorder::new("nodes", BROKEN);
        registry.register(Box::new(module)).unwrap();

        let error = registry.start_all(&context).await.unwrap_err();

        assert!(matches!(error, ModuleError::Migration { ref module, .. } if module == "nodes"));
    }

    #[tokio::test]
    async fn starting_an_empty_registry_does_nothing() {
        // MeshDash must run with every module switched off.
        let context = context().await;
        let registry = ModuleRegistry::new();

        registry.start_all(&context).await.unwrap();
    }
}
