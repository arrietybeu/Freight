
use std::sync::atomic::{AtomicU64, AtomicI64, Ordering};
use std::time::Instant;

pub struct Metrics {
    pub bytes_sent: AtomicU64,
    pub bytes_recv: AtomicU64,

    pub total_connections: AtomicU64,
    /// Active connections hiện tại
    pub active_connections: AtomicI64,
    /// Peak concurrent connections
    pub peak_connections: AtomicI64,

    // === Requests ===
    /// Tổng requests đã xử lý
    pub total_requests: AtomicU64,
    /// Requests thành công (có data trả về)
    pub requests_ok: AtomicU64,
    /// Requests thất bại (file not found)
    pub requests_not_found: AtomicU64,

    // === Per-command counters ===
    pub cmd_icon: AtomicU64,
    pub cmd_effdata: AtomicU64,
    pub cmd_map: AtomicU64,
    pub cmd_npc: AtomicU64,
    pub cmd_image_source: AtomicU64,
    pub cmd_background: AtomicU64,
    pub cmd_tileset: AtomicU64,

    // === Cache ===
    pub cache_hits: AtomicU64,
    pub cache_misses: AtomicU64,
    /// Bytes đang chiếm trong cache
    pub cache_bytes: AtomicI64,
    /// Số entries trong cache
    pub cache_entries: AtomicI64,

    // === Rate limiting ===
    /// Số lần bị rate limited
    pub rate_limited: AtomicU64,

    // === Uptime ===
    pub start_time: Instant,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            bytes_sent: AtomicU64::new(0),
            bytes_recv: AtomicU64::new(0),
            total_connections: AtomicU64::new(0),
            active_connections: AtomicI64::new(0),
            peak_connections: AtomicI64::new(0),
            total_requests: AtomicU64::new(0),
            requests_ok: AtomicU64::new(0),
            requests_not_found: AtomicU64::new(0),
            cmd_icon: AtomicU64::new(0),
            cmd_effdata: AtomicU64::new(0),
            cmd_map: AtomicU64::new(0),
            cmd_npc: AtomicU64::new(0),
            cmd_image_source: AtomicU64::new(0),
            cmd_background: AtomicU64::new(0),
            cmd_tileset: AtomicU64::new(0),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
            cache_bytes: AtomicI64::new(0),
            cache_entries: AtomicI64::new(0),
            rate_limited: AtomicU64::new(0),
            start_time: Instant::now(),
        }
    }

    // === Convenience methods ===

    pub fn add_bytes_sent(&self, n: u64) {
        self.bytes_sent.fetch_add(n, Ordering::Relaxed);
    }

    pub fn add_bytes_recv(&self, n: u64) {
        self.bytes_recv.fetch_add(n, Ordering::Relaxed);
    }

    pub fn on_connect(&self) {
        self.total_connections.fetch_add(1, Ordering::Relaxed);
        let current = self.active_connections.fetch_add(1, Ordering::Relaxed) + 1;
        // Update peak
        let mut peak = self.peak_connections.load(Ordering::Relaxed);
        while current > peak {
            match self.peak_connections.compare_exchange_weak(
                peak, current, Ordering::Relaxed, Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => peak = actual,
            }
        }
    }

    pub fn on_disconnect(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn on_request(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
    }

    pub fn on_request_ok(&self) {
        self.requests_ok.fetch_add(1, Ordering::Relaxed);
    }

    pub fn on_request_not_found(&self) {
        self.requests_not_found.fetch_add(1, Ordering::Relaxed);
    }

    pub fn on_cache_hit(&self) {
        self.cache_hits.fetch_add(1, Ordering::Relaxed);
    }

    pub fn on_cache_miss(&self) {
        self.cache_misses.fetch_add(1, Ordering::Relaxed);
    }

    pub fn on_rate_limited(&self) {
        self.rate_limited.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_cmd(&self, command: i8) {
        use crate::protocol::cmd;
        match command {
            cmd::REQUEST_ICON => { self.cmd_icon.fetch_add(1, Ordering::Relaxed); }
            cmd::GET_EFFDATA => { self.cmd_effdata.fetch_add(1, Ordering::Relaxed); }
            cmd::REQUEST_MAPTEMPLATE => { self.cmd_map.fetch_add(1, Ordering::Relaxed); }
            cmd::REQUEST_NPCTEMPLATE => { self.cmd_npc.fetch_add(1, Ordering::Relaxed); }
            cmd::GET_IMAGE_SOURCE | cmd::GET_IMAGE_SOURCE2 => { self.cmd_image_source.fetch_add(1, Ordering::Relaxed); }
            cmd::BACKGROUND_TEMPLATE => { self.cmd_background.fetch_add(1, Ordering::Relaxed); }
            cmd::TILE_SET => { self.cmd_tileset.fetch_add(1, Ordering::Relaxed); }
            _ => {}
        }
    }

    /// Uptime in seconds
    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Format bytes thành human readable
    pub fn fmt_bytes(bytes: u64) -> String {
        if bytes < 1024 {
            format!("{} B", bytes)
        } else if bytes < 1024 * 1024 {
            format!("{:.1} KB", bytes as f64 / 1024.0)
        } else if bytes < 1024 * 1024 * 1024 {
            format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
        } else {
            format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
        }
    }

    /// Format uptime thành human readable
    pub fn fmt_uptime(secs: u64) -> String {
        let days = secs / 86400;
        let hours = (secs % 86400) / 3600;
        let mins = (secs % 3600) / 60;
        let s = secs % 60;
        if days > 0 {
            format!("{}d {}h {}m {}s", days, hours, mins, s)
        } else if hours > 0 {
            format!("{}h {}m {}s", hours, mins, s)
        } else if mins > 0 {
            format!("{}m {}s", mins, s)
        } else {
            format!("{}s", s)
        }
    }

    /// Cache hit rate (0.0 - 100.0)
    pub fn cache_hit_rate(&self) -> f64 {
        let hits = self.cache_hits.load(Ordering::Relaxed) as f64;
        let misses = self.cache_misses.load(Ordering::Relaxed) as f64;
        let total = hits + misses;
        if total == 0.0 { 0.0 } else { (hits / total) * 100.0 }
    }

    /// In toàn bộ stats ra log
    pub fn log_stats(&self) {
        let uptime = self.uptime_secs();
        let sent = self.bytes_sent.load(Ordering::Relaxed);
        let recv = self.bytes_recv.load(Ordering::Relaxed);
        let active = self.active_connections.load(Ordering::Relaxed);
        let peak = self.peak_connections.load(Ordering::Relaxed);
        let total_conn = self.total_connections.load(Ordering::Relaxed);
        let total_req = self.total_requests.load(Ordering::Relaxed);
        let req_ok = self.requests_ok.load(Ordering::Relaxed);
        let req_nf = self.requests_not_found.load(Ordering::Relaxed);
        let rate_lim = self.rate_limited.load(Ordering::Relaxed);

        // Bandwidth per second
        let bps_sent = if uptime > 0 { sent / uptime } else { 0 };
        let bps_recv = if uptime > 0 { recv / uptime } else { 0 };

        tracing::info!(
            "\n╔══════════════════ FREIGHT STATS ══════════════════╗\n\
             ║ Uptime:       {:<37}║\n\
             ║ Connections:  {} active / {} peak / {} total       \n\
             ║───────────────────────────────────────────────────║\n\
             ║ Bandwidth:    ↑ {} (avg {}/s)                     \n\
             ║               ↓ {} (avg {}/s)                     \n\
             ║───────────────────────────────────────────────────║\n\
             ║ Requests:     {} total, {} ok, {} not found       \n\
             ║ Rate Limited: {}                                  \n\
             ║───────────────────────────────────────────────────║\n\
             ║ Commands:     icon={} eff={} map={} npc={}        \n\
             ║               img={} bg={} tile={}                \n\
             ║───────────────────────────────────────────────────║\n\
             ║ Cache:        {} entries, {}                      \n\
             ║               Hit rate: {:.1}%                    \n\
             ╚══════════════════════════════════════════════════╝",
            Self::fmt_uptime(uptime),
            active, peak, total_conn,
            Self::fmt_bytes(sent), Self::fmt_bytes(bps_sent),
            Self::fmt_bytes(recv), Self::fmt_bytes(bps_recv),
            total_req, req_ok, req_nf,
            rate_lim,
            self.cmd_icon.load(Ordering::Relaxed),
            self.cmd_effdata.load(Ordering::Relaxed),
            self.cmd_map.load(Ordering::Relaxed),
            self.cmd_npc.load(Ordering::Relaxed),
            self.cmd_image_source.load(Ordering::Relaxed),
            self.cmd_background.load(Ordering::Relaxed),
            self.cmd_tileset.load(Ordering::Relaxed),
            self.cache_entries.load(Ordering::Relaxed),
            Self::fmt_bytes(self.cache_bytes.load(Ordering::Relaxed) as u64),
            self.cache_hit_rate(),
        );
    }
}
