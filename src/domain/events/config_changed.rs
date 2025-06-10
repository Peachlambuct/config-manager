use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigChangedEvent {
    pub config_path: String,
    pub old_checksum: String,
    pub new_checksum: String,
    pub changed_by: String, // 变更来源：file_watcher, http_api, tcp_client
    pub timestamp: DateTime<Utc>,
}

impl ConfigChangedEvent {
    pub fn new(
        config_path: String,
        old_checksum: String,
        new_checksum: String,
        changed_by: String,
    ) -> Self {
        Self {
            config_path,
            old_checksum,
            new_checksum,
            changed_by,
            timestamp: Utc::now(),
        }
    }
}
