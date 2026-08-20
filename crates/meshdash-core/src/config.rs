//! Settings, assembled from defaults, a file and the environment.
//!
//! Later sources win over earlier ones:
//!
//! 1. the defaults in this module,
//! 2. `meshdash.toml` in the working directory,
//! 3. environment variables prefixed `MESHDASH_`.
//!
//! Nesting is spelled with a double underscore, so `[server] bind` is
//! `MESHDASH_SERVER__BIND`. Command line arguments are meant to win over all of
//! these; they belong to the binary and are not handled here.
//!
//! # MeshDash starts without a configuration file
//!
//! Every setting has a default, and a missing file is not an error. Defaults
//! are chosen conservatively: the web interface binds to localhost only, and
//! there is no authentication token, so nothing is exposed by accident.

use std::{
    collections::BTreeMap,
    net::SocketAddr,
    path::{Path, PathBuf},
};

use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use serde::{Deserialize, Serialize};

/// Environment variables carrying settings start with this.
const ENV_PREFIX: &str = "MESHDASH_";

/// Where the configuration file is looked for unless told otherwise.
const DEFAULT_FILE: &str = "meshdash.toml";

/// Everything MeshDash needs to know before it starts.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Where the web interface listens.
    pub server: ServerConfig,
    /// How callers authenticate, if at all.
    pub auth: AuthConfig,
    /// Where the database lives.
    pub database: DatabaseConfig,
    /// How to reach the companion node.
    pub node: NodeConfig,
    /// How much to log.
    pub log: LogConfig,
    /// Settings that belong to modules rather than to the core.
    ///
    /// The core does not read these — it carries them. What a section means
    /// is the module's business, which is why the value stays untyped here.
    /// See `docs/module-system.md`.
    pub modules: ModuleSettings,
}

/// The `[modules.<name>]` sections, kept as they were written.
///
/// A `BTreeMap` rather than a `HashMap` so the order is stable when the
/// configuration is written back out or compared in a test.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(transparent)]
pub struct ModuleSettings(BTreeMap<String, serde_json::Value>);

impl ModuleSettings {
    /// Reads one module's section into its own settings type.
    ///
    /// A missing section yields the type's default, so a module works without
    /// being configured. A section that does not fit is an error rather than
    /// a silent fallback: a misspelled option that quietly does nothing is the
    /// same trap `deny_unknown_fields` exists to prevent elsewhere.
    pub fn get<T>(&self, module: &str) -> Result<T, ModuleSettingsError>
    where
        T: Default + serde::de::DeserializeOwned,
    {
        let Some(section) = self.0.get(module) else {
            return Ok(T::default());
        };

        serde_json::from_value(section.clone()).map_err(|error| ModuleSettingsError {
            module: module.to_owned(),
            reason: error.to_string(),
        })
    }

    /// Stores a section. Used by tests and by anything assembling settings by
    /// hand rather than from a file.
    pub fn set(&mut self, module: &str, value: serde_json::Value) {
        self.0.insert(module.to_owned(), value);
    }
}

/// A module's section could not be read.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("the [modules.{module}] section could not be read: {reason}")]
pub struct ModuleSettingsError {
    /// Which module's section.
    pub module: String,
    /// What serde complained about.
    pub reason: String,
}

/// Settings for the HTTP server.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    /// Address the web interface binds to.
    ///
    /// Defaults to localhost on purpose: MeshDash belongs behind a reverse
    /// proxy, not unprotected on the network. See `SECURITY.md`.
    pub bind: SocketAddr,
}

/// Settings for authentication.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct AuthConfig {
    /// Optional bearer token. `None` means no authentication.
    ///
    /// See ADR-0006. Safe only in combination with a loopback `bind`, which is
    /// why [`Config::check_exposure`] refuses the other combination.
    pub token: Option<String>,

    /// Permits listening on a public address without a token.
    ///
    /// For a deployment behind a reverse proxy that authenticates instead.
    /// Deliberately an explicit switch: forgetting the token must not look the
    /// same as choosing to do without it.
    pub allow_unauthenticated: bool,
}

/// Settings for storage.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct DatabaseConfig {
    /// Path of the SQLite file. Created on first start.
    pub path: PathBuf,
}

/// Which transport connects to the node, and how.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct NodeConfig {
    /// Which way to reach the node.
    pub transport: TransportKind,
    /// Used when [`NodeConfig::transport`] is [`TransportKind::Serial`].
    pub serial: SerialConfig,
    /// Used when [`NodeConfig::transport`] is [`TransportKind::Tcp`].
    pub tcp: TcpConfig,
}

/// The kinds of connection MeshDash can open.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum TransportKind {
    /// A node on a USB port.
    #[default]
    Serial,
    /// A node reachable over the network.
    Tcp,
}

/// Settings for a node on a serial port.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct SerialConfig {
    /// Device path, for instance `/dev/ttyUSB0`.
    pub port: String,
    /// Baud rate; the node's own default is 115200.
    pub baud: u32,
}

/// Settings for a node reachable over TCP.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct TcpConfig {
    /// Host name or address of the node.
    pub host: String,
    /// Port the node listens on.
    pub port: u16,
}

/// Settings for logging.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct LogConfig {
    /// Filter in the same syntax as `RUST_LOG`.
    pub filter: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            // Localhost only. Widening this is a deliberate act by an operator
            // who has put a reverse proxy in front, not something that happens
            // by leaving the file untouched.
            bind: SocketAddr::from(([127, 0, 0, 1], 8080)),
        }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::from("data/meshdash.db"),
        }
    }
}

impl Default for SerialConfig {
    fn default() -> Self {
        Self {
            port: "/dev/ttyUSB0".to_owned(),
            // The rate the firmware opens its USB console at, see
            // `meshdash_transport::serial::DEFAULT_BAUD_RATE`.
            baud: 115_200,
        }
    }
}

impl Default for TcpConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_owned(),
            port: 5000,
        }
    }
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            filter: "meshdash=info".to_owned(),
        }
    }
}

impl AuthConfig {
    /// Whether a usable token is configured.
    ///
    /// An empty or blank token counts as none: in a file it reads like a
    /// setting that was made, but it protects nothing.
    pub fn is_protected(&self) -> bool {
        self.configured_token().is_some()
    }

    /// The token to compare against, if there is a usable one.
    pub fn configured_token(&self) -> Option<&str> {
        self.token
            .as_deref()
            .map(str::trim)
            .filter(|token| !token.is_empty())
    }
}

/// The service would be reachable from outside without any authentication.
///
/// Its own type rather than a variant of [`ConfigError`], because it is not a
/// malformed configuration — every value is valid on its own. Only the
/// combination is dangerous.
#[derive(Debug, thiserror::Error)]
#[error(
    "refusing to listen on {bind} without authentication: set [auth] token, \
     or set [auth] allow_unauthenticated = true if something in front of \
     MeshDash authenticates instead"
)]
pub struct UnprotectedExposure {
    /// The address that would have been exposed.
    pub bind: SocketAddr,
}

/// Why a configuration could not be assembled.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The file or the environment held something unusable.
    ///
    /// Boxed because figment's error carries a lot of context; unboxed it would
    /// bloat every `Result` in this module.
    #[error("configuration is invalid")]
    Invalid(#[from] Box<figment::Error>),
}

impl Config {
    /// Loads settings from [`DEFAULT_FILE`] and the environment.
    ///
    /// A missing file is fine — the defaults then apply unchanged.
    pub fn load() -> Result<Self, ConfigError> {
        Self::load_from(DEFAULT_FILE)
    }

    /// Loads settings from a specific file and the environment.
    pub fn load_from(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let config = Figment::from(Serialized::defaults(Self::default()))
            // Plain, not `nested`: the top-level tables are sections of one
            // configuration, not figment profiles.
            .merge(Toml::file(path.as_ref()))
            // The double underscore separates levels, so MESHDASH_NODE__SERIAL__PORT
            // reaches `node.serial.port`.
            .merge(Env::prefixed(ENV_PREFIX).split("__"))
            .extract()
            .map_err(Box::new)?;

        Ok(config)
    }

    /// Fails if the service would be reachable from outside unprotected.
    ///
    /// See ADR-0006. The accident this prevents is opening `bind` to reach the
    /// dashboard from another machine and not thinking about the token —
    /// which would expose sending into the mesh and, later, repeater
    /// administration.
    pub fn check_exposure(&self) -> Result<(), UnprotectedExposure> {
        let reachable_from_outside = !self.server.bind.ip().is_loopback();

        if reachable_from_outside && !self.auth.is_protected() && !self.auth.allow_unauthenticated {
            return Err(UnprotectedExposure {
                bind: self.server.bind,
            });
        }

        Ok(())
    }
}

#[cfg(test)]
// The closure signature is figment's, not ours: `Jail::expect_with` requires a
// `Result<(), figment::Error>`, and that error is what the lint objects to.
#[allow(clippy::result_large_err)]
mod tests {
    use super::*;
    use figment::Jail;

    #[test]
    fn starts_without_a_configuration_file() {
        Jail::expect_with(|_| {
            let config = Config::load().unwrap();

            assert_eq!(config, Config::default());
            Ok(())
        });
    }

    #[test]
    fn binds_to_localhost_by_default() {
        // A wrong default here would expose an operator's mesh to the network.
        assert_eq!(
            Config::default().server.bind.ip().to_string(),
            "127.0.0.1",
            "the default must not be reachable from outside"
        );
    }

    #[test]
    fn has_no_authentication_token_by_default() {
        assert_eq!(Config::default().auth.token, None);
    }

    /// Builds a configuration bound to `bind`, with the given auth settings.
    fn exposed(bind: &str, token: Option<&str>, allow: bool) -> Config {
        Config {
            server: ServerConfig {
                bind: bind.parse().unwrap(),
            },
            auth: AuthConfig {
                token: token.map(str::to_owned),
                allow_unauthenticated: allow,
            },
            ..Config::default()
        }
    }

    #[test]
    fn the_default_configuration_is_allowed_to_start() {
        Config::default().check_exposure().unwrap();
    }

    #[test]
    fn allows_loopback_without_a_token() {
        // Only reachable by someone already on the machine.
        exposed("127.0.0.1:8080", None, false)
            .check_exposure()
            .unwrap();
        exposed("[::1]:8080", None, false).check_exposure().unwrap();
        exposed("127.0.0.5:8080", None, false)
            .check_exposure()
            .unwrap();
    }

    #[test]
    fn refuses_a_public_address_without_a_token() {
        let error = exposed("0.0.0.0:8080", None, false)
            .check_exposure()
            .unwrap_err();

        // 0.0.0.0 is reachable from outside, however local it looks.
        assert!(error.to_string().contains("0.0.0.0:8080"));
    }

    #[test]
    fn refuses_every_kind_of_public_address_without_a_token() {
        for bind in ["0.0.0.0:8080", "192.168.1.10:8080", "[::]:8080"] {
            assert!(
                exposed(bind, None, false).check_exposure().is_err(),
                "{bind} must not be served unprotected"
            );
        }
    }

    #[test]
    fn allows_a_public_address_with_a_token() {
        exposed("0.0.0.0:8080", Some("secret"), false)
            .check_exposure()
            .unwrap();
    }

    #[test]
    fn allows_a_public_address_when_it_was_asked_for() {
        // The reverse proxy case from ADR-0006.
        exposed("0.0.0.0:8080", None, true)
            .check_exposure()
            .unwrap();
    }

    #[test]
    fn treats_an_empty_token_as_no_token() {
        // An empty string in the file reads as "I set it" but protects nothing.
        assert!(
            exposed("0.0.0.0:8080", Some(""), false)
                .check_exposure()
                .is_err()
        );
        assert!(
            exposed("0.0.0.0:8080", Some("   "), false)
                .check_exposure()
                .is_err(),
            "whitespace is not a token either"
        );
    }

    #[test]
    fn carries_a_module_section_from_the_file() {
        // The core does not know what these mean — it only has to not reject
        // them. Before this existed, deny_unknown_fields refused to start at
        // the sight of a [modules.…] section.
        Jail::expect_with(|jail| {
            jail.create_file(
                DEFAULT_FILE,
                r#"
                [modules.telemetry]
                neighbours = true
                every_minutes = 45
                "#,
            )?;

            let config = Config::load().unwrap();
            let section: serde_json::Value = config.modules.get("telemetry").unwrap();

            assert_eq!(section["neighbours"], serde_json::json!(true));
            assert_eq!(section["every_minutes"], serde_json::json!(45));
            Ok(())
        });
    }

    #[test]
    fn reads_settings_from_the_file() {
        Jail::expect_with(|jail| {
            jail.create_file(
                DEFAULT_FILE,
                r#"
                [server]
                bind = "0.0.0.0:9000"

                [node]
                transport = "tcp"

                [node.tcp]
                host = "192.168.1.50"
                port = 5000
                "#,
            )?;

            let config = Config::load().unwrap();

            assert_eq!(config.server.bind.to_string(), "0.0.0.0:9000");
            assert_eq!(config.node.transport, TransportKind::Tcp);
            assert_eq!(config.node.tcp.host, "192.168.1.50");
            Ok(())
        });
    }

    #[test]
    fn keeps_defaults_for_settings_the_file_omits() {
        Jail::expect_with(|jail| {
            jail.create_file(DEFAULT_FILE, "[log]\nfilter = \"debug\"\n")?;

            let config = Config::load().unwrap();

            assert_eq!(config.log.filter, "debug");
            assert_eq!(
                config.server.bind,
                Config::default().server.bind,
                "an unmentioned section must keep its default"
            );
            Ok(())
        });
    }

    #[test]
    fn lets_the_environment_win_over_the_file() {
        Jail::expect_with(|jail| {
            jail.create_file(DEFAULT_FILE, "[server]\nbind = \"127.0.0.1:1111\"\n")?;
            jail.set_env("MESHDASH_SERVER__BIND", "127.0.0.1:2222");

            let config = Config::load().unwrap();

            assert_eq!(config.server.bind.to_string(), "127.0.0.1:2222");
            Ok(())
        });
    }

    #[test]
    fn reads_nested_settings_from_the_environment() {
        Jail::expect_with(|jail| {
            jail.set_env("MESHDASH_NODE__SERIAL__PORT", "/dev/ttyACM0");

            let config = Config::load().unwrap();

            assert_eq!(config.node.serial.port, "/dev/ttyACM0");
            Ok(())
        });
    }

    #[test]
    fn rejects_a_setting_nobody_knows() {
        // A typo must be reported, not silently ignored: a misspelled `bind`
        // would otherwise leave the server on a different address than the
        // operator intended.
        Jail::expect_with(|jail| {
            jail.create_file(DEFAULT_FILE, "[server]\nbnid = \"0.0.0.0:9000\"\n")?;

            assert!(Config::load().is_err());
            Ok(())
        });
    }

    #[test]
    fn reports_a_malformed_file() {
        Jail::expect_with(|jail| {
            jail.create_file(DEFAULT_FILE, "this is not toml at all")?;

            assert!(Config::load().is_err());
            Ok(())
        });
    }

    #[test]
    fn reads_a_file_from_an_explicit_path() {
        Jail::expect_with(|jail| {
            jail.create_file("elsewhere.toml", "[log]\nfilter = \"trace\"\n")?;

            let config = Config::load_from("elsewhere.toml").unwrap();

            assert_eq!(config.log.filter, "trace");
            Ok(())
        });
    }
}

#[cfg(test)]
mod module_settings_tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Default, Deserialize, PartialEq)]
    #[serde(default, deny_unknown_fields)]
    struct Example {
        enabled: bool,
        every_minutes: u32,
    }

    #[test]
    fn a_module_without_a_section_gets_its_defaults() {
        // A module has to work unconfigured; that is what makes a section
        // optional rather than required.
        let settings = ModuleSettings::default();

        assert_eq!(
            settings.get::<Example>("telemetry").unwrap(),
            Example::default()
        );
    }

    #[test]
    fn reads_a_section_into_the_module_type() {
        let mut settings = ModuleSettings::default();
        settings.set(
            "telemetry",
            serde_json::json!({ "enabled": true, "every_minutes": 30 }),
        );

        assert_eq!(
            settings.get::<Example>("telemetry").unwrap(),
            Example {
                enabled: true,
                every_minutes: 30
            }
        );
    }

    #[test]
    fn a_section_that_does_not_fit_is_an_error() {
        // Not a silent fallback to defaults: an option nobody notices is
        // wrong is the trap deny_unknown_fields exists to prevent.
        let mut settings = ModuleSettings::default();
        settings.set("telemetry", serde_json::json!({ "enabeld": true }));

        let error = settings.get::<Example>("telemetry").unwrap_err();

        assert_eq!(error.module, "telemetry");
        assert!(error.reason.contains("enabeld"), "names the offending key");
    }

    #[test]
    fn one_module_cannot_see_another_section() {
        let mut settings = ModuleSettings::default();
        settings.set("nodes", serde_json::json!({ "enabled": true }));

        assert_eq!(
            settings.get::<Example>("telemetry").unwrap(),
            Example::default()
        );
    }
}
