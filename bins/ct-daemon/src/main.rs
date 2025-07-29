mod server;
mod state;

use clap::Parser;
use ct_core::{config::Config, compute_workspace_fingerprint, utils::find_workspace_root};
use ct_db::Database;
use ct_indexer::{Indexer, watcher::spawn_watcher};
use std::path::PathBuf;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug)]
#[command(name = "ct-daemon")]
#[command(about = "ct indexing daemon", version)]
struct Args {
    /// Path to workspace to index
    #[arg(long = "idx", value_name = "PATH")]
    workspace: Option<PathBuf>,

    /// Features to enable (can be specified multiple times)
    #[arg(long, value_name = "FEATURE")]
    features: Vec<String>,

    /// Target triple
    #[arg(long, value_name = "TARGET")]
    target: Option<String>,

    /// Transport type (auto, unix, pipe, tcp)
    #[arg(long, default_value = "auto")]
    transport: String,

    /// Run once and exit
    #[arg(long)]
    once: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ct_daemon=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();
    
    let workspace_root = if let Some(path) = args.workspace {
        path.canonicalize()?
    } else {
        find_workspace_root(&std::env::current_dir()?)?
    };
    
    info!("Starting ct-daemon for workspace: {:?}", workspace_root);
    
    let config = Config::load()?;
    let workspace_fingerprint = compute_workspace_fingerprint(&workspace_root);
    
    // Create cache directory
    let cache_dir = config.get_cache_dir(&workspace_fingerprint);
    std::fs::create_dir_all(&cache_dir)?;
    
    // Open database
    let db_path = config.get_db_path(&workspace_fingerprint);
    info!("Opening database at {:?}", db_path);
    let db = Database::open(&db_path)?;
    
    // Create indexer and perform initial indexing
    let mut indexer = Indexer::new(workspace_root.clone(), db);
    
    info!("Starting initial indexing...");
    let stats = indexer.index_workspace().await?;
    info!(
        "Initial indexing complete: {} crates, {} files, {} symbols in {}ms",
        stats.crates_indexed, stats.files_indexed, stats.symbols_indexed, stats.duration_ms
    );
    
    if args.once {
        info!("Running in --once mode, exiting");
        return Ok(());
    }
    
    // Start file watcher
    let watcher_handle = spawn_watcher(workspace_root.clone(), config.watcher_debounce_ms).await?;
    
    // Start IPC server
    let server_handle = server::start_server(config, workspace_fingerprint).await?;
    
    info!("Daemon started, waiting for shutdown signal...");
    
    // Wait for shutdown signal
    #[cfg(unix)]
    {
        use tokio::signal;
        tokio::select! {
            _ = signal::ctrl_c() => {
                info!("Received Ctrl+C, shutting down...");
            }
            _ = async {
                signal::unix::signal(signal::unix::SignalKind::terminate())
                    .expect("Failed to install SIGTERM handler")
                    .recv()
                    .await
            } => {
                info!("Received SIGTERM, shutting down...");
            }
        }
    }
    
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await?;
        info!("Received Ctrl+C, shutting down...");
    }
    
    // Cleanup
    watcher_handle.stop().await?;
    server_handle.shutdown().await?;
    
    info!("Daemon shutdown complete");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn test_cli_args() {
        Args::command().debug_assert();
    }
}