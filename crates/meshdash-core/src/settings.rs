//! Settings a module reads, and an operator can change while it runs.
//!
//! # Two layers, and which wins
//!
//! What `meshdash.toml` and the environment say is the **ground**: it is read
//! once at start and never changes. On top of it lie the changes an operator
//! made through the interface, kept in the database. A stored value wins over
//! the file, option by option — not section by section, so changing one option
//! does not silently reset the ones beside it.
//!
//! The file stays authoritative for anything nobody has touched. That is what
//! makes a deployment reproducible: copy the file, get the same behaviour,
//! minus whatever somebody deliberately changed on the running system.
//!
//! # Why the database and not the file
//!
//! Writing back to `meshdash.toml` would mean MeshDash rewriting a file the
//! operator maintains — reformatting it, dropping their comments, and racing
//! with their editor. The file is theirs. What the interface changes is
//! MeshDash's own state, and MeshDash's own state lives in its database.
//!
//! # Changes are announced, not polled
//!
//! Setting something publishes [`AppEvent::SettingsChanged`]. A module that
//! captured a value at start can react; a module that reads the value when it
//! uses it needs nothing. The second is the better shape and the one to
//! prefer — see `docs/module-system.md`.

use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};

use serde::de::DeserializeOwned;

use crate::{
    config::{ModuleSettings, ModuleSettingsError},
    db::{Database, DatabaseError, Migration},
    event::{AppEvent, EventBus},
};

/// The core's own schema. Counted separately from every module's.
const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    description: "settings changed while running",
    sql: "
    -- One row per module whose settings were changed through the interface.
    -- The value holds only what was changed; everything else still comes from
    -- the configuration file.
    CREATE TABLE core_settings (
        module  TEXT PRIMARY KEY,
        changed TEXT NOT NULL
    );
",
}];

/// The name the core migrates its own tables under.
const CORE: &str = "core";

/// What a module reads its settings from.
///
/// Cheap to clone: every clone shares the same store, so a change made through
/// one is seen by all of them.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    /// What the file and the environment said. Fixed for the run.
    file: ModuleSettings,
    /// What was changed since, per module. Empty until something is.
    changed: Arc<RwLock<BTreeMap<String, serde_json::Value>>>,
    /// Where changes are kept, or `None` for settings that are not stored —
    /// tests and anything assembling settings by hand.
    store: Option<Database>,
    /// Where a change is announced, if anybody is listening.
    events: Option<EventBus>,
}

impl Settings {
    /// Settings that come from a file and are never written back.
    ///
    /// For tests and for anything that assembles settings by hand.
    pub fn from_file(file: ModuleSettings) -> Self {
        Self {
            file,
            ..Self::default()
        }
    }

    /// Settings backed by the database, with whatever was changed before.
    pub async fn load(
        file: ModuleSettings,
        db: Database,
        events: EventBus,
    ) -> Result<Self, DatabaseError> {
        db.migrate(CORE, MIGRATIONS).await?;

        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT module, changed FROM core_settings")
                .fetch_all(db.pool())
                .await?;

        let mut changed = BTreeMap::new();
        for (module, stored) in rows {
            match serde_json::from_str(&stored) {
                Ok(value) => {
                    changed.insert(module, value);
                }
                // A row we cannot read is left out rather than taken down with
                // the whole service: the file's value still applies.
                Err(error) => tracing::error!(%error, module, "stored settings are not JSON"),
            }
        }

        Ok(Self {
            file,
            changed: Arc::new(RwLock::new(changed)),
            store: Some(db),
            events: Some(events),
        })
    }

    /// Reads one module's settings, file and changes together.
    ///
    /// A missing section yields the type's default, so a module works without
    /// being configured. A section that does not fit is an error rather than a
    /// silent fallback: a misspelled option that quietly does nothing is the
    /// same trap `deny_unknown_fields` exists to prevent elsewhere.
    pub fn get<T>(&self, module: &str) -> Result<T, ModuleSettingsError>
    where
        T: Default + DeserializeOwned,
    {
        let from_file: serde_json::Value = self.file.get(module)?;
        let changes = self.changes_for(module);

        // Neither a section nor a change: the module's own defaults, without
        // going through serde at all — an empty object would not deserialize
        // into a type whose fields have no serde defaults of their own.
        if from_file.is_null() && changes.is_none() {
            return Ok(T::default());
        }

        let merged = match changes {
            Some(changes) => merge(from_file, changes),
            None => from_file,
        };

        serde_json::from_value(merged).map_err(|error| ModuleSettingsError {
            module: module.to_owned(),
            reason: error.to_string(),
        })
    }

    /// Changes some of one module's settings and announces it.
    ///
    /// Only the options named are touched. Passing an option that module does
    /// not have is refused: silently keeping it would leave the operator
    /// believing they changed something.
    pub async fn set<T>(&self, module: &str, patch: serde_json::Value) -> Result<(), SetError>
    where
        T: Default + DeserializeOwned,
    {
        if !patch.is_object() {
            return Err(SetError::NotSettings);
        }

        let merged = merge(self.file.get(module).unwrap_or(serde_json::Value::Null), {
            let mut all = self.changes_for(module).unwrap_or(serde_json::json!({}));
            all = merge(all, patch.clone());
            all
        });

        // Checked against the module's own type before anything is stored, so
        // a typo is answered rather than kept.
        serde_json::from_value::<T>(merged).map_err(|error| SetError::Rejected {
            reason: error.to_string(),
        })?;

        let stored = {
            let mut changes = self.changed.write().map_err(|_| SetError::Poisoned)?;
            let all = changes
                .entry(module.to_owned())
                .or_insert_with(|| serde_json::json!({}));
            *all = merge(all.clone(), patch);
            all.clone()
        };

        if let Some(db) = &self.store {
            sqlx::query(
                "INSERT INTO core_settings (module, changed) VALUES (?1, ?2)
                 ON CONFLICT(module) DO UPDATE SET changed = excluded.changed",
            )
            .bind(module)
            .bind(stored.to_string())
            .execute(db.pool())
            .await
            .map_err(|error| SetError::Storage(error.to_string()))?;
        }

        if let Some(events) = &self.events {
            events.publish(AppEvent::SettingsChanged {
                module: module.to_owned(),
            });
        }

        Ok(())
    }

    /// What was changed for one module, if anything.
    fn changes_for(&self, module: &str) -> Option<serde_json::Value> {
        self.changed.read().ok()?.get(module).cloned()
    }

    /// Which modules have changes stored, for whoever reports on them.
    pub fn changed_modules(&self) -> Vec<String> {
        self.changed
            .read()
            .map(|changes| changes.keys().cloned().collect())
            .unwrap_or_default()
    }
}

/// Lays one object over another, option by option.
///
/// Deliberately shallow-per-key rather than replacing whole sections: changing
/// one option must not reset the ones beside it.
fn merge(base: serde_json::Value, top: serde_json::Value) -> serde_json::Value {
    // A module with no section in the file has no base at all. That is not a
    // reason to drop what was changed — it is the normal case for anything
    // configured only through the interface.
    let mut merged = match base {
        serde_json::Value::Object(fields) => fields,
        _ => serde_json::Map::new(),
    };

    if let serde_json::Value::Object(top) = top {
        for (key, value) in top {
            merged.insert(key, value);
        }
    }

    serde_json::Value::Object(merged)
}

/// Why a setting could not be changed.
#[derive(Debug, thiserror::Error)]
pub enum SetError {
    /// The body was not a set of options.
    #[error("settings must be an object of options")]
    NotSettings,
    /// The module would not accept the result.
    #[error("the module refused these settings: {reason}")]
    Rejected {
        /// What serde complained about.
        reason: String,
    },
    /// The change could not be kept.
    #[error("the change could not be stored: {0}")]
    Storage(String),
    /// Another thread died holding the lock.
    #[error("the settings store is unusable")]
    Poisoned,
}

#[cfg(test)]
mod tests;
