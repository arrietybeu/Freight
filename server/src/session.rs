
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{info, debug, warn};

use crate::config::FreightConfig;
use crate::data::DataStore;
use crate::handler::Handler;
use crate::protocol::{self, Cipher, Message, MessageReader, MessageWriter, ProtocolError, cmd};

pub struct Session {
    stream: TcpStream,
    cipher: Cipher,
    handler: Handler,
    key_exchanged: bool,
    zoom_level: u8,
    config: Arc<FreightConfig>,
}

impl Session {
    pub fn new(stream: TcpStream, data_store: Arc<DataStore>, config: Arc<FreightConfig>) -> Self {
        Self {
            stream,
            cipher: Cipher::new(),
            handler: Handler::new(data_store, config.clone()),
            key_exchanged: false,
            zoom_level: config.server.default_zoom,
            config,
        }
    }

    pub async fn run(&mut self) -> Result<(), ProtocolError> {
        loop {
            let message = self.read_message().await?;
            
            debug!("Received command: {}", message.command);
            
            // Handle message
            let responses = self.handle_message(message).await?;
            
            // Send responses
            for response in responses {
                self.send_message(response.0, &response.1).await?;
            }
        }
    }

    async fn read_message(&mut self) -> Result<Message, ProtocolError> {
        // Read command byte
        let cmd_byte = self.stream.read_i8().await?;
        
        let command = if self.key_exchanged {
            self.cipher.decrypt_byte(cmd_byte)
        } else {
            cmd_byte
        };

        // Check if big data command
        let is_big_data = cmd::BIG_DATA_CMDS.contains(&command);
        
        // Read length
        let length = if is_big_data {
            // 3-byte length (little endian trong readMessage2 của client)
            let b1 = self.stream.read_i8().await?;
            let b2 = self.stream.read_i8().await?;
            let b3 = self.stream.read_i8().await?;
            
            if self.key_exchanged {
                let l1 = (self.cipher.decrypt_byte(b1) as u8) as usize + 128;
                let l2 = (self.cipher.decrypt_byte(b2) as u8) as usize + 128;
                let l3 = (self.cipher.decrypt_byte(b3) as u8) as usize + 128;
                (l3 * 256 + l2) * 256 + l1
            } else {
                let l1 = (b1 as u8) as usize + 128;
                let l2 = (b2 as u8) as usize + 128;
                let l3 = (b3 as u8) as usize + 128;
                (l3 * 256 + l2) * 256 + l1
            }
        } else {
            // 2-byte length (big endian)
            let b1 = self.stream.read_i8().await?;
            let b2 = self.stream.read_i8().await?;
            
            if self.key_exchanged {
                let l1 = (self.cipher.decrypt_byte(b1) as u8 & 0xFF) as usize;
                let l2 = (self.cipher.decrypt_byte(b2) as u8 & 0xFF) as usize;
                (l1 << 8) | l2
            } else {
                let l1 = (b1 as u8 & 0xFF) as usize;
                let l2 = (b2 as u8 & 0xFF) as usize;
                (l1 << 8) | l2
            }
        };

        // Read data
        let mut data = vec![0u8; length];
        if length > 0 {
            self.stream.read_exact(&mut data).await?;
            
            if self.key_exchanged {
                data = self.cipher.decrypt_bytes(&data);
            }
        }

        Ok(Message::new(command, data))
    }

    async fn send_message(&mut self, command: i8, data: &[u8]) -> Result<(), ProtocolError> {
        let packet = protocol::build_response(&mut self.cipher, command, data);
        self.stream.write_all(&packet).await?;
        self.stream.flush().await?;
        Ok(())
    }

    async fn handle_message(&mut self, msg: Message) -> Result<Vec<(i8, Vec<u8>)>, ProtocolError> {
        match msg.command {
            cmd::GET_SESSION_ID => {
                // Key exchange
                let key = self.cipher.generate_key(32);
                self.key_exchanged = true;
                
                let mut writer = MessageWriter::new();
                
                // Write key length and key
                writer.write_byte(key.len() as i8);
                writer.write_sbytes(&key);
                
                // IP2 và PORT2 (không dùng cho Freight, gửi dummy)
                writer.write_utf("");
                writer.write_int(0);
                writer.write_byte(0); // isConnect2 = false
                
                info!(" Key exchanged, length: {}", key.len());
                
                Ok(vec![(cmd::GET_SESSION_ID, writer.into_bytes())])
            }

            cmd::FREIGHT_INIT => {
                //   writeByte(zoomLevel)
                //   writeInt(screenWidth)
                //   writeInt(screenHeight)
                let mut reader = MessageReader::new(&msg.data);
                let zoom = reader.read_byte() as u8;
                let screen_w = reader.read_int();
                let screen_h = reader.read_int();

                // Validate zoom level (phải > 0)
                self.zoom_level = if zoom > 0 { zoom } else { self.config.server.default_zoom };

                info!(
                    "📱 FREIGHT_INIT: zoom={}, screen={}x{}",
                    self.zoom_level, screen_w, screen_h
                );

                // ACK response: writeByte(accepted_zoom)
                let mut writer = MessageWriter::new();
                writer.write_byte(self.zoom_level as i8);
                Ok(vec![(cmd::FREIGHT_INIT, writer.into_bytes())])
            }
            
            _ => {
                // Delegate to handler, kèm zoom level của session
                self.handler.handle(msg, self.zoom_level).await
            }
        }
    }
}
