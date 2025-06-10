use std::sync::{Arc, Mutex};

use axum::extract::{ws::{Message, WebSocket}, Query, State, WebSocketUpgrade};
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use tracing::{debug, info};

use crate::{application::dtos::ws_query::WsQuery, domain::services::env_override::EnvOverrideService, shared::app_state::AppState};

// 🔌 WebSocket 升级处理
pub async fn handle_websocket_upgrade(
    State(state): State<Arc<Mutex<AppState>>>,
    query: Result<Query<WsQuery>, axum::extract::rejection::QueryRejection>,
    ws: WebSocketUpgrade,
) -> axum::response::Response {
    match query {
        Ok(Query(query)) => {
            info!("WebSocket upgrade request success - file: {}", query.file);
            
            // 检查文件是否存在于配置映射中
            let file_exists = {
                let app_state = state.lock().unwrap();
                app_state.config_map.contains_key(&query.file)
            };
            
            if !file_exists {
                info!("warning: request file {} not in config map", query.file);
            }
            
            ws.on_upgrade(move |socket| handle_websocket_connection(socket, state, query.file))
        }
        Err(e) => {
            info!("WebSocket query parameters parse failed: {}", e);
            axum::response::Response::builder()
                .status(400)
                .body("Bad Request: Invalid query parameters".into())
                .unwrap()
        }
    }
}

// 🔌 WebSocket 连接处理
async fn handle_websocket_connection(
    mut socket: WebSocket,
    state: Arc<Mutex<AppState>>,
    file_name: String,
) {
    info!("new WebSocket connection, watching file: {}", file_name);

    // 生成唯一的客户端ID
    let client_id = format!(
        "ws_{}_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
        rand::random::<u32>()
    );

    // 发送初始配置
    let initial_config = {
        let app_state = state.lock().unwrap();
        match app_state.config_map.get(&file_name) {
            Some(config) => {
                let mut config_clone = config.clone();
                match EnvOverrideService::apply_env_override(&mut config_clone) {
                    Ok(released_config) => serde_json::to_string(&serde_json::json!({
                        "type": "initial",
                        "file": file_name,
                        "config": released_config.to_serde_value()
                    }))
                    .unwrap_or_else(|_| "{}".to_string()),
                    Err(e) => serde_json::json!({
                        "type": "error",
                        "message": format!("Failed to process config: {}", e)
                    })
                    .to_string(),
                }
            }
            None => serde_json::json!({
                "type": "error",
                "message": format!("config file {} not found", file_name)
            })
            .to_string(),
        }
    };

    // 发送初始配置
    if let Err(e) = socket.send(Message::Text(initial_config.into())).await {
        debug!("send initial config failed: {}", e);
        return;
    }

    // 创建通知通道
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    // 将 WebSocket 连接注册到通知系统
    {
        let mut app_state = state.lock().unwrap();
        app_state
            .notify_map
            .insert(client_id.clone(), (file_name.clone(), tx));
    }

    info!("WebSocket client {} start watching file {}", client_id, file_name);

    // 分别处理发送和接收
    let (mut sender, mut receiver) = socket.split();

    // 创建一个通道用于从接收端向发送端传递消息
    let (internal_tx, mut internal_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    // 启动发送任务，处理配置更新推送和内部消息
    let client_id_for_send = client_id.clone();
    let file_name_for_send = file_name.clone();
    let send_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                // 处理配置更新推送
                config_data = rx.recv() => {
                    if let Some(config_data) = config_data {
                        let message = serde_json::json!({
                            "type": "update",
                            "file": file_name_for_send,
                            "config": config_data,
                            "timestamp": Utc::now().to_rfc3339()
                        }).to_string();

                        if let Err(e) = sender.send(Message::Text(message.into())).await {
                            debug!("push config update failed: {}", e);
                            break;
                        }
                        debug!("push config update to WebSocket client {} success", client_id_for_send);
                    } else {
                        break;
                    }
                }
                // 处理内部消息（如pong响应）
                internal_msg = internal_rx.recv() => {
                    if let Some(msg) = internal_msg {
                        if let Err(e) = sender.send(Message::Text(msg.into())).await {
                            debug!("send internal message failed: {}", e);
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }
        }
    });

    // 处理客户端消息（保持连接活跃）
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                let text_str = text.to_string();
                debug!("receive WebSocket message: {}", text_str);
                // 处理ping消息
                if text_str == "ping" {
                    let pong = serde_json::json!({
                        "type": "pong",
                        "timestamp": Utc::now().to_rfc3339()
                    })
                    .to_string();

                    if let Err(_) = internal_tx.send(pong) {
                        debug!("send pong to internal channel failed");
                        break;
                    }
                }
            }
            Ok(Message::Close(_)) => {
                debug!("WebSocket client {} closed connection", client_id);
                break;
            }
            Err(e) => {
                debug!("WebSocket receive error: {}", e);
                break;
            }
            _ => {
                debug!("unknown WebSocket message type");
            }
        }
    }

    // 清理：从通知映射中移除该客户端
    {
        let mut app_state = state.lock().unwrap();
        app_state.notify_map.remove(&client_id);
    }

    // 取消发送任务
    send_task.abort();

    info!("WebSocket client {} disconnected", client_id);
}