//! MeshDash binary.
//!
//! Wires the pieces together in the order they depend on each other:
//! configuration, storage, the connection to the node, the modules, then the
//! HTTP surface.
//!
//! # What it does not do yet
//!
//! No module is registered — there are none, they arrive in step 6 of
//! `docs/roadmap.md`. The server therefore answers every path with a 404 in
//! the agreed error shape. Authentication, the WebSocket stream and the
//! embedded frontend are still missing too; authentication needs an ADR first.

use std::{net::ToSocketAddrs, process::ExitCode};

use anyhow::Context;
use meshdash_core::{
    config::{Config, TransportKind},
    db::Database,
    event::EventBus,
    link::{self, LinkConfig},
    module::{AppContext, ModuleRegistry},
};
use meshdash_transport::{Transport, serial::SerialTransport, tcp::TcpTransport};
use tracing_subscriber::EnvFilter;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // A failure here means the service never came up. Reporting the
            // whole chain matters: "database operation failed" alone would not
            // say which path could not be opened.
            eprintln!("meshdash could not start: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// Everything that can fail before the server is listening.
fn run() -> anyhow::Result<()> {
    let config = Config::load().context("reading the configuration")?;
    init_tracing(&config.log.filter);

    tokio::runtime::Runtime::new()
        .context("starting the async runtime")?
        .block_on(serve(config))
}

/// Sets up logging. `RUST_LOG` wins over the configured filter.
fn init_tracing(filter: &str) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter));

    tracing_subscriber::fmt().with_env_filter(filter).init();
}

/// Brings the service up and serves until the process ends.
async fn serve(config: Config) -> anyhow::Result<()> {
    let db = Database::open(&config.database)
        .await
        .with_context(|| format!("opening the database at {}", config.database.path.display()))?;

    let transport = open_transport(&config).context("setting up the connection to the node")?;

    let events = EventBus::new();
    let (link, _link_task) = link::spawn(transport, LinkConfig::default(), events.clone());

    let context = AppContext { db, events, link };

    // Empty for now; modules register here from step 6 onwards.
    let registry = ModuleRegistry::new();
    registry
        .start_all(&context)
        .await
        .context("starting the modules")?;

    let router = meshdash_server::build_router(&registry, context);

    let listener = tokio::net::TcpListener::bind(config.server.bind)
        .await
        .with_context(|| format!("binding to {}", config.server.bind))?;

    tracing::info!(
        address = %config.server.bind,
        modules = registry.names().len(),
        "meshdash is listening"
    );

    axum::serve(listener, router)
        .await
        .context("serving HTTP")?;

    Ok(())
}

/// Builds the transport the configuration asks for.
///
/// No port or socket is opened here — the link connects, and reconnects, on
/// its own. Only a TCP host name is resolved right away, so a typo in the
/// configuration is reported at startup instead of hiding inside a reconnect
/// loop.
fn open_transport(config: &Config) -> anyhow::Result<Box<dyn Transport>> {
    let transport: Box<dyn Transport> = match config.node.transport {
        TransportKind::Serial => Box::new(SerialTransport::new(
            config.node.serial.port.clone(),
            config.node.serial.baud,
        )),
        TransportKind::Tcp => {
            let target = format!("{}:{}", config.node.tcp.host, config.node.tcp.port);
            let address = target
                .to_socket_addrs()
                .with_context(|| format!("resolving the node address {target}"))?
                .next()
                .with_context(|| format!("no address found for {target}"))?;

            Box::new(TcpTransport::new(address))
        }
    };

    Ok(transport)
}
