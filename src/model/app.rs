use std::collections::HashMap;
use tokio::sync::mpsc::UnboundedSender;

use crate::model::config::ConfigMap;

pub struct AppState {
    pub config_map: ConfigMap,
    pub port: u16,
    pub host: String,
    pub config_path: String,
    pub notify_map: NotifyMap,
}

impl AppState {
    pub fn new(port: u16, host: String, config_path: String) -> Self {
        Self {
            config_map: ConfigMap::new(),
            port,
            host,
            config_path,
            notify_map: NotifyMap::new(),
        }
    }
}

// 存储监听者信息：客户端ID -> (文件路径, 通知发送器)
type NotifyMap = HashMap<String, (String, UnboundedSender<String>)>;
