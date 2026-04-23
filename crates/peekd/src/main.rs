mod cache;
mod config;
mod frecency;
mod scanner;
mod server;
mod watcher;

use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use tokio::net::UnixListener;
use tokio::sync::RwLock;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("peekd=info".parse().unwrap()))
        .init();

    // Ensure data directory exists
    let data_dir = config::data_dir();
    std::fs::create_dir_all(&data_dir)
        .context("failed to create ~/.peek directory")?;
    std::fs::create_dir_all(config::log_dir())
        .context("failed to create log directory")?;

    // Load config
    let config = Arc::new(config::load_config().context("failed to load config")?);
    info!("config loaded");

    // Write PID file
    let pid = std::process::id();
    std::fs::write(config::pid_path(), pid.to_string())
        .context("failed to write PID file")?;

    // Remove stale socket
    let socket_path = config::socket_path();
    if socket_path.exists() {
        std::fs::remove_file(&socket_path).ok();
    }

    // Bind Unix socket
    let listener = UnixListener::bind(&socket_path)
        .context("failed to bind Unix socket")?;
    info!("listening on {}", socket_path.display());

    // Initialize components
    let cache = Arc::new(RwLock::new(cache::DirectoryCache::new()));
    let scanner = Arc::new(scanner::Scanner::new(cache.clone(), config.clone()));

    let db_path = data_dir.join("history.db");
    let frecency = Arc::new(Mutex::new(
        frecency::FrecencyEngine::new(&db_path, config.frecency.clone())
            .context("failed to initialize frecency engine")?,
    ));

    let (file_watcher, mut invalidate_rx) = watcher::FileWatcher::new(cache.clone())
        .context("failed to initialize file watcher")?;
    let file_watcher = Arc::new(RwLock::new(file_watcher));

    // Spawn invalidation handler
    let scanner_clone = scanner.clone();
    tokio::spawn(async move {
        while let Some(dir) = invalidate_rx.recv().await {
            info!("re-scanning {} after file change", dir.display());
            scanner_clone.invalidate(&dir).await;
        }
    });

    // Run server
    let server = server::Server::new(listener, scanner, frecency, config, file_watcher);
    server.run().await
}
