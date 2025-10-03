use anyhow::Result;
use rmcp::{transport::stdio, ServiceExt};
use tracing_subscriber::{self, EnvFilter};

mod process_manager;
mod process_server;
use process_server::ProcessServer;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize the tracing subscriber with stderr logging
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    tracing::info!("Starting Process Management MCP server");

    // Create an instance of our process server
    let service = ProcessServer::new()
        .await?
        .serve(stdio())
        .await
        .inspect_err(|e| {
            tracing::error!("serving error: {:?}", e);
        })?;

    service.waiting().await?;
    Ok(())
}
