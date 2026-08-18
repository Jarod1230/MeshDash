//! MeshDash binary.
//!
//! Wires the pieces together in the order they depend on each other:
//! configuration, storage, the connection to the node, the modules, then the
//! HTTP surface.
//!
//! # What it does not do yet
//!
//! All four modules of step 6 are registered. The dashboard is still a
//! placeholder page — that is step 7 of `docs/roadmap.md`.

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

    // Before anything is opened: a service reachable from outside without
    // authentication must not come up at all. See ADR-0006.
    config.check_exposure()?;

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

    // Built but not started: the modules must be listening before the link
    // reports its first connection. The bus keeps no backlog, so an event sent
    // now would be lost and the status would stay wrong until the next
    // disconnect.
    let (link, prepared_link) = link::prepare(transport, LinkConfig::default(), events.clone());

    let context = AppContext { db, events, link };

    // The one place modules are switched on. Removing one means deleting its
    // line here and its directory — nothing else, see docs/module-system.md.
    let mut registry = ModuleRegistry::new();
    registry
        .register(Box::new(meshdash_modules::system::SystemModule))
        .context("registering the system module")?;
    registry
        .register(Box::new(meshdash_modules::nodes::NodesModule))
        .context("registering the nodes module")?;
    registry
        .register(Box::new(meshdash_modules::messages::MessagesModule))
        .context("registering the messages module")?;
    registry
        .register(Box::new(meshdash_modules::telemetry::TelemetryModule))
        .context("registering the telemetry module")?;

    registry
        .start_all(&context)
        .await
        .context("starting the modules")?;

    // Everyone is listening now.
    let link_task = prepared_link.start();

    let router = meshdash_server::build_router(&registry, context, config.auth.clone());

    let listener = tokio::net::TcpListener::bind(config.server.bind)
        .await
        .with_context(|| format!("binding to {}", config.server.bind))?;

    tracing::info!(
        address = %config.server.bind,
        modules = registry.names().len(),
        "meshdash is listening"
    );

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("serving HTTP")?;

    // The link owns the connection to the node; ending it releases the serial
    // port, which matters when a service manager restarts us right away.
    link_task.abort();
    tracing::info!("meshdash stopped");

    Ok(())
}

/// Resolves when the process is asked to stop.
///
/// Both signals matter: Ctrl-C for someone running it in a terminal, SIGTERM
/// for a service manager. Without SIGTERM the process would be killed outright
/// after a grace period, cutting requests off mid-answer.
async fn shutdown_signal() {
    let interrupt = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            // Without the handler the default behaviour still applies, so
            // there is nothing to recover from here.
            Err(error) => {
                tracing::warn!(%error, "could not listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = interrupt => tracing::info!("received interrupt, shutting down"),
        () = terminate => tracing::info!("received terminate, shutting down"),
    }
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
