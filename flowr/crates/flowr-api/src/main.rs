//! FlowR Server
//!
//! HTTP Admin API server for workflow management.
//!
//! Supports two storage modes:
//! - `dragonfly`: DragonflyDB + Iggy (distributed)
//! - `adb`: ClawDB/LanceDB (Agent Database, single-node)

use clap::{Parser, ValueEnum};
use flowr_api::{create_router, AppState};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[cfg(feature = "dragonfly")]
use flowr_hub::{HubRegistry, init_builtins};
#[cfg(feature = "dragonfly")]
use flowr_registry::FlowRegistry;
#[cfg(feature = "dragonfly")]
use iggy::clients::client::IggyClient;
#[cfg(feature = "dragonfly")]
use iggy::client::{Client, UserClient};

#[cfg(feature = "adb")]
use flowr_ce_storage::LanceStorage;
#[cfg(feature = "adb")]
use flowr_storage::StorageBackend;

/// Storage backend mode
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum StorageMode {
    /// DragonflyDB + Iggy (distributed mode)
    #[default]
    Dragonfly,
    /// ClawDB/LanceDB Agent Database (single-node mode)
    Adb,
}

/// FlowR API Server
#[derive(Parser, Debug)]
#[command(name = "flowr-server")]
#[command(about = "FlowR API Server - HTTP Admin API for workflow management")]
struct Cli {
    /// Server port
    #[arg(short, long, env = "PORT", default_value = "8080")]
    port: u16,

    /// Storage mode
    #[arg(short, long, env = "STORAGE_MODE", value_enum, default_value = "dragonfly")]
    storage: StorageMode,

    /// Dragonfly URL (for dragonfly mode)
    #[arg(long, env = "DRAGONFLY_URL", default_value = "redis://127.0.0.1:6379")]
    dragonfly_url: String,

    /// Iggy URL (for dragonfly mode)
    #[arg(long, env = "IGGY_URL", default_value = "127.0.0.1:8090")]
    iggy_url: String,

    /// Iggy username (for dragonfly mode)
    #[arg(long, env = "IGGY_USERNAME", default_value = "iggy")]
    iggy_username: String,

    /// Iggy password (for dragonfly mode)
    #[arg(long, env = "IGGY_PASSWORD", default_value = "iggy")]
    iggy_password: String,

    /// ADB (ClawDB) data path (for adb mode)
    #[arg(long, env = "ADB_PATH", default_value = "./flowr_data")]
    adb_path: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            EnvFilter::new("flowr_api=debug,flowr_registry=debug,tower_http=debug,info")
        }))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();

    info!("Starting FlowR API server");
    info!(mode = ?cli.storage, "Storage mode");

    match cli.storage {
        StorageMode::Dragonfly => {
            #[cfg(feature = "dragonfly")]
            {
                run_dragonfly_mode(cli).await?;
            }
            #[cfg(not(feature = "dragonfly"))]
            {
                anyhow::bail!("Dragonfly mode not enabled. Rebuild with --features dragonfly");
            }
        }
        StorageMode::Adb => {
            #[cfg(feature = "adb")]
            {
                run_adb_mode(cli).await?;
            }
            #[cfg(not(feature = "adb"))]
            {
                anyhow::bail!("ADB mode not enabled. Rebuild with --features adb");
            }
        }
    }

    info!("Server shut down gracefully");
    Ok(())
}

/// Run server with Dragonfly + Iggy backend
#[cfg(feature = "dragonfly")]
async fn run_dragonfly_mode(cli: Cli) -> anyhow::Result<()> {
    // Connect to Dragonfly
    info!(url = %cli.dragonfly_url, "Connecting to Dragonfly");
    let redis_client = redis::Client::open(cli.dragonfly_url.as_str())?;
    let redis = redis_client.get_multiplexed_async_connection().await?;
    info!("Connected to Dragonfly");

    // Connect to Iggy
    info!(url = %cli.iggy_url, "Connecting to Iggy");
    let iggy = IggyClient::builder()
        .with_tcp()
        .with_server_address(cli.iggy_url)
        .build()?;
    iggy.connect().await?;

    // Authenticate to Iggy
    iggy.login_user(&cli.iggy_username, &cli.iggy_password).await?;
    let iggy = Arc::new(iggy);
    info!("Connected and authenticated to Iggy");

    // Create registries
    let registry = FlowRegistry::new(redis.clone());
    let hub = HubRegistry::new(redis.clone());

    // Initialize built-in resource types
    init_builtins(&hub).await?;
    info!("Initialized built-in resource types");

    // Create app state
    let state = AppState::new(registry, hub, redis, iggy);

    // Create router and start server
    let app = create_router(state);
    let addr = SocketAddr::from(([0, 0, 0, 0], cli.port));
    info!(%addr, "Starting HTTP server (Dragonfly mode)");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

/// Run server with ADB (ClawDB/LanceDB) backend
#[cfg(feature = "adb")]
async fn run_adb_mode(cli: Cli) -> anyhow::Result<()> {
    use flowr_api::AppStateAdb;

    // Create data directory if needed
    std::fs::create_dir_all(&cli.adb_path)?;

    let db_path = format!("{}/flowr.lance", cli.adb_path);
    info!(path = %db_path, "Opening ADB (ClawDB/LanceDB)");

    // Initialize LanceDB storage
    let storage = LanceStorage::new(&db_path)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let storage = Arc::new(storage);

    // Initialize storage tables
    storage.init().await.map_err(|e| anyhow::anyhow!("{}", e))?;
    info!("ADB storage initialized");

    // Create app state with ADB backend
    let state = AppStateAdb::new(storage);

    // Create router and start server
    let app = flowr_api::create_router_adb(state);
    let addr = SocketAddr::from(([0, 0, 0, 0], cli.port));
    info!(%addr, "Starting HTTP server (ADB mode)");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

/// Shutdown signal handler
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            warn!("Received Ctrl+C, shutting down");
        }
        _ = terminate => {
            warn!("Received SIGTERM, shutting down");
        }
    }
}
