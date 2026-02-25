// Binary packet format: [command: i8][length: u16 BE][data: bytes]

use bytes::{Buf, BufMut, BytesMut};
use thiserror::Error;

// Command constants - Các lệnh liên quan đến data/assets
pub mod cmd {
    // Data request commands
    pub const GET_SESSION_ID: i8 = -27;
    pub const FREIGHT_INIT: i8 = 1;   // Client gửi zoomLevel sau key exchange
    pub const REQUEST_ICON: i8 = -67;
    pub const GET_EFFDATA: i8 = -66;
    pub const GET_IMAGE_SOURCE: i8 = -74;
    pub const GET_IMAGE_SOURCE2: i8 = -111;
    pub const REQUEST_MAPTEMPLATE: i8 = 10;
    pub const REQUEST_NPCTEMPLATE: i8 = 11;
    pub const BACKGROUND_TEMPLATE: i8 = -32;
    pub const ITEM_BACKGROUND: i8 = -31;
    pub const SMALLIMAGE_VERSION: i8 = -77;
    pub const BGITEM_VERSION: i8 = -93;
    pub const TILE_SET: i8 = -82;
    
    // Big data commands (3-byte length)
    pub const BIG_DATA_CMDS: &[i8] = &[-32, -66, 11, -67, -74, -87, 66, 12];
}

#[derive(Error, Debug)]
pub enum ProtocolError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Invalid packet")]
    InvalidPacket,
    
    #[error("Connection closed")]
    ConnectionClosed,
    
    #[error("Encryption not ready")]
    EncryptionNotReady,
}

/// XOR encryption key manager
pub struct Cipher {
    key: Vec<i8>,
    read_pos: usize,
    write_pos: usize,
}

impl Cipher {
    pub fn new() -> Self {
        Self {
            key: Vec::new(),
            read_pos: 0,
            write_pos: 0,
        }
    }

    /// Generate key mà KHÔNG set vào cipher (static method)
    pub fn generate_key_only(length: u8) -> Vec<i8> {
        use std::time::{SystemTime, UNIX_EPOCH};
        
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        
        // Simple LCG random
        let mut rng = seed;
        let mut raw_key = Vec::with_capacity(length as usize);
        
        for _ in 0..length {
            rng = rng.wrapping_mul(1103515245).wrapping_add(12345);
            raw_key.push((rng >> 16) as i8);
        }
        
        raw_key
    }

    /// Set key từ raw key (sẽ XOR chain)
    pub fn set_key_from_raw(&mut self, raw_key: &[i8]) {
        // XOR chain như client làm
        let mut processed = raw_key.to_vec();
        for i in 0..processed.len() - 1 {
            processed[i + 1] ^= processed[i];
        }
        self.key = processed;
        self.read_pos = 0;
        self.write_pos = 0;
    }

    /// Generate random key và set cho cipher (old method - không dùng cho key exchange)
    /// Returns: raw_key_to_send
    pub fn generate_key(&mut self, length: u8) -> Vec<i8> {
        let raw_key = Self::generate_key_only(length);
        self.set_key_from_raw(&raw_key);
        raw_key
    }
    
    pub fn set_key(&mut self, key: Vec<i8>) {
        // XOR chain như client nhận key
        let mut processed = key.clone();
        for i in 0..processed.len() - 1 {
            processed[i + 1] ^= processed[i];
        }
        self.key = processed;
        self.read_pos = 0;
        self.write_pos = 0;
    }

    pub fn is_ready(&self) -> bool {
        !self.key.is_empty()
    }

    pub fn encrypt_byte(&mut self, b: i8) -> i8 {
        if self.key.is_empty() {
            return b;
        }
        let result = (self.key[self.write_pos] as u8 ^ b as u8) as i8;
        self.write_pos = (self.write_pos + 1) % self.key.len();
        result
    }

    pub fn decrypt_byte(&mut self, b: i8) -> i8 {
        if self.key.is_empty() {
            return b;
        }
        let result = (self.key[self.read_pos] as u8 ^ b as u8) as i8;
        self.read_pos = (self.read_pos + 1) % self.key.len();
        result
    }

    pub fn encrypt_bytes(&mut self, data: &[u8]) -> Vec<u8> {
        data.iter()
            .map(|&b| self.encrypt_byte(b as i8) as u8)
            .collect()
    }

    pub fn decrypt_bytes(&mut self, data: &[u8]) -> Vec<u8> {
        data.iter()
            .map(|&b| self.decrypt_byte(b as i8) as u8)
            .collect()
    }
    
    pub fn reset_positions(&mut self) {
        self.read_pos = 0;
        self.write_pos = 0;
    }
}

/// Incoming message from client
#[derive(Debug)]
pub struct Message {
    pub command: i8,
    pub data: Vec<u8>,
}

impl Message {
    pub fn new(command: i8, data: Vec<u8>) -> Self {
        Self { command, data }
    }
    
    pub fn empty(command: i8) -> Self {
        Self { command, data: Vec::new() }
    }
}

/// Message reader helper
pub struct MessageReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> MessageReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn read_byte(&mut self) -> i8 {
        if self.pos >= self.data.len() {
            return 0;
        }
        let b = self.data[self.pos] as i8;
        self.pos += 1;
        b
    }

    pub fn read_ubyte(&mut self) -> u8 {
        if self.pos >= self.data.len() {
            return 0;
        }
        let b = self.data[self.pos];
        self.pos += 1;
        b
    }

    pub fn read_short(&mut self) -> i16 {
        let b1 = self.read_ubyte() as i16;
        let b2 = self.read_ubyte() as i16;
        (b1 << 8) | b2
    }

    pub fn read_int(&mut self) -> i32 {
        let b1 = self.read_ubyte() as i32;
        let b2 = self.read_ubyte() as i32;
        let b3 = self.read_ubyte() as i32;
        let b4 = self.read_ubyte() as i32;
        (b1 << 24) | (b2 << 16) | (b3 << 8) | b4
    }

    pub fn read_utf(&mut self) -> String {
        let len = self.read_short() as usize;
        if self.pos + len > self.data.len() {
            return String::new();
        }
        let s = String::from_utf8_lossy(&self.data[self.pos..self.pos + len]).to_string();
        self.pos += len;
        s
    }
    
    pub fn available(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }
}

/// Message writer helper
pub struct MessageWriter {
    data: BytesMut,
}

impl MessageWriter {
    pub fn new() -> Self {
        Self {
            data: BytesMut::with_capacity(256),
        }
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            data: BytesMut::with_capacity(cap),
        }
    }

    pub fn write_byte(&mut self, b: i8) {
        self.data.put_i8(b);
    }

    pub fn write_ubyte(&mut self, b: u8) {
        self.data.put_u8(b);
    }

    pub fn write_short(&mut self, s: i16) {
        self.data.put_i16(s);
    }

    pub fn write_int(&mut self, i: i32) {
        self.data.put_i32(i);
    }

    pub fn write_long(&mut self, l: i64) {
        self.data.put_i64(l);
    }

    pub fn write_utf(&mut self, s: &str) {
        let bytes = s.as_bytes();
        self.write_short(bytes.len() as i16);
        self.data.extend_from_slice(bytes);
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) {
        self.data.extend_from_slice(bytes);
    }
    
    pub fn write_sbytes(&mut self, bytes: &[i8]) {
        for &b in bytes {
            self.data.put_i8(b);
        }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.data.to_vec()
    }
    
    pub fn len(&self) -> usize {// độ dài data đã write không bao gồm header
        self.data.len()
    }
}

/// Build response message với encryption
pub fn build_response(cipher: &mut Cipher, command: i8, data: &[u8]) -> Vec<u8> {
    let mut response = BytesMut::with_capacity(3 + data.len());
    
    let is_big_data = cmd::BIG_DATA_CMDS.contains(&command);
    
    if cipher.is_ready() {
        // Encrypted response
        response.put_i8(cipher.encrypt_byte(command));
        
        if is_big_data {
            // 3-byte length for big data
            // Client does: decrypt(byte) + 128 to recover each component
            // So we must send: encrypt(component - 128)
            let len = data.len();
            let l1 = ((len & 0xFF) as u8).wrapping_sub(128) as i8;
            let l2 = (((len >> 8) & 0xFF) as u8).wrapping_sub(128) as i8;
            let l3 = (((len >> 16) & 0xFF) as u8).wrapping_sub(128) as i8;
            response.put_i8(cipher.encrypt_byte(l1));
            response.put_i8(cipher.encrypt_byte(l2));
            response.put_i8(cipher.encrypt_byte(l3));
        } else {
            // 2-byte length
            let len = data.len();
            response.put_i8(cipher.encrypt_byte(((len >> 8) & 0xFF) as i8));
            response.put_i8(cipher.encrypt_byte((len & 0xFF) as i8));
        }
        
        // Encrypt data
        for &b in data {
            response.put_i8(cipher.encrypt_byte(b as i8));
        }
    } else {
        // Unencrypted response (only for GET_SESSION_ID)
        // QUAN TRỌNG: Ghi từng byte riêng để đảm bảo đúng format
        response.put_u8(command as u8);  // command byte
        response.put_u8((data.len() >> 8) as u8);  // length high byte
        response.put_u8((data.len() & 0xFF) as u8);  // length low byte
        response.extend_from_slice(data);
    }
    
    response.to_vec()
}
