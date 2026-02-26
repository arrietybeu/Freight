
use std::sync::Arc;
use std::sync::atomic::Ordering;

use dashmap::DashMap;
use tracing::{debug, warn, info};
use tokio::fs;

use crate::config::PathsConfig;
use crate::metrics::Metrics;

/// Entry trong cache, kèm metadata cho LRU eviction
struct CacheEntry {
    data: Vec<u8>,
    last_access: std::time::Instant,
}

/// Thông tin một img_by_name: frame number và đường dẫn file
#[derive(Debug, Clone)]
pub struct ImageByNameEntry {
    pub frame: i8,
    pub path: String,
}

pub struct DataStore {
    cache: DashMap<String, CacheEntry>,
    /// Index cho img_by_name: key = "{zoom}:{name}" (ví dụ "2:mount_0_0"), value = ImageByNameEntry
    img_by_name_index: DashMap<String, ImageByNameEntry>,
    metrics: Arc<Metrics>,
    /// Max cache size in bytes (0 = unlimited)
    max_cache_bytes: u64,
}

impl DataStore {
    pub fn new(metrics: Arc<Metrics>, max_cache_bytes: u64) -> Self {
        Self {
            cache: DashMap::new(),
            img_by_name_index: DashMap::new(),
            metrics,
            max_cache_bytes,
        }
    }

    /// Đọc file từ disk (có cache).
    /// `path` đã được resolve bởi PathsConfig (vd: "./data/x2/icon/123.png")
    pub async fn load_file(&self, path: &str) -> Option<Vec<u8>> {
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

    pub async fn scan_img_by_name(&self, paths: &PathsConfig) {
        let mut total = 0;
        for zoom in 1..=4u8 {
            let dir = paths.img_by_name_dir(zoom);
            let dir_path = std::path::Path::new(&dir);
            if !dir_path.exists() {
                debug!("img_by_name dir not found for zoom {}: {}", zoom, dir);
                continue;
            }

            let mut read_dir = match fs::read_dir(&dir).await {
                Ok(rd) => rd,
                Err(e) => {
                    warn!("Failed to read img_by_name dir {}: {}", dir, e);
                    continue;
                }
            };

            while let Ok(Some(entry)) = read_dir.next_entry().await {
                let file_name = entry.file_name();
                let file_name_str = file_name.to_string_lossy();

                if !file_name_str.ends_with(".png") {
                    continue;
                }

                let stem = &file_name_str[..file_name_str.len() - 4];

                if let Some(last_underscore) = stem.rfind('_') {
                    let name = &stem[..last_underscore];
                    let frame_str = &stem[last_underscore + 1..];

                    if let Ok(frame) = frame_str.parse::<i8>() {
                        let full_path = entry.path().to_string_lossy().to_string();
                        let key = format!("{}:{}", zoom, name);
                        self.img_by_name_index.insert(key, ImageByNameEntry {
                            frame,
                            path: full_path,
                        });
                        total += 1;
                    } else {
                        warn!("Cannot parse frame from img_by_name file: {}", file_name_str);
                    }
                } else {
                    warn!("Invalid img_by_name filename format: {}", file_name_str);
                }
            }
        }
        info!("- Scanned {} img_by_name entries across all zoom levels", total);
    }

    pub fn lookup_img_by_name(&self, zoom: u8, name: &str) -> Option<ImageByNameEntry> {
        let key = format!("{}:{}", zoom, name);
        self.img_by_name_index.get(&key).map(|e| e.value().clone())
    }
}
