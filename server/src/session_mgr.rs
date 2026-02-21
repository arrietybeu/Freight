// Session Manager - Quản lý & theo dõi tất cả active sessions

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use dashmap::DashMap;
use tracing::{info, warn};

use crate::metrics::Metrics;

// === Session ID generator ===
static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);

fn next_id() -> u64 {
    NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed)
}

/// Thông tin tracking cho mỗi session
pub struct SessionInfo {
    pub id: u64,
    pub addr: SocketAddr,
    pub zoom_level: u8,
    pub connected_at: Instant,
    pub last_activity: Instant,
    pub bytes_sent: AtomicU64,
    pub bytes_recv: AtomicU64,
    pub request_count: AtomicU64,
    pub requests_ok: AtomicU64,
    pub requests_not_found: AtomicU64,
}

impl SessionInfo {
    pub fn new(addr: SocketAddr) -> Self {
        let now = Instant::now();
        Self {
            id: next_id(),
            addr,
            zoom_level: 0,
            connected_at: now,
            last_activity: now,
            bytes_sent: AtomicU64::new(0),
            bytes_recv: AtomicU64::new(0),
            request_count: AtomicU64::new(0),
            requests_ok: AtomicU64::new(0),
            requests_not_found: AtomicU64::new(0),
        }
    }

    pub fn duration_secs(&self) -> u64 {
        self.connected_at.elapsed().as_secs()
    }

    pub fn idle_secs(&self) -> u64 {
        self.last_activity.elapsed().as_secs()
    }
}

pub struct SessionManager {
    sessions: DashMap<u64, SessionInfo>,
    metrics: Arc<Metrics>,
}

impl SessionManager {
    pub fn new(metrics: Arc<Metrics>) -> Self {
        Self {
            sessions: DashMap::new(),
            metrics,
        }
    }

    /// Đăng ký session mới, trả về session_id
    pub fn register(&self, addr: SocketAddr) -> u64 {
        let info = SessionInfo::new(addr);
        let id = info.id;
        self.sessions.insert(id, info);
        self.metrics.on_connect();
        info!("- Session #{} registered from {}", id, addr);
        id
    }

    /// Huỷ đăng ký khi session disconnect
    pub fn unregister(&self, session_id: u64) {
        if let Some((_, info)) = self.sessions.remove(&session_id) {
            let duration = info.duration_secs();
            let sent = info.bytes_sent.load(Ordering::Relaxed);
            let recv = info.bytes_recv.load(Ordering::Relaxed);
            let reqs = info.request_count.load(Ordering::Relaxed);
            self.metrics.on_disconnect();
            info!(
                "- Session #{} disconnected: {} from {}, duration={}s, ↑{} ↓{}, {} reqs",
                session_id, info.addr, info.addr,
                duration,
                Metrics::fmt_bytes(sent), Metrics::fmt_bytes(recv),
                reqs
            );
        }
    }

    /// Update zoom_level cho session
    pub fn set_zoom(&self, session_id: u64, zoom: u8) {
        if let Some(mut info) = self.sessions.get_mut(&session_id) {
            info.zoom_level = zoom;
        }
    }

    /// Cập nhật last_activity timestamp
    pub fn touch(&self, session_id: u64) {
        if let Some(mut info) = self.sessions.get_mut(&session_id) {
            info.last_activity = Instant::now();
        }
    }

    /// Ghi nhận bytes gửi đi cho session
    pub fn add_bytes_sent(&self, session_id: u64, n: u64) {
        if let Some(info) = self.sessions.get(&session_id) {
            info.bytes_sent.fetch_add(n, Ordering::Relaxed);
        }
        self.metrics.add_bytes_sent(n);
    }

    /// Ghi nhận bytes nhận vào cho session
    pub fn add_bytes_recv(&self, session_id: u64, n: u64) {
        if let Some(info) = self.sessions.get(&session_id) {
            info.bytes_recv.fetch_add(n, Ordering::Relaxed);
        }
        self.metrics.add_bytes_recv(n);
    }

    /// Ghi nhận 1 request
    pub fn on_request(&self, session_id: u64) {
        if let Some(info) = self.sessions.get(&session_id) {
            info.request_count.fetch_add(1, Ordering::Relaxed);
        }
        self.metrics.on_request();
    }

    pub fn on_request_ok(&self, session_id: u64) {
        if let Some(info) = self.sessions.get(&session_id) {
            info.requests_ok.fetch_add(1, Ordering::Relaxed);
        }
        self.metrics.on_request_ok();
    }

    pub fn on_request_not_found(&self, session_id: u64) {
        if let Some(info) = self.sessions.get(&session_id) {
            info.requests_not_found.fetch_add(1, Ordering::Relaxed);
        }
        self.metrics.on_request_not_found();
    }

    /// Số sessions hiện tại
    pub fn active_count(&self) -> usize {
        self.sessions.len()
    }

    /// Top N sessions theo bytes_sent (nhiều nhất → ít nhất)
    pub fn top_by_bandwidth(&self, n: usize) -> Vec<(u64, SocketAddr, u64, u64, u64)> {
        let mut entries: Vec<_> = self.sessions.iter().map(|entry| {
            let info = entry.value();
            (
                info.id,
                info.addr,
                info.bytes_sent.load(Ordering::Relaxed),
                info.bytes_recv.load(Ordering::Relaxed),
                info.request_count.load(Ordering::Relaxed),
            )
        }).collect();
        
        // Sort by bytes_sent descending
        entries.sort_by(|a, b| b.2.cmp(&a.2));
        entries.truncate(n);
        entries
    }

    /// Tìm và kick các sessions idle quá lâu
    /// Trả về danh sách session_id đã bị kick
    pub fn kick_idle(&self, idle_timeout_secs: u64) -> Vec<u64> {
        let mut kicked = Vec::new();
        
        // Collect idle sessions
        let idle_ids: Vec<u64> = self.sessions.iter()
            .filter(|entry| entry.value().idle_secs() > idle_timeout_secs)
            .map(|entry| entry.value().id)
            .collect();
        
        for id in idle_ids {
            warn!("⏰ Kicking idle session #{} (idle > {}s)", id, idle_timeout_secs);
            self.unregister(id);
            kicked.push(id);
        }
        
        kicked
    }

    /// Log top sessions theo bandwidth
    pub fn log_top_sessions(&self, n: usize) {
        let top = self.top_by_bandwidth(n);
        if top.is_empty() {
            return;
        }

        let mut lines = String::from("\n┌── TOP SESSIONS BY BANDWIDTH ──┐\n");
        for (i, (id, addr, sent, _recv, reqs)) in top.iter().enumerate() {
            lines.push_str(&format!(
                "│ #{}: Session {} ({}) — ↑{}, {} reqs\n",
                i + 1, id, addr, Metrics::fmt_bytes(*sent), reqs
            ));
        }
        lines.push_str("└───────────────────────────────┘");
        info!("{}", lines);
    }
}
