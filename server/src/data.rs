
use std::sync::Arc;
use std::sync::atomic::Ordering;

use dashmap::DashMap;
use tracing::{debug, warn, info};
use tokio::fs;

use crate::metrics::Metrics;

/// Entry trong cache, kèm metadata cho LRU eviction
struct CacheEntry {
    data: Vec<u8>,
    /// Timestamp truy cập gần nhất (monotonic, từ Instant)
    last_access: std::time::Instant,
}

pub struct DataStore {
    cache: DashMap<String, CacheEntry>,
    metrics: Arc<Metrics>,
    /// Max cache size in bytes (0 = unlimited)
    max_cache_bytes: u64,
}

impl DataStore {
    pub fn new(metrics: Arc<Metrics>, max_cache_bytes: u64) -> Self {
        Self {
            cache: DashMap::new(),
            metrics,
            max_cache_bytes,
        }
    }

    /// Đọc file từ disk (có cache).
    /// `path` đã được resolve bởi PathsConfig (vd: "./data/x2/icon/123.png")
    pub async fn load_file(&self, path: &str) -> Option<Vec<u8>> {
        // Kiểm tra cache trước
        if let Some(mut entry) = self.cache.get_mut(path) {
            entry.last_access = std::time::Instant::now();
            self.metrics.on_cache_hit();
            return Some(entry.data.clone());
        }

        self.metrics.on_cache_miss();

        // Đọc file
        match fs::read(path).await {
            Ok(data) => {
                debug!("📂 Loaded {} ({} bytes)", path, data.len());
                let data_len = data.len() as i64;
                
                // Evict nếu cần trước khi insert
                if self.max_cache_bytes > 0 {
                    self.evict_if_needed(data.len() as u64).await;
                }
                
                self.cache.insert(path.to_string(), CacheEntry {
                    data: data.clone(),
                    last_access: std::time::Instant::now(),
                });
                
                // Update metrics
                self.metrics.cache_bytes.fetch_add(data_len, Ordering::Relaxed);
                self.metrics.cache_entries.fetch_add(1, Ordering::Relaxed);
                
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

    /// LRU Eviction: xoá entries cũ nhất cho đến khi đủ chỗ
    async fn evict_if_needed(&self, incoming_bytes: u64) {
        let current = self.metrics.cache_bytes.load(Ordering::Relaxed) as u64;
        
        if current + incoming_bytes <= self.max_cache_bytes {
            return; // Đủ chỗ
        }

        let target = current + incoming_bytes - self.max_cache_bytes;
        let mut freed: u64 = 0;
        let mut evicted_count: i64 = 0;
        
        // Collect all entries sorted by last_access (oldest first)
        let mut entries: Vec<(String, std::time::Instant, u64)> = self.cache.iter()
            .map(|e| (e.key().clone(), e.value().last_access, e.value().data.len() as u64))
            .collect();
        entries.sort_by(|a, b| a.1.cmp(&b.1)); // oldest first
        
        for (key, _access, size) in entries {
            if freed >= target {
                break;
            }
            if self.cache.remove(&key).is_some() {
                freed += size;
                evicted_count += 1;
            }
        }

        if evicted_count > 0 {
            info!("🗑️ Cache evicted {} entries, freed {}", evicted_count, Metrics::fmt_bytes(freed));
            self.metrics.cache_bytes.fetch_sub(freed as i64, Ordering::Relaxed);
            self.metrics.cache_entries.fetch_sub(evicted_count, Ordering::Relaxed);
        }
    }

    /// Xoá cache cho một path cụ thể
    #[allow(dead_code)]
    pub fn invalidate(&self, path: &str) {
        if let Some((_, entry)) = self.cache.remove(path) {
            self.metrics.cache_bytes.fetch_sub(entry.data.len() as i64, Ordering::Relaxed);
            self.metrics.cache_entries.fetch_sub(1, Ordering::Relaxed);
        }
    }

    /// Xoá toàn bộ cache
    #[allow(dead_code)]
    pub fn clear_cache(&self) {
        self.cache.clear();
        self.metrics.cache_bytes.store(0, Ordering::Relaxed);
        self.metrics.cache_entries.store(0, Ordering::Relaxed);
    }

    /// Số entry trong cache
    #[allow(dead_code)]
    pub fn cache_size(&self) -> usize {
        self.cache.len()
    }
}
