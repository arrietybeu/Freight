
use std::sync::Arc;
use tracing::{info, warn};

use crate::config::FreightConfig;
use crate::data::DataStore;
use crate::metrics::Metrics;
use crate::session_mgr::SessionManager;
use crate::protocol::{Message, MessageReader, MessageWriter, ProtocolError, cmd};

pub struct Handler {
    data_store: Arc<DataStore>,
    config: Arc<FreightConfig>,
    session_mgr: Arc<SessionManager>,
    metrics: Arc<Metrics>,
}

impl Handler {
    pub fn new(
        data_store: Arc<DataStore>,
        config: Arc<FreightConfig>,
        session_mgr: Arc<SessionManager>,
        metrics: Arc<Metrics>,
    ) -> Self {
        Self { data_store, config, session_mgr, metrics }
    }

    pub async fn handle(&self, msg: Message, zoom: u8, session_id: u64) -> Result<Vec<(i8, Vec<u8>)>, ProtocolError> {
        let mut reader = MessageReader::new(&msg.data);
        let paths = &self.config.paths;
        
        match msg.command {
            cmd::REQUEST_ICON => {
                let id = reader.read_int();
                info!("- [#{}] REQUEST_ICON id={} zoom={}", session_id, id, zoom);
                
                let file_path = paths.icon_path(zoom, id);
                if let Some(data) = self.data_store.load_file(&file_path).await {
                    self.session_mgr.on_request_ok(session_id);
                    Ok(vec![(cmd::REQUEST_ICON, data)])
                } else {
                    self.session_mgr.on_request_not_found(session_id);
                    warn!("Icon {} not found at {}", id, file_path);
                    let mut w = MessageWriter::new();
                    w.write_int(id);
                    w.write_byte(0);
                    Ok(vec![(cmd::REQUEST_ICON, w.into_bytes())])
                }
            }
            
            cmd::GET_EFFDATA => {
                let id = reader.read_short();
                info!("- [#{}] GET_EFFDATA id={} zoom={}", session_id, id, zoom);
                
                let file_path = paths.effect_path(zoom, id as i32);
                if let Some(data) = self.data_store.load_file(&file_path).await {
                    self.session_mgr.on_request_ok(session_id);
                    Ok(vec![(cmd::GET_EFFDATA, data)])
                } else {
                    self.session_mgr.on_request_not_found(session_id);
                    warn!("EffData {} not found at {}", id, file_path);
                    let mut w = MessageWriter::new();
                    w.write_short(id);
                    w.write_byte(0);
                    Ok(vec![(cmd::GET_EFFDATA, w.into_bytes())])
                }
            }
            
            cmd::REQUEST_MAPTEMPLATE => {
                let map_id = reader.read_byte();
                info!("- [#{}] REQUEST_MAPTEMPLATE id={} zoom={}", session_id, map_id, zoom);
                
                let file_path = paths.map_path(zoom, map_id as i32);
                if let Some(data) = self.data_store.load_file(&file_path).await {
                    self.session_mgr.on_request_ok(session_id);
                    Ok(vec![(cmd::REQUEST_MAPTEMPLATE, data)])
                } else {
                    self.session_mgr.on_request_not_found(session_id);
                    warn!("MapTemplate {} not found at {}", map_id, file_path);
                    let mut w = MessageWriter::new();
                    w.write_byte(map_id);
                    w.write_byte(0);
                    Ok(vec![(cmd::REQUEST_MAPTEMPLATE, w.into_bytes())])
                }
            }
            
            cmd::REQUEST_NPCTEMPLATE => {
                let npc_id = reader.read_byte();
                info!("- [#{}] REQUEST_NPCTEMPLATE id={} zoom={}", session_id, npc_id, zoom);
                
                let file_path = paths.npc_path(zoom, npc_id as i32);
                if let Some(data) = self.data_store.load_file(&file_path).await {
                    self.session_mgr.on_request_ok(session_id);
                    Ok(vec![(cmd::REQUEST_NPCTEMPLATE, data)])
                } else {
                    self.session_mgr.on_request_not_found(session_id);
                    warn!("NpcTemplate {} not found at {}", npc_id, file_path);
                    let mut w = MessageWriter::new();
                    w.write_byte(npc_id);
                    w.write_byte(0);
                    Ok(vec![(cmd::REQUEST_NPCTEMPLATE, w.into_bytes())])
                }
            }
            
            cmd::GET_IMAGE_SOURCE => {
                info!("- [#{}] GET_IMAGE_SOURCE zoom={}", session_id, zoom);
                
                let file_path = paths.image_source_path(zoom);
                if let Some(data) = self.data_store.load_file(&file_path).await {
                    self.session_mgr.on_request_ok(session_id);
                    Ok(vec![(cmd::GET_IMAGE_SOURCE, data)])
                } else {
                    self.session_mgr.on_request_not_found(session_id);
                    Ok(vec![(cmd::GET_IMAGE_SOURCE, vec![])])
                }
            }
            
            cmd::GET_IMAGE_SOURCE2 => {
                info!("- [#{}] GET_IMAGE_SOURCE2 zoom={}", session_id, zoom);
                
                let file_path = paths.image_source2_path(zoom);
                if let Some(data) = self.data_store.load_file(&file_path).await {
                    self.session_mgr.on_request_ok(session_id);
                    Ok(vec![(cmd::GET_IMAGE_SOURCE2, data)])
                } else {
                    self.session_mgr.on_request_not_found(session_id);
                    Ok(vec![(cmd::GET_IMAGE_SOURCE2, vec![])])
                }
            }
            
            cmd::BACKGROUND_TEMPLATE => {
                let id = reader.read_byte();
                info!("- [#{}] BACKGROUND_TEMPLATE id={} zoom={}", session_id, id, zoom);
                
                let file_path = paths.background_path(zoom, id as i32);
                if let Some(data) = self.data_store.load_file(&file_path).await {
                    self.session_mgr.on_request_ok(session_id);
                    Ok(vec![(cmd::BACKGROUND_TEMPLATE, data)])
                } else {
                    self.session_mgr.on_request_not_found(session_id);
                    Ok(vec![(cmd::BACKGROUND_TEMPLATE, vec![])])
                }
            }
            
            cmd::TILE_SET => {
                let id = reader.read_byte();
                info!("- [#{}] TILE_SET id={} zoom={}", session_id, id, zoom);
                
                let file_path = paths.tileset_path(zoom, id as i32);
                if let Some(data) = self.data_store.load_file(&file_path).await {
                    self.session_mgr.on_request_ok(session_id);
                    Ok(vec![(cmd::TILE_SET, data)])
                } else {
                    self.session_mgr.on_request_not_found(session_id);
                    Ok(vec![(cmd::TILE_SET, vec![])])
                }
            }
            
            cmd::SMALLIMAGE_VERSION => {
                info!("- [#{}] SMALLIMAGE_VERSION zoom={}", session_id, zoom);
                self.session_mgr.on_request_ok(session_id);
                
                let file_path = paths.smallimage_version_path(zoom);
                let version = self.data_store.load_version_file(&file_path).await;
                let mut w = MessageWriter::new();
                w.write_int(version);
                Ok(vec![(cmd::SMALLIMAGE_VERSION, w.into_bytes())])
            }
            
            cmd::BGITEM_VERSION => {
                info!("- [#{}] BGITEM_VERSION zoom={}", session_id, zoom);
                self.session_mgr.on_request_ok(session_id);
                
                let file_path = paths.bgitem_version_path(zoom);
                let version = self.data_store.load_version_file(&file_path).await;
                let mut w = MessageWriter::new();
                w.write_int(version);
                Ok(vec![(cmd::BGITEM_VERSION, w.into_bytes())])
            }
            
            _ => {
                warn!("!️ [#{}] Unknown command: {}", session_id, msg.command);
                Ok(vec![])
            }
        }
    }
}
