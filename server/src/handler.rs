
use std::sync::Arc;
use tracing::{info, warn};

use crate::config::FreightConfig;
use crate::data::DataStore;
use crate::protocol::{Message, MessageReader, MessageWriter, ProtocolError, cmd};

pub struct Handler {
    data_store: Arc<DataStore>,
    config: Arc<FreightConfig>,
}

impl Handler {
    pub fn new(data_store: Arc<DataStore>, config: Arc<FreightConfig>) -> Self {
        Self { data_store, config }
    }

    pub async fn handle(&self, msg: Message, zoom: u8) -> Result<Vec<(i8, Vec<u8>)>, ProtocolError> {
        let mut reader = MessageReader::new(&msg.data);
        let paths = &self.config.paths;
        
        match msg.command {
            cmd::REQUEST_ICON => {
                let id = reader.read_int();
                info!("- REQUEST_ICON id={} zoom={}", id, zoom);
                
                let file_path = paths.icon_path(zoom, id);
                if let Some(data) = self.data_store.load_file(&file_path).await {
                    Ok(vec![(cmd::REQUEST_ICON, data)])
                } else {
                    warn!("Icon {} not found at {}", id, file_path);
                    let mut w = MessageWriter::new();
                    w.write_int(id);
                    w.write_byte(0);
                    Ok(vec![(cmd::REQUEST_ICON, w.into_bytes())])
                }
            }
            
            cmd::GET_EFFDATA => {
                let id = reader.read_short();
                info!("- GET_EFFDATA id={} zoom={}", id, zoom);
                
                let file_path = paths.effect_path(zoom, id as i32);
                if let Some(data) = self.data_store.load_file(&file_path).await {
                    Ok(vec![(cmd::GET_EFFDATA, data)])
                } else {
                    warn!("EffData {} not found at {}", id, file_path);
                    let mut w = MessageWriter::new();
                    w.write_short(id);
                    w.write_byte(0);
                    Ok(vec![(cmd::GET_EFFDATA, w.into_bytes())])
                }
            }
            
            cmd::REQUEST_MAPTEMPLATE => {
                let map_id = reader.read_byte();
                info!("📦 REQUEST_MAPTEMPLATE id={} zoom={}", map_id, zoom);
                
                let file_path = paths.map_path(zoom, map_id as i32);
                if let Some(data) = self.data_store.load_file(&file_path).await {
                    Ok(vec![(cmd::REQUEST_MAPTEMPLATE, data)])
                } else {
                    warn!("MapTemplate {} not found at {}", map_id, file_path);
                    let mut w = MessageWriter::new();
                    w.write_byte(map_id);
                    w.write_byte(0);
                    Ok(vec![(cmd::REQUEST_MAPTEMPLATE, w.into_bytes())])
                }
            }
            
            cmd::REQUEST_NPCTEMPLATE => {
                let npc_id = reader.read_byte();
                info!("📦 REQUEST_NPCTEMPLATE id={} zoom={}", npc_id, zoom);
                
                let file_path = paths.npc_path(zoom, npc_id as i32);
                if let Some(data) = self.data_store.load_file(&file_path).await {
                    Ok(vec![(cmd::REQUEST_NPCTEMPLATE, data)])
                } else {
                    warn!("NpcTemplate {} not found at {}", npc_id, file_path);
                    let mut w = MessageWriter::new();
                    w.write_byte(npc_id);
                    w.write_byte(0);
                    Ok(vec![(cmd::REQUEST_NPCTEMPLATE, w.into_bytes())])
                }
            }
            
            cmd::GET_IMAGE_SOURCE => {
                info!("📦 GET_IMAGE_SOURCE zoom={}", zoom);
                
                let file_path = paths.image_source_path(zoom);
                if let Some(data) = self.data_store.load_file(&file_path).await {
                    Ok(vec![(cmd::GET_IMAGE_SOURCE, data)])
                } else {
                    Ok(vec![(cmd::GET_IMAGE_SOURCE, vec![])])
                }
            }
            
            cmd::GET_IMAGE_SOURCE2 => {
                info!("📦 GET_IMAGE_SOURCE2 zoom={}", zoom);
                
                let file_path = paths.image_source2_path(zoom);
                if let Some(data) = self.data_store.load_file(&file_path).await {
                    Ok(vec![(cmd::GET_IMAGE_SOURCE2, data)])
                } else {
                    Ok(vec![(cmd::GET_IMAGE_SOURCE2, vec![])])
                }
            }
            
            cmd::BACKGROUND_TEMPLATE => {
                let id = reader.read_byte();
                info!("📦 BACKGROUND_TEMPLATE id={} zoom={}", id, zoom);
                
                let file_path = paths.background_path(zoom, id as i32);
                if let Some(data) = self.data_store.load_file(&file_path).await {
                    Ok(vec![(cmd::BACKGROUND_TEMPLATE, data)])
                } else {
                    Ok(vec![(cmd::BACKGROUND_TEMPLATE, vec![])])
                }
            }
            
            cmd::TILE_SET => {
                let id = reader.read_byte();
                info!("📦 TILE_SET id={} zoom={}", id, zoom);
                
                let file_path = paths.tileset_path(zoom, id as i32);
                if let Some(data) = self.data_store.load_file(&file_path).await {
                    Ok(vec![(cmd::TILE_SET, data)])
                } else {
                    Ok(vec![(cmd::TILE_SET, vec![])])
                }
            }
            
            cmd::SMALLIMAGE_VERSION => {
                info!("📦 SMALLIMAGE_VERSION zoom={}", zoom);
                
                let file_path = paths.smallimage_version_path(zoom);
                let version = self.data_store.load_version_file(&file_path).await;
                let mut w = MessageWriter::new();
                w.write_int(version);
                Ok(vec![(cmd::SMALLIMAGE_VERSION, w.into_bytes())])
            }
            
            cmd::BGITEM_VERSION => {
                info!("📦 BGITEM_VERSION zoom={}", zoom);
                
                let file_path = paths.bgitem_version_path(zoom);
                let version = self.data_store.load_version_file(&file_path).await;
                let mut w = MessageWriter::new();
                w.write_int(version);
                Ok(vec![(cmd::BGITEM_VERSION, w.into_bytes())])
            }
            
            _ => {
                warn!("⚠️ Unknown command: {}", msg.command);
                Ok(vec![])
            }
        }
    }
}
