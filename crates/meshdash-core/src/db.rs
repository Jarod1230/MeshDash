//! SQLite storage and the migration mechanism modules share.
//!
//! # Every module migrates on its own
//!
//! A module owns its tables and brings its own migrations. Versions are counted
//! **per module**, not globally, so two modules never have to agree on a
//! number — adding a module means adding migrations, not renumbering someone
//! else's. Removing one leaves the others untouched.
//!
//! That is what makes the modularity real rather than claimed: if migrations
//! shared one sequence, every module would depend on every other one's history.
//!
//! # Queries are checked at runtime, not at compile time
//!
//! `sqlx::query!` would require a prepared database at build time, so every
//! build and every CI run would need one. Plain [`sqlx::query`] is used instead,
//! with tests that run against a real schema. See `docs/architecture.md`.

use sqlx::{
    Executor, Row, SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

use crate::config::DatabaseConfig;

/// Table recording which migrations already ran.
///
/// Prefixed with an underscore to keep it apart from module tables, which are
/// named `<module>_<thing>`.
const MIGRATION_TABLE: &str = "_migrations";

/// One step in a module's schema history.
///
/// Once merged, a migration is never edited — a correction is a new migration.
/// Editing one would leave databases that already ran it in a state nobody can
/// reproduce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Migration {
    /// Position in this module's sequence, counted from 1.
    pub version: i64,
    /// What it does, for the log and for anyone reading the table.
    pub description: &'static str,
    /// The statements to run.
    pub sql: &'static str,
}

/// Why a database operation failed.
#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    /// The database itself reported a problem.
    #[error("database operation failed")]
    Sqlx(#[from] sqlx::Error),

    /// The directory holding the database file could not be created.
    #[error("could not create the directory for the database file")]
    Directory(#[from] std::io::Error),

    /// A module offered migrations that do not form a usable sequence.
    #[error("module {module} has an invalid migration sequence: {problem}")]
    InvalidMigrations {
        /// Which module is at fault.
        module: String,
        /// What is wrong with it.
        problem: String,
    },
}

/// A connection pool to the SQLite file, plus the migration bookkeeping.
#[derive(Debug, Clone)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// Opens the configured database, creating file and directory if needed.
    pub async fn open(config: &DatabaseConfig) -> Result<Self, DatabaseError> {
        // A fresh installation has no data directory yet; failing on that would
        // demand manual setup before the first start for no good reason.
        if let Some(parent) = config.path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }

        let options = SqliteConnectOptions::new()
            .filename(&config.path)
            .create_if_missing(true)
            // Write-ahead logging lets readers work while something writes,
            // which matters once modules query while the link records events.
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .foreign_keys(true);

        Self::connect(options).await
    }

    /// Opens a database that exists only for the lifetime of this handle.
    ///
    /// Used by tests, which get a real schema without touching the disk.
    pub async fn open_in_memory() -> Result<Self, DatabaseError> {
        let options = SqliteConnectOptions::new()
            .in_memory(true)
            .foreign_keys(true);

        Self::connect(options).await
    }

    /// Builds the pool and makes sure the bookkeeping table exists.
    async fn connect(options: SqliteConnectOptions) -> Result<Self, DatabaseError> {
        let pool = SqlitePoolOptions::new()
            // An in-memory database lives in its connection: a second one would
            // see an empty database, so the pool is kept to a single connection.
            .max_connections(1)
            .connect_with(options)
            .await?;

        let database = Self { pool };
        database.ensure_migration_table().await?;
        Ok(database)
    }

    /// Creates the table tracking applied migrations, unless it is there.
    async fn ensure_migration_table(&self) -> Result<(), DatabaseError> {
        let statement = format!(
            "CREATE TABLE IF NOT EXISTS {MIGRATION_TABLE} (
                module      TEXT    NOT NULL,
                version     INTEGER NOT NULL,
                description TEXT    NOT NULL,
                applied_at  TEXT    NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (module, version)
            )"
        );
        self.pool.execute(statement.as_str()).await?;
        Ok(())
    }

    /// The pool, for modules that need to query directly.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Brings one module's schema up to date.
    ///
    /// Already applied versions are skipped, so calling this on every start is
    /// the intended use. Each migration runs in its own transaction: a failing
    /// one leaves no half-applied schema behind.
    pub async fn migrate(
        &self,
        module: &str,
        migrations: &[Migration],
    ) -> Result<u32, DatabaseError> {
        check_sequence(module, migrations)?;

        let current = self.schema_version(module).await?.unwrap_or(0);
        let mut applied = 0;

        for migration in migrations.iter().filter(|m| m.version > current) {
            // One transaction per migration: a failure rolls back its own
            // statements, and the ones before it stay applied and recorded.
            let mut transaction = self.pool.begin().await?;

            transaction.execute(migration.sql).await?;

            sqlx::query(&format!(
                "INSERT INTO {MIGRATION_TABLE} (module, version, description) VALUES (?, ?, ?)"
            ))
            .bind(module)
            .bind(migration.version)
            .bind(migration.description)
            .execute(&mut *transaction)
            .await?;

            transaction.commit().await?;

            tracing::info!(
                module,
                version = migration.version,
                description = migration.description,
                "applied migration"
            );
            applied += 1;
        }

        Ok(applied)
    }

    /// Which version a module's schema is at, or `None` if it never migrated.
    pub async fn schema_version(&self, module: &str) -> Result<Option<i64>, DatabaseError> {
        let row = sqlx::query(&format!(
            "SELECT MAX(version) AS version FROM {MIGRATION_TABLE} WHERE module = ?"
        ))
        .bind(module)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.try_get::<Option<i64>, _>("version")?)
    }
}

/// Rejects a migration list that cannot be applied meaningfully.
///
/// Both faults would otherwise show up as data loss rather than an error: a
/// repeated version makes "already applied" ambiguous, and an unsorted list
/// would silently skip everything below the highest version once recorded.
fn check_sequence(module: &str, migrations: &[Migration]) -> Result<(), DatabaseError> {
    let mut previous: Option<i64> = None;

    for migration in migrations {
        if let Some(previous) = previous {
            if migration.version == previous {
                return Err(DatabaseError::InvalidMigrations {
                    module: module.to_owned(),
                    problem: format!("version {} appears more than once", migration.version),
                });
            }
            if migration.version < previous {
                return Err(DatabaseError::InvalidMigrations {
                    module: module.to_owned(),
                    problem: format!(
                        "version {} comes after {previous}, but versions must ascend",
                        migration.version
                    ),
                });
            }
        }
        previous = Some(migration.version);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn migration(version: i64, sql: &'static str) -> Migration {
        Migration {
            version,
            description: "test migration",
            sql,
        }
    }

    /// Asks the database whether a table exists.
    async fn table_exists(db: &Database, name: &str) -> bool {
        let found: Option<(String,)> =
            sqlx::query_as("SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?")
                .bind(name)
                .fetch_optional(db.pool())
                .await
                .unwrap();
        found.is_some()
    }

    #[tokio::test]
    async fn opens_a_database_in_memory() {
        let db = Database::open_in_memory().await.unwrap();

        assert!(sqlx::query("SELECT 1").execute(db.pool()).await.is_ok());
    }

    #[tokio::test]
    async fn applies_a_migration() {
        let db = Database::open_in_memory().await.unwrap();

        let applied = db
            .migrate(
                "nodes",
                &[migration(
                    1,
                    "CREATE TABLE nodes_contacts (id INTEGER PRIMARY KEY)",
                )],
            )
            .await
            .unwrap();

        assert_eq!(applied, 1);
        assert!(table_exists(&db, "nodes_contacts").await);
        assert_eq!(db.schema_version("nodes").await.unwrap(), Some(1));
    }

    #[tokio::test]
    async fn applies_migrations_in_order() {
        let db = Database::open_in_memory().await.unwrap();

        db.migrate(
            "nodes",
            &[
                migration(1, "CREATE TABLE nodes_contacts (id INTEGER PRIMARY KEY)"),
                migration(2, "ALTER TABLE nodes_contacts ADD COLUMN name TEXT"),
            ],
        )
        .await
        .unwrap();

        // The second only works if the first ran before it.
        assert_eq!(db.schema_version("nodes").await.unwrap(), Some(2));
    }

    #[tokio::test]
    async fn skips_migrations_that_already_ran() {
        let db = Database::open_in_memory().await.unwrap();
        let migrations = [migration(
            1,
            "CREATE TABLE nodes_contacts (id INTEGER PRIMARY KEY)",
        )];

        db.migrate("nodes", &migrations).await.unwrap();
        // Running again must not fail on the existing table.
        let applied = db.migrate("nodes", &migrations).await.unwrap();

        assert_eq!(applied, 0, "nothing left to do the second time");
    }

    #[tokio::test]
    async fn applies_only_what_is_new() {
        let db = Database::open_in_memory().await.unwrap();
        let first = [migration(
            1,
            "CREATE TABLE nodes_contacts (id INTEGER PRIMARY KEY)",
        )];
        db.migrate("nodes", &first).await.unwrap();

        let both = [
            first[0],
            migration(2, "ALTER TABLE nodes_contacts ADD COLUMN name TEXT"),
        ];
        let applied = db.migrate("nodes", &both).await.unwrap();

        assert_eq!(applied, 1, "only the new migration runs");
        assert_eq!(db.schema_version("nodes").await.unwrap(), Some(2));
    }

    #[tokio::test]
    async fn counts_versions_per_module() {
        // Two modules both starting at 1 must not collide — otherwise adding a
        // module would mean renumbering another one's history.
        let db = Database::open_in_memory().await.unwrap();

        db.migrate(
            "nodes",
            &[migration(1, "CREATE TABLE nodes_contacts (id INTEGER)")],
        )
        .await
        .unwrap();
        db.migrate(
            "telemetry",
            &[migration(1, "CREATE TABLE telemetry_samples (id INTEGER)")],
        )
        .await
        .unwrap();

        assert!(table_exists(&db, "nodes_contacts").await);
        assert!(table_exists(&db, "telemetry_samples").await);
        assert_eq!(db.schema_version("nodes").await.unwrap(), Some(1));
        assert_eq!(db.schema_version("telemetry").await.unwrap(), Some(1));
    }

    #[tokio::test]
    async fn a_module_without_migrations_has_no_version() {
        let db = Database::open_in_memory().await.unwrap();

        assert_eq!(db.schema_version("nodes").await.unwrap(), None);
    }

    #[tokio::test]
    async fn leaves_nothing_behind_when_a_migration_fails() {
        let db = Database::open_in_memory().await.unwrap();

        let result = db
            .migrate(
                "nodes",
                &[migration(
                    1,
                    "CREATE TABLE nodes_contacts (id INTEGER); THIS IS NOT SQL",
                )],
            )
            .await;

        assert!(result.is_err());
        assert!(
            !table_exists(&db, "nodes_contacts").await,
            "a failed migration must not leave half a schema"
        );
        assert_eq!(db.schema_version("nodes").await.unwrap(), None);
    }

    #[tokio::test]
    async fn rejects_versions_that_are_out_of_order() {
        let db = Database::open_in_memory().await.unwrap();

        let result = db
            .migrate(
                "nodes",
                &[
                    migration(2, "CREATE TABLE a (id INTEGER)"),
                    migration(1, "CREATE TABLE b (id INTEGER)"),
                ],
            )
            .await;

        assert!(matches!(
            result,
            Err(DatabaseError::InvalidMigrations { .. })
        ));
    }

    #[tokio::test]
    async fn rejects_a_repeated_version() {
        // Two migrations claiming version 1 would make "already applied"
        // ambiguous, and one of them would silently never run.
        let db = Database::open_in_memory().await.unwrap();

        let result = db
            .migrate(
                "nodes",
                &[
                    migration(1, "CREATE TABLE a (id INTEGER)"),
                    migration(1, "CREATE TABLE b (id INTEGER)"),
                ],
            )
            .await;

        assert!(matches!(
            result,
            Err(DatabaseError::InvalidMigrations { .. })
        ));
    }

    #[tokio::test]
    async fn creates_the_directory_for_the_database_file() {
        let dir = std::env::temp_dir().join(format!("meshdash-db-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let config = DatabaseConfig {
            path: dir.join("nested").join("meshdash.db"),
        };

        let db = Database::open(&config).await.unwrap();
        db.migrate(
            "nodes",
            &[migration(1, "CREATE TABLE nodes_contacts (id INTEGER)")],
        )
        .await
        .unwrap();

        assert!(config.path.exists(), "the database file must be created");

        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
