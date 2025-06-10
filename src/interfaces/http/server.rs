use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use axum::{Router, extract::State, routing::get};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use tracing::{debug, info};

use crate::{
    domain::{services::{env_override::EnvOverrideService, format_converter::FormatConverterService}, value_objects::config_path::ConfigPath},
    infrastructure::{
        logging::log_manager::LogManager,
        repositories::file_config_repository::FileConfigRepository,
    },
    shared::{
        app_state::{AppState, RestResponse},
        utils::read_file,
    },
};

pub async fn handle_http(
    port: u16,
    host: String,
    app_state: Arc<Mutex<AppState>>,
    mut log_manager: LogManager,
) -> anyhow::Result<()> {
    info!(
        "check config path: {}",
        app_state.lock().unwrap().config_path
    );
    if !Path::new(&app_state.lock().unwrap().config_path).exists() {
        info!("config path not found, create it");
        std::fs::create_dir_all(app_state.lock().unwrap().config_path.clone())?;
    }
    info!(
        "load config from path: {}",
        app_state.lock().unwrap().config_path
    );

    // 先获取配置路径，避免在循环中持有锁
    let config_path = app_state.lock().unwrap().config_path.clone();

    // 收集所有配置文件到临时 HashMap
    let mut configs_to_load = HashMap::new();

    // 遍历文件夹中的文件
    for entry in std::fs::read_dir(config_path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            info!("load config file: {}", path.to_string_lossy().to_string());
            let content = read_file(path.to_string_lossy().to_string().as_str())?;
            info!("content: {}", content);
            let config = FormatConverterService::new(
                ConfigPath::new(path.to_string_lossy().to_string(), true).unwrap(),
                content,
            )
            .validate_config()?;
            info!("config: {:?}", config);

            // 添加到临时 HashMap，不需要获取锁
            configs_to_load.insert(entry.file_name().to_string_lossy().to_string(), config);
        }
    }

    // 批量插入所有配置，只获取一次锁
    {
        let mut app_state_guard = app_state.lock().unwrap();
        for (key, config) in configs_to_load {
            app_state_guard.config_map.insert(key, config);
        }
    } // 锁在这里被释放

    info!(
        "config loaded finished: {} files",
        app_state.lock().unwrap().config_map.len()
    );
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(String, String)>();
    let app_state_for_notify = app_state.clone();
    tokio::spawn(async move {
        while let Some((file_name, config_str)) = rx.recv().await {
            let notify_senders: Vec<tokio::sync::mpsc::UnboundedSender<String>> = {
                let app_state_guard = app_state_for_notify.lock().unwrap();
                app_state_guard
                    .notify_map
                    .iter()
                    .filter(|(_, (watched_file, _))| *watched_file == file_name)
                    .map(|(_, (_, sender))| sender.clone())
                    .collect()
            };
            log_manager
                .log_info(format!(
                    "config file: {} updated, notify {} clients, config: {}",
                    file_name,
                    notify_senders.len(),
                    config_str
                ))
                .await;
            let sender_count = notify_senders.len();
            for sender in notify_senders {
                if let Err(_) = sender.send(config_str.clone()) {
                    debug!("send config to client failed, maybe client is closed");
                }
            }
            debug!("send {} config to {} clients", sender_count, file_name);
        }
    });
    // HTTP 版本的文件监听器
    let app_state_for_watcher = app_state.clone();
    let mut watcher = RecommendedWatcher::new(
        move |result: notify::Result<Event>| {
            let event = match result {
                Ok(event) => event,
                Err(e) => {
                    debug!("file watch error: {}", e);
                    return;
                }
            };

            if event.kind.is_modify() && !event.paths.contains(&PathBuf::from("target")) {
                debug!("config file modified event: {:?}", event);
                if let Some(file_path) = event.paths.last() {
                    if let Some(file_name_os) = file_path.file_name() {
                        let file_name = file_name_os.to_string_lossy().to_string();
                        debug!("file_name: {:?}", file_name);

                        // 过滤临时文件和非配置文件
                        if file_name.starts_with('.')
                            || file_name.ends_with(".tmp")
                            || file_name.ends_with("~")
                        {
                            debug!("ignore temporary file: {}", file_name);
                            return;
                        }

                        // 只处理配置文件类型
                        if !file_name.ends_with(".toml")
                            && !file_name.ends_with(".json")
                            && !file_name.ends_with(".yaml")
                            && !file_name.ends_with(".yml")
                        {
                            debug!("ignore non-config file: {}", file_name);
                            return;
                        }

                        match std::fs::read_to_string(file_path) {
                            Ok(content) => {
                                match FormatConverterService::new(
                                    ConfigPath::new(file_name.clone(), true).unwrap(),
                                    content,
                                )
                                .validate_config()
                                {
                                    Ok(mut validated_config) => {
                                        match EnvOverrideService::apply_env_override(
                                            &mut validated_config,
                                        ) {
                                            Ok(config_for_notify) => {
                                                let config_str = serde_json::to_string(
                                                    &config_for_notify.to_serde_value(),
                                                )
                                                .unwrap_or_else(|_| "{}".to_string());

                                                app_state_for_watcher
                                                    .lock()
                                                    .unwrap()
                                                    .config_map
                                                    .insert(file_name.clone(), validated_config);

                                                // 通过通道发送通知请求
                                                if let Err(_) =
                                                    tx.send((file_name.clone(), config_str))
                                                {
                                                    debug!("notify channel is closed");
                                                }

                                                info!("config watcher event: {:?}", event);
                                            }
                                            Err(e) => {
                                                debug!(
                                                    "config release failed: {} - {}",
                                                    file_name, e
                                                );
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        debug!("config validate failed: {} - {}", file_name, e);
                                    }
                                }
                            }
                            Err(e) => {
                                debug!("read file failed: {} - {}", file_name, e);
                            }
                        }
                    }
                }
            }
        },
        notify::Config::default(),
    )?;
    watcher.watch(
        Path::new(&app_state.lock().unwrap().config_path),
        RecursiveMode::Recursive,
    )?;
    info!("config watcher init finished");

    let app = Router::new()
        .route("/", get(handle_http_root))
        .route("/api/configs", get(handle_http_list_configs))
        .route(
            "/api/configs/{path}",
            get(handle_http_get_config)
                .put(handle_http_update_config)
                .delete(handle_http_delete_config),
        )
        .route(
            "/ws/listen",
            get(crate::interfaces::websocket::server::handle_websocket_upgrade),
        ) // 🔌 WebSocket 路由
        .with_state(app_state); // 🔑 关键：将状态附加到路由

    let addr = (host.clone(), port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("HTTP server listening on {}:{}", host, port);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn handle_http_root() -> axum::Json<RestResponse<String>> {
    RestResponse::success("🔧 ConfigMaster HTTP API Server".to_string())
}

async fn handle_http_list_configs(
    State(state): State<Arc<Mutex<AppState>>>,
) -> impl axum::response::IntoResponse {
    let configs: Vec<String> = {
        let app_state = state.lock().unwrap();
        app_state.config_map.keys().cloned().collect()
    };

    RestResponse::success(configs)
}

async fn handle_http_get_config(
    State(state): State<Arc<Mutex<AppState>>>,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> impl axum::response::IntoResponse {
    let config_result = {
        let app_state = state.lock().unwrap();
        app_state.config_map.get(&path).cloned()
    };

    match config_result {
        Some(mut config) => match EnvOverrideService::apply_env_override(&mut config) {
            Ok(released_config) => RestResponse::success(serde_json::json!({
                "path": released_config.path,
                "type": released_config.config_type,
                "config": released_config.to_serde_value()
            })),
            Err(e) => RestResponse::<serde_json::Value>::error(
                400,
                format!("Failed to process config: {}", e),
            ),
        },
        None => {
            RestResponse::<serde_json::Value>::error(404, format!("Config '{}' not found", path))
        }
    }
}

async fn handle_http_update_config(
    State(state): State<Arc<Mutex<AppState>>>,
    axum::extract::Path(path): axum::extract::Path<String>,
    body: String,
) -> impl axum::response::IntoResponse {
    match FormatConverterService::new(ConfigPath::new(path.clone(), true).unwrap(), body)
        .validate_config()
    {
        Ok(config) => {
            let mut app_state = state.lock().unwrap();
            app_state.config_map.insert(path.clone(), config.clone());
            FileConfigRepository::new(app_state.config_path.clone()).save(config).unwrap();
            RestResponse::success(format!("Config '{}' updated successfully", path))
        }
        Err(e) => RestResponse::<String>::error(400, format!("Failed to update config: {}", e)),
    }
}

async fn handle_http_delete_config(
    State(state): State<Arc<Mutex<AppState>>>,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> impl axum::response::IntoResponse {
    let removed = {
        let mut app_state = state.lock().unwrap();
        app_state.config_map.remove(&path).is_some()
    };

    if removed {
        RestResponse::success(format!("Config '{}' deleted successfully", path))
    } else {
        RestResponse::<String>::error(404, format!("Config '{}' not found", path))
    }
}
