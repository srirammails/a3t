//! ClawdFlow CE Binary
//!
//! Single-process workflow engine with MCP protocol support.
//!
//! ```bash
//! # Stdio mode (Claude Desktop)
//! clawdflow
//!
//! # HTTP/SSE mode (remote access)
//! clawdflow --mode http --port 3002
//! ```

use std::sync::Arc;
use flowr_ce::{init_storage, build_ce_router, restore_triggers, spawn_worker, serve, run_stdio, ServerConfig};
use tracing::{info, error};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let args: Vec<String> = std::env::args().collect();
    let mode = get_arg(&args, "--mode").unwrap_or_else(|| "stdio".to_string());
    let port: u16 = get_arg(&args, "--port")
        .and_then(|p| p.parse().ok())
        .unwrap_or(3002);

    info!(mode = %mode, "Starting ClawdFlow MCP server");

    let config = ServerConfig {
        port,
        mode: mode.clone(),
        data_dir: std::env::var("CLAWDFLOW_DATA_DIR").unwrap_or_else(|_| {
            dirs::data_local_dir()
                .map(|p| p.join("clawdflow").to_string_lossy().to_string())
                .unwrap_or_else(|| ".clawdflow".to_string())
        }),
        ui_enabled: false,
    };

    let storage = init_storage(&config.data_dir).await?;

    info!("ClawdFlow initialized successfully");

    // Start background worker
    spawn_worker(storage.clone());

    match mode.as_str() {
        "http" | "sse" => {
            restore_triggers(&storage).await;
            let router = build_ce_router(storage);
            serve(router, port).await?;
        }
        _ => {
            run_stdio(storage).await?;
        }
    }

    info!("ClawdFlow shutting down");
    Ok(())
}

fn get_arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}
