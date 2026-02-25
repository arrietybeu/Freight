mod config;
mod protocol;
mod session;
mod handler;
mod data;
mod metrics;
mod session_mgr;

use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::io::AsyncWriteExt;
use tokio::signal;
use tracing::{info, warn, error, Level};
use tracing_subscriber::FmtSubscriber;

use config::FreightConfig;
use session::Session;
use data::DataStore;
use metrics::Metrics;
use session_mgr::SessionManager;

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

    // === Shared state ===
    let metrics = Arc::new(Metrics::new());
    let session_mgr = Arc::new(SessionManager::new(metrics.clone()));
    
    let max_cache_bytes = config.server.max_cache_mb.unwrap_or(512) as u64 * 1024 * 1024;
    let data_store = Arc::new(DataStore::new(metrics.clone(), max_cache_bytes));
    
    info!("- Cache limit: {}", Metrics::fmt_bytes(max_cache_bytes));

    data_store.scan_img_by_name(&config.paths).await;

    // === Periodic stats task ===
    let stats_interval_secs = config.server.stats_interval_secs.unwrap_or(60);
    {
        let metrics = metrics.clone();
        let session_mgr = session_mgr.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                std::time::Duration::from_secs(stats_interval_secs)
            );
            loop {
                interval.tick().await;
                metrics.log_stats();
                session_mgr.log_top_sessions(5);
            }
        });
    }
    
    let max_connections = config.server.max_connections;

    // === Accept loop with graceful shutdown ===
    let shutdown = async {
        // Ctrl+C
        signal::ctrl_c().await.ok();
        info!("- Shutdown signal received, stopping accept loop...");
    };
    
    tokio::select! {
        _ = async {
            loop {
                match listener.accept().await {
                    Ok((socket, addr)) => {
                        let current = session_mgr.active_count();
                        if current >= max_connections {
                            warn!("- Max connections reached ({}/{}), rejecting {} with thongbao", 
                                current, max_connections, addr);
                            tokio::spawn(async move {
                                let mut socket = socket;
                                let text = "Máy chủ đã quá tải, vui lòng quay lại sau!";
                                let text_bytes = text.as_bytes();
                                
                                let mut payload = Vec::with_capacity(2 + text_bytes.len());
                                payload.extend_from_slice(&(text_bytes.len() as i16).to_be_bytes());
                                payload.extend_from_slice(text_bytes);
                                
                                let mut packet = Vec::with_capacity(1 + 2 + payload.len());
                                packet.push(-26_i8 as u8);
                                packet.extend_from_slice(&(payload.len() as u16).to_be_bytes());
                                packet.extend_from_slice(&payload);
                                
                                let _ = socket.write_all(&packet).await;
                                let _ = socket.flush().await;
                                let _ = socket.shutdown().await;
                            });
                            continue;
                        }
                        
                        info!("- New connection from: {} ({}/{})", addr, current + 1, max_connections);
                        let store = Arc::clone(&data_store);
                        let cfg = Arc::clone(&config);
                        let sm = Arc::clone(&session_mgr);
                        let m = Arc::clone(&metrics);
                        
                        tokio::spawn(async move {
                            let mut session = Session::new(socket, addr, store, cfg, sm, m);
                            if let Err(e) = session.run().await {
                                // Connection reset / broken pipe are normal, don't log as error
                                let err_str = e.to_string();
                                if err_str.contains("connection reset") 
                                    || err_str.contains("broken pipe")
                                    || err_str.contains("unexpected eof") {
                                    info!("- Session disconnected: {} ({})", addr, e);
                                } else {
                                    error!("-  Session error for {}: {}", addr, e);
                                }
                            }
                            // Session::drop() sẽ tự unregister khỏi SessionManager
                        });
                    }
                    Err(e) => {
                        error!("Accept error: {}", e);
                    }
                }
            }
        } => {}
        _ = shutdown => {}
    }

    // Final stats
    info!("- Final server stats:");
    metrics.log_stats();
    info!("- Freight Data Server stopped.");
    
    Ok(())
}
