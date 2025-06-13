use axum::{
    body::Body,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::mpsc::UnboundedSender;
use crate::domain::entities::configuration::ConfigMap;

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

// 🌐 HTTP 响应统一格式
#[derive(Debug, Serialize, Deserialize)]
pub struct RestResponse<T> {
    pub success: bool,
    pub code: u16,
    pub message: String,
    pub data: Option<T>,
}

impl<T: Serialize> RestResponse<T> {
    pub fn success(data: T) -> axum::Json<Self> {
        let response = Self {
            success: true,
            code: 200,
            message: "Success".to_string(),
            data: Some(data),
        };
        axum::Json(response)
    }
    
    pub fn error(code: u16, message: String) -> axum::Json<Self> {
        let response = Self {
            success: false,
            code,
            message,
            data: None,
        };
        axum::Json(response)
    }
}

impl<T: Serialize> RestResponse<T> {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

impl<T: Serialize> IntoResponse for RestResponse<T> {
    fn into_response(self) -> Response {
        let json = self.to_json();
        let response = Response::builder()
            .header("Content-Type", "application/json")
            .body(Body::from(json))
            .unwrap();
        response
    }
}
