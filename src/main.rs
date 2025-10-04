use anyhow::Result;
use rmcp::{transport::stdio, ServiceExt};
use tracing_subscriber::{self, EnvFilter};

mod process_manager;
mod process_server;
use process_server::ProcessServer;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
async fn main() -> Result<()> {
    // Check for --version flag
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && (args[1] == "--version" || args[1] == "-V") {
        println!("{}", VERSION);
        return Ok(());
    }

    // Initialize the tracing subscriber with stderr logging
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    tracing::info!("Starting Process Management MCP server");

    // Create an instance of our process server
    let server = ProcessServer::new().await?;

    // Clone server for cleanup on shutdown
    let cleanup_server = server.clone();

    // Spawn a task to handle shutdown signals
    tokio::spawn(async move {
        // Wait for shutdown signal (Ctrl+C or SIGTERM)
        let ctrl_c = async {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to listen for Ctrl+C");
        };

        #[cfg(unix)]
        let terminate = async {
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("Failed to listen for SIGTERM")
                .recv()
                .await;
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            _ = ctrl_c => {
                tracing::info!("Received Ctrl+C signal");
            }
            _ = terminate => {
                tracing::info!("Received SIGTERM signal");
            }
        }

        // Cleanup all processes
        if let Err(e) = cleanup_server.cleanup().await {
            tracing::error!("Error during cleanup: {:?}", e);
        }

        std::process::exit(0);
    });

    let service = server.serve(stdio()).await.inspect_err(|e| {
        tracing::error!("serving error: {:?}", e);
    })?;

    service.waiting().await?;
    Ok(())
}
