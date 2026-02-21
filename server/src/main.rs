mod config;
mod protocol;
mod session;
mod handler;
mod data;

use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{info, error, Level};
use tracing_subscriber::FmtSubscriber;

use config::FreightConfig;
use session::Session;
use data::DataStore;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // Load config
    let config = Arc::new(FreightConfig::load("config.yml"));
    info!("- Config: {:?}", config.server);
    info!("- Base dir: {}", config.paths.base_dir);

    let addr = config.bind_addr();
    let listener = TcpListener::bind(&addr).await?;
    
    info!("- Freight Data Server started on {}", addr);

    let data_store = Arc::new(DataStore::new());
    
    loop {
        match listener.accept().await {
            Ok((socket, addr)) => {
                info!("- New connection from: {}", addr);
                let store = Arc::clone(&data_store);
                let cfg = Arc::clone(&config);
                
                tokio::spawn(async move {
                    let mut session = Session::new(socket, store, cfg);
                    if let Err(e) = session.run().await {
                        error!("Session error for {}: {}", addr, e);
                    }
                    info!("- Connection closed: {}", addr);
                });
            }
            Err(e) => {
                error!("Accept error: {}", e);
            }
        }
    }
}
