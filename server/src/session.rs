
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{info, debug, warn};

use crate::config::FreightConfig;
use crate::data::DataStore;
use crate::handler::Handler;
use crate::metrics::Metrics;
use crate::session_mgr::SessionManager;
use crate::protocol::{self, Cipher, Message, MessageReader, MessageWriter, ProtocolError, cmd};

/// Rate limiter đơn giản: sliding window
struct RateLimiter {
    /// Max requests trong window
    max_requests: u32,
    /// Window duration
    window_secs: u64,
    /// Timestamps của requests gần nhất
    timestamps: Vec<Instant>,
}

impl RateLimiter {
    fn new(max_requests: u32, window_secs: u64) -> Self {
        Self {
            max_requests,
            window_secs,
            timestamps: Vec::with_capacity(max_requests as usize),
        }
    }

    /// Check xem request có được phép không. Trả về true nếu OK.
    fn check(&mut self) -> bool {
        let now = Instant::now();
        let cutoff = now - std::time::Duration::from_secs(self.window_secs);
        
        // Xoá timestamps cũ hơn window
        self.timestamps.retain(|t| *t > cutoff);
        
        if self.timestamps.len() < self.max_requests as usize {
            self.timestamps.push(now);
            true
        } else {
            false
        }
    }

}

pub struct Session {
    stream: TcpStream,
    cipher: Cipher,
    handler: Handler,
    key_exchanged: bool,
    zoom_level: u8,
    config: Arc<FreightConfig>,
    // --- Monitoring ---
    session_id: u64,
    addr: SocketAddr,
    session_mgr: Arc<SessionManager>,
    metrics: Arc<Metrics>,
    rate_limiter: RateLimiter,
}

impl Session {
    pub fn new(
        stream: TcpStream,
        addr: SocketAddr,
        data_store: Arc<DataStore>,
        config: Arc<FreightConfig>,
        session_mgr: Arc<SessionManager>,
        metrics: Arc<Metrics>,
    ) -> Self {
        let session_id = session_mgr.register(addr);
        // Rate limit: max 200 requests per 10 seconds (configurable sau)
        let rate_limiter = RateLimiter::new(200, 10);
        
        Self {
            stream,
            cipher: Cipher::new(),
            handler: Handler::new(data_store, config.clone(), session_mgr.clone(), metrics.clone()),
            key_exchanged: false,
            zoom_level: config.server.default_zoom,
            config,
            session_id,
            addr,
            session_mgr,
            metrics,
            rate_limiter,
        }
    }

    pub async fn run(&mut self) -> Result<(), ProtocolError> {
        let idle_timeout = std::time::Duration::from_secs(
            self.config.server.idle_timeout_secs.unwrap_or(300)
        );
        
        loop {
            let message = match tokio::time::timeout(idle_timeout, self.read_message()).await {
                Ok(result) => result?,
                Err(_) => {
                    warn!("- Session #{} idle timeout ({}s)", self.session_id, idle_timeout.as_secs());
                    return Ok(());
                }
            };
            
            debug!("Session #{} received cmd: {}", self.session_id, message.command);
            
            if message.command != cmd::GET_SESSION_ID && message.command != cmd::FREIGHT_INIT {
                if !self.rate_limiter.check() {
                    self.metrics.on_rate_limited();
                    warn!("- Session #{} rate limited ({})", self.session_id, self.addr);
                    continue; // Drop request, don't disconnect
                }
            }
            
            // Update last activity
            self.session_mgr.touch(self.session_id);
            
            // Handle message
            let responses = self.handle_message(message).await?;
            
            // Send responses
            for response in responses {
                self.send_message(response.0, &response.1).await?;
            }
        }
    }

    async fn read_message(&mut self) -> Result<Message, ProtocolError> {
        let cmd_byte = self.stream.read_i8().await?;
        let mut bytes_read: u64 = 1;
        
        let command = if self.key_exchanged {
            self.cipher.decrypt_byte(cmd_byte)
        } else {
            cmd_byte
        };
        
        // === Đọc length (LUÔN 2 bytes từ client) ===
        let b1 = self.stream.read_u8().await?;
        let b2 = self.stream.read_u8().await?;
        bytes_read += 2;
        
        let length = if self.key_exchanged {
            // Big Endian + encrypted: high byte trước, low byte sau
            let l1 = (self.cipher.decrypt_byte(b1 as i8) as u8) as usize;
            let l2 = (self.cipher.decrypt_byte(b2 as i8) as u8) as usize;
            (l1 << 8) | l2
        } else {
            // ushort little-endian (như BinaryWriter.Write(ushort) trong C#)
            ((b2 as usize) << 8) | (b1 as usize)
        };

        // === Đọc data ===
        let mut data = vec![0u8; length];
        if length > 0 {
            self.stream.read_exact(&mut data).await?;
            bytes_read += length as u64;
            
            if self.key_exchanged {
                data = self.cipher.decrypt_bytes(&data);
            }
        }

        self.session_mgr.add_bytes_recv(self.session_id, bytes_read);

        Ok(Message::new(command, data))
    }

    async fn send_message(&mut self, command: i8, data: &[u8]) -> Result<(), ProtocolError> {
        let packet = protocol::build_response(&mut self.cipher, command, data);
        let packet_len = packet.len() as u64;
        
        // Debug: log first bytes for key exchange
        if command == cmd::GET_SESSION_ID {
            let hex: String = packet.iter().take(20).map(|b| format!("{:02X} ", b)).collect();
            debug!("- Sending key exchange packet (first 20 bytes): {}", hex);
        }
        
        self.stream.write_all(&packet).await?;
        self.stream.flush().await?;
        
        // Track bytes sent
        self.session_mgr.add_bytes_sent(self.session_id, packet_len);
        
        Ok(())
    }

    async fn handle_message(&mut self, msg: Message) -> Result<Vec<(i8, Vec<u8>)>, ProtocolError> {
        match msg.command {
            cmd::GET_SESSION_ID => {
                let raw_key = Cipher::generate_key_only(32);
                let mut writer = MessageWriter::new();
                writer.write_byte(raw_key.len() as i8);
                writer.write_sbytes(&raw_key);
                writer.write_utf("");
                writer.write_int(0);
                writer.write_byte(0); // isConnect2 = false
                
                let response_data = writer.into_bytes();
                let packet = protocol::build_response(&mut self.cipher, cmd::GET_SESSION_ID, &response_data);
                
                self.cipher.set_key_from_raw(&raw_key);
                self.key_exchanged = true;
                
                info!("- Session #{} key exchanged, length: {}", self.session_id, raw_key.len());
                self.stream.write_all(&packet).await?;
                self.stream.flush().await?;
                self.session_mgr.add_bytes_sent(self.session_id, packet.len() as u64);
                Ok(vec![])
            }

            cmd::FREIGHT_INIT => {
                let mut reader = MessageReader::new(&msg.data);
                let zoom = reader.read_byte() as u8;
                let screen_w = reader.read_int();
                let screen_h = reader.read_int();

                self.zoom_level = if zoom > 0 { zoom } else { self.config.server.default_zoom };
                
                // Update zoom trong session manager
                self.session_mgr.set_zoom(self.session_id, self.zoom_level);

                info!(
                    "- Session #{} FREIGHT_INIT: zoom={}, screen={}x{}",
                    self.session_id, self.zoom_level, screen_w, screen_h
                );

                let mut writer = MessageWriter::new();
                writer.write_byte(self.zoom_level as i8);
                Ok(vec![(cmd::FREIGHT_INIT, writer.into_bytes())])
            }
            
            _ => {
                self.session_mgr.on_request(self.session_id);
                self.metrics.increment_cmd(msg.command);
                
                self.handler.handle(msg, self.zoom_level, self.session_id).await
            }
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.session_mgr.unregister(self.session_id);
    }
}
