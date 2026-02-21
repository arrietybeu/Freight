
use dashmap::DashMap;
use tracing::{debug, warn};
use tokio::fs;

pub struct DataStore {
    cache: DashMap<String, Vec<u8>>,
}

impl DataStore {
    pub fn new() -> Self {
        Self {
            cache: DashMap::new(),
        }
    }

    /// Đọc file từ disk (có cache).
    /// `path` đã được resolve bởi PathsConfig (vd: "./data/x2/icon/123.png")
    pub async fn load_file(&self, path: &str) -> Option<Vec<u8>> {
        // Kiểm tra cache trước
        if let Some(data) = self.cache.get(path) {
            return Some(data.clone());
        }

        // Đọc file
        match fs::read(path).await {
            Ok(data) => {
                debug!("- Loaded {} ({} bytes)", path, data.len());
                self.cache.insert(path.to_string(), data.clone());
                Some(data)
            }
            Err(e) => {
                warn!("! File not found: {} ({})", path, e);
                None
            }
        }
    }

    /// Đọc file version (4 bytes i32 big-endian), trả về 0 nếu không tìm thấy
    pub async fn load_version_file(&self, path: &str) -> i32 {
        if let Some(data) = self.load_file(path).await {
            if data.len() >= 4 {
                return i32::from_be_bytes([data[0], data[1], data[2], data[3]]);
            }
            // Nếu file chỉ chứa text number
            if let Ok(s) = std::str::from_utf8(&data) {
                if let Ok(v) = s.trim().parse::<i32>() {
                    return v;
                }
            }
        }
        0
    }

    /// Xoá cache cho một path cụ thể
    #[allow(dead_code)]
    pub fn invalidate(&self, path: &str) {
        self.cache.remove(path);
    }

    /// Xoá toàn bộ cache
    #[allow(dead_code)]
    pub fn clear_cache(&self) {
        self.cache.clear();
    }

    /// Số entry trong cache
    #[allow(dead_code)]
    pub fn cache_size(&self) -> usize {
        self.cache.len()
    }
}
