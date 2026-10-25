//! rs-mcp: an MCP server that lets AI clients work with a notes database.
//!
//! The server speaks MCP over stdio: an MCP client launches this binary and
//! exchanges JSON-RPC messages with it on stdin and stdout. Because stdout
//! carries the protocol, every log line goes to stderr.

use anyhow::Context;
use rmcp::{ServiceExt, service::ServerInitializeError, transport::stdio};
use rs_mcp::{config::Config, db, server::NotesServer};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // A local .env is optional; variables already in the environment win.
    let dotenv = dotenvy::dotenv();
    init_logging();
    if let Err(error) = dotenv
        && !error.not_found()
    {
        tracing::warn!(%error, "ignoring unreadable .env file");
    }

    let config = Config::from_env()?;
    let pool = db::connect(&config.database_url)
        .await
        .context("cannot connect to PostgreSQL (check DATABASE_URL)")?;
    db::migrate(&pool)
        .await
        .context("database migrations failed")?;
    tracing::info!("database ready");

    let service = match NotesServer.serve(stdio()).await {
        Ok(service) => service,
        // A client may probe the server (server/discover) and leave without
        // starting a session; that is a normal disconnect, not a failure.
        Err(ServerInitializeError::ConnectionClosed(_)) => {
            tracing::info!("client disconnected before starting a session");
            pool.close().await;
            return Ok(());
        }
        Err(error) => return Err(error).context("failed to start the MCP service on stdio"),
    };
    tracing::info!("MCP server ready on stdio");

    // Ctrl-C / SIGTERM cancel the service; a client that closes stdin ends it too.
    let cancel = service.cancellation_token();
    tokio::spawn(async move {
        shutdown_signal().await;
        tracing::info!("shutdown signal received");
        cancel.cancel();
    });

    let reason = service.waiting().await.context("MCP service task failed")?;
    tracing::info!(?reason, "MCP server stopped");
    pool.close().await;
    Ok(())
}

/// Logs to stderr, filtered by `RUST_LOG` (default `info`).
fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut sigterm) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = sigterm.recv() => {}
                }
            }
            Err(error) => {
                tracing::warn!(%error, "cannot listen for SIGTERM; Ctrl-C still works");
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
