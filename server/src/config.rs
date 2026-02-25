// Config - YAML configuration cho Freight Data Server

use serde::Deserialize;
use std::path::Path;
use tracing::{info, warn};

#[derive(Debug, Deserialize, Clone)]
pub struct FreightConfig {
    pub server: ServerConfig,
    pub paths: PathsConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    /// Bind address
    #[serde(default = "default_host")]
    pub host: String,

    /// Listen port
    #[serde(default = "default_port")]
    pub port: u16,

    /// Max concurrent connections
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,

    /// Default zoom level nếu client chưa gửi
    #[serde(default = "default_zoom")]
    pub default_zoom: u8,

    /// Idle timeout (seconds) - kick sessions không hoạt động
    #[serde(default)]
    pub idle_timeout_secs: Option<u64>,

    /// Max cache size in MB (0 = unlimited)
    #[serde(default)]
    pub max_cache_mb: Option<u64>,

    /// Interval for periodic stats logging (seconds)
    #[serde(default)]
    pub stats_interval_secs: Option<u64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PathsConfig {
    /// Root directory chứa asset data
    #[serde(default = "default_base_dir")]
    pub base_dir: String,

    /// Pattern cho icon: {base}, {zoom}, {id} được thay thế runtime
    #[serde(default = "default_icon_path")]
    pub icon: String,

    /// Pattern cho effect data
    #[serde(default = "default_effect_path")]
    pub effect: String,

    /// Pattern cho map template
    #[serde(default = "default_map_path")]
    pub map: String,

    /// Pattern cho NPC template
    #[serde(default = "default_npc_path")]
    pub npc: String,

    /// Pattern cho background template
    #[serde(default = "default_background_path")]
    pub background: String,

    /// Pattern cho tile set
    #[serde(default = "default_tileset_path")]
    pub tileset: String,

    /// Pattern cho image source (không có {id})
    #[serde(default = "default_image_source_path")]
    pub image_source: String,

    /// Pattern cho image source 2
    #[serde(default = "default_image_source2_path")]
    pub image_source2: String,

    /// Pattern cho small image version file
    #[serde(default = "default_smallimage_path")]
    pub smallimage_version: String,

    /// Pattern cho bgitem version file
    #[serde(default = "default_bgitem_path")]
    pub bgitem_version: String,
}

// ===== Default values =====

fn default_host() -> String { "0.0.0.0".to_string() }
fn default_port() -> u16 { 14450 }
fn default_max_connections() -> usize { 10000 }
fn default_zoom() -> u8 { 1 }
fn default_base_dir() -> String { "./data".to_string() }

fn default_icon_path() -> String {
    "{base}/x{zoom}/icon/{id}.png".to_string()
}
fn default_effect_path() -> String {
    "{base}/x{zoom}/effectdata/{id}.dat".to_string()
}
fn default_map_path() -> String {
    "{base}/x{zoom}/map/{id}.dat".to_string()
}
fn default_npc_path() -> String {
    "{base}/x{zoom}/npc/{id}.dat".to_string()
}
fn default_background_path() -> String {
    "{base}/x{zoom}/bg/{id}.png".to_string()
}
fn default_tileset_path() -> String {
    "{base}/x{zoom}/tileset/{id}.dat".to_string()
}
fn default_image_source_path() -> String {
    "{base}/x{zoom}/imagesource.dat".to_string()
}
fn default_image_source2_path() -> String {
    "{base}/x{zoom}/imagesource2.dat".to_string()
}
fn default_smallimage_path() -> String {
    "{base}/x{zoom}/smallimage_version.dat".to_string()
}
fn default_bgitem_path() -> String {
    "{base}/x{zoom}/bgitem_version.dat".to_string()
}

impl FreightConfig {
    pub fn load(path: &str) -> Self {
        let config_path = Path::new(path);

        if config_path.exists() {
            match std::fs::read_to_string(config_path) {
                Ok(content) => {
                    match serde_yaml::from_str::<FreightConfig>(&content) {
                        Ok(config) => {
                            info!("- Config loaded from {}", path);
                            return config;
                        }
                        Err(e) => {
                            warn!("! Failed to parse config {}: {}", path, e);
                        }
                    }
                }
                Err(e) => {
                    warn!("! Failed to read config {}: {}", path, e);
                }
            }
        } else {
            info!("- Config file not found at {}, using defaults", path);
        }

        Self::default()
    }

    /// Bind address dạng "host:port"
    pub fn bind_addr(&self) -> String {
        format!("{}:{}", self.server.host, self.server.port)
    }
}

impl Default for FreightConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: default_host(),
                port: default_port(),
                max_connections: default_max_connections(),
                default_zoom: default_zoom(),
                idle_timeout_secs: Some(300),
                max_cache_mb: Some(512),
                stats_interval_secs: Some(60),
            },
            paths: PathsConfig {
                base_dir: default_base_dir(),
                icon: default_icon_path(),
                effect: default_effect_path(),
                map: default_map_path(),
                npc: default_npc_path(),
                background: default_background_path(),
                tileset: default_tileset_path(),
                image_source: default_image_source_path(),
                image_source2: default_image_source2_path(),
                smallimage_version: default_smallimage_path(),
                bgitem_version: default_bgitem_path(),
            },
        }
    }
}

impl PathsConfig {
    pub fn resolve(&self, pattern: &str, zoom: u8, id: Option<i32>) -> String {
        let mut path = pattern
            .replace("{base}", &self.base_dir)
            .replace("{zoom}", &zoom.to_string());

        if let Some(id_val) = id {
            path = path.replace("{id}", &id_val.to_string());
        }

        path
    }

    pub fn icon_path(&self, zoom: u8, id: i32) -> String {
        self.resolve(&self.icon, zoom, Some(id))
    }

    pub fn effect_path(&self, zoom: u8, id: i32) -> String {
        self.resolve(&self.effect, zoom, Some(id))
    }

    pub fn map_path(&self, zoom: u8, id: i32) -> String {
        self.resolve(&self.map, zoom, Some(id))
    }

    pub fn npc_path(&self, zoom: u8, id: i32) -> String {
        self.resolve(&self.npc, zoom, Some(id))
    }

    pub fn background_path(&self, zoom: u8, id: i32) -> String {
        self.resolve(&self.background, zoom, Some(id))
    }

    pub fn tileset_path(&self, zoom: u8, id: i32) -> String {
        self.resolve(&self.tileset, zoom, Some(id))
    }

    pub fn image_source_path(&self, zoom: u8) -> String {
        self.resolve(&self.image_source, zoom, None)
    }

    pub fn image_source2_path(&self, zoom: u8) -> String {
        self.resolve(&self.image_source2, zoom, None)
    }

    pub fn smallimage_version_path(&self, zoom: u8) -> String {
        self.resolve(&self.smallimage_version, zoom, None)
    }

    pub fn bgitem_version_path(&self, zoom: u8) -> String {
        self.resolve(&self.bgitem_version, zoom, None)
    }
}
