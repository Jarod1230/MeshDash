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
    /// The final shape of authentication is undecided and needs its own ADR,
    /// see the open points in `docs/architecture.md`.
    pub token: Option<String>,
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
