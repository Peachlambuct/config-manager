use serde::Deserialize;

// 📋 WebSocket 查询参数
#[derive(Deserialize)]
pub struct WsQuery {
    pub file: String, // 要监听的配置文件名
}