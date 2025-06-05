use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::extract::Query;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::routing::get;
use axum::{Router, extract::State};
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, info};

use crate::command::CliCommand;
use crate::error::ConfigError;
use crate::model::app::{AppState, RestResponse};
use crate::model::config::{Config, ConfigType, ConfigValue};
use crate::model::log::LogManager;
use crate::model::template::TemplateType;
use crate::model::validation::{FieldType, Validation, ValidationConfig, ValidationResult};
use crate::{delete_ignore_line, read_file};

pub fn handle_validate(path: String, content: String) -> Result<Config, ConfigError> {
    let mut config_type = ConfigType::Unknown;
    let path = path.trim().to_lowercase();
    if path.is_empty() {
        return Err(ConfigError::EmptyPath);
    } else if path.ends_with(".toml") {
        config_type = ConfigType::Toml;
    } else if path.ends_with(".json") {
        config_type = ConfigType::Json;
    } else if path.ends_with(".yaml") || path.ends_with(".yml") {
        config_type = ConfigType::Yaml;
    }

    let processed_content = delete_ignore_line(&content);

    if config_type == ConfigType::Unknown {
        config_type = parse_file_type(&processed_content)?;
    }

    let conf = Config::from(path, processed_content.clone(), config_type.clone())?;
    Ok(conf)
}

pub fn handle_validate_by_validation_file(
    validation: Validation,
    config: Config,
) -> ValidationResult {
    let validation_config = ValidationConfig::new(validation, config);
    validation_config.validate()
}

fn parse_file_type(content: &str) -> Result<ConfigType, ConfigError> {
    let mut config_type = ConfigType::Unknown;
    if content.is_empty() {
        return Err(ConfigError::EmptyContent);
    }
    if config_type == ConfigType::Unknown {
        let mut lines = content.lines();
        if let Some(line) = lines.next() {
            if (line.starts_with("[")
                && line.ends_with("]")
                && !line.contains(":")
                && !line.contains("{")
                && !line.contains("}")
                && !line.contains(","))
                || (line.contains(" = ") && !line.contains(": "))
            {
                config_type = ConfigType::Toml;
            } else if line.trim().eq("{") || line.trim().eq("[") {
                config_type = ConfigType::Json;
            } else if (line.contains(": ")
                && !line.starts_with("\"")
                && !line.contains("[")
                && !line.contains("{"))
                || line.trim().starts_with('\"') && (!line.contains("{") || !line.contains("["))
            {
                config_type = ConfigType::Yaml;
            }
        }
    }

    match config_type {
        ConfigType::Toml => {}
        ConfigType::Json => {}
        ConfigType::Yaml => {}
        ConfigType::Unknown => {
            return Err(ConfigError::UnknownConfigType);
        }
    }

    Ok(config_type)
}

pub fn handle_show(path: String, depth: usize) -> Result<(), ConfigError> {
    let content = read_file(&path)?;
    let config = handle_validate(path.clone(), content)?;
    config.show(&path, depth);
    Ok(())
}

pub fn handle_get(path: String, key: String) -> Result<(), ConfigError> {
    let content = read_file(&path)?;
    let config = handle_validate(path.clone(), content)?;
    let value = config.get(&key);
    if let Some(value) = value {
        Config::display_config_value(&key, &value, 0, false, 0);
    } else {
        return Err(ConfigError::KeyNotFound);
    }
    Ok(())
}

pub fn handle_convert(input: String, output: String) -> Result<(), ConfigError> {
    let content = read_file(&input)?;
    let config = handle_validate(input.clone(), content)?;

    // 检测目标格式
    let target_format = if output.ends_with(".json") {
        ConfigType::Json
    } else if output.ends_with(".yaml") || output.ends_with(".yml") {
        ConfigType::Yaml
    } else if output.ends_with(".toml") {
        ConfigType::Toml
    } else {
        return Err(ConfigError::UnsupportedFormat {
            format: "无法从文件扩展名识别目标格式".to_string(),
        });
    };

    // 转换为serde_json::Value以避免类型标签
    let serde_value = config.to_serde_value();

    let converted_content = match target_format {
        ConfigType::Json => {
            serde_json::to_string_pretty(&serde_value).map_err(|_| ConfigError::ParseConfigError)?
        }
        ConfigType::Yaml => {
            serde_yaml::to_string(&serde_value).map_err(|_| ConfigError::ParseConfigError)?
        }
        ConfigType::Toml => {
            // TOML需要特殊处理，因为它不支持所有JSON类型
            toml::to_string_pretty(&serde_value).map_err(|_| ConfigError::ParseConfigError)?
        }
        ConfigType::Unknown => {
            return Err(ConfigError::UnknownConfigType);
        }
    };

    // 写入目标文件
    std::fs::write(&output, converted_content).map_err(|e| ConfigError::IoError(e))?;

    println!(
        "✅ 转换完成: {} ({:?}) -> {} ({:?})",
        input, config.config_type, output, target_format
    );

    Ok(())
}

pub fn write_env_config(config: Config, config_path: String) -> Result<(), ConfigError> {
    // 转换为serde_json::Value以避免类型标签
    let serde_value = config.to_serde_value();

    let converted_content = match config.config_type {
        ConfigType::Json => {
            serde_json::to_string_pretty(&serde_value).map_err(|_| ConfigError::ParseConfigError)?
        }
        ConfigType::Yaml => {
            serde_yaml::to_string(&serde_value).map_err(|_| ConfigError::ParseConfigError)?
        }
        ConfigType::Toml => {
            // TOML需要特殊处理，因为它不支持所有JSON类型
            toml::to_string_pretty(&serde_value).map_err(|_| ConfigError::ParseConfigError)?
        }
        ConfigType::Unknown => {
            return Err(ConfigError::UnknownConfigType);
        }
    };

    // 写入目标文件
    std::fs::write(&config_path, converted_content).map_err(|e| ConfigError::IoError(e))?;
    Ok(())
}

pub fn handle_template(template: TemplateType, format: String) -> Result<(), ConfigError> {
    let format = format.trim().to_lowercase();
    if format.is_empty() {
        return Err(ConfigError::UnsupportedFormat {
            format: "无法从文件扩展名识别目标格式".to_string(),
        });
    }
    let format = match format.as_str() {
        "json" => ConfigType::Json,
        "yaml" => ConfigType::Yaml,
        "toml" => ConfigType::Toml,
        _ => {
            return Err(ConfigError::UnsupportedFormat {
                format: "无法从文件扩展名识别目标格式".to_string(),
            });
        }
    };

    let config = Config::get_default_config(template.clone(), format.clone())?;
    config.show(".", 5);
    let serde_value = config.to_serde_value();

    let converted_content = match format {
        ConfigType::Json => {
            serde_json::to_string_pretty(&serde_value).map_err(|_| ConfigError::ParseConfigError)?
        }
        ConfigType::Yaml => {
            serde_yaml::to_string(&serde_value).map_err(|_| ConfigError::ParseConfigError)?
        }
        ConfigType::Toml => {
            // TOML需要特殊处理，因为它不支持所有JSON类型
            toml::to_string_pretty(&serde_value).map_err(|_| ConfigError::ParseConfigError)?
        }
        ConfigType::Unknown => {
            return Err(ConfigError::UnknownConfigType);
        }
    };
    println!("🔧 生成配置文件: {}", converted_content);
    let format_ext = match format {
        ConfigType::Json => "json",
        ConfigType::Yaml => "yaml",
        ConfigType::Toml => "toml",
        ConfigType::Unknown => "txt",
    };
    let output = format!("{}-config.{}", template, format_ext);
    println!("📝 输出文件名: {}", output);
    // 写入目标文件
    std::fs::write(&output, converted_content).map_err(|e| ConfigError::IoError(e))?;

    println!("✅ 模板文件已生成: {}", output);

    Ok(())
}

pub fn get_validation_by_config(config: &Config) -> Result<Validation, ConfigError> {
    let mut validation = Validation::default();
    if let Some(field) = config.get("required_fields") {
        if let ConfigValue::Array(array) = field {
            validation.required_fields = array
                .iter()
                .map(|v| {
                    if let ConfigValue::String(s) = v {
                        s.to_string()
                    } else {
                        "".to_string()
                    }
                })
                .collect::<Vec<String>>();
        }
    }
    debug!("required_fields: {:?}", validation.required_fields);

    if let Some(field) = config.get("field_types") {
        if let ConfigValue::Object(object) = field {
            validation.field_types = object
                .iter()
                .map(|(k, v)| {
                    debug!("k: {k}, v: {:?}", v);
                    let mut field_type = FieldType::String {
                        max_length: None,
                        min_length: None,
                    };
                    if let ConfigValue::Object(object) = v {
                        let mut max_length_parse = None;
                        if let Some(max_length) = object.get("max") {
                            if let ConfigValue::Number(max_length) = max_length {
                                max_length_parse =
                                    Some(max_length.to_string().parse::<usize>().unwrap());
                            }
                        }
                        let mut min_length_parse = None;
                        if let Some(min_length) = object.get("min") {
                            if let ConfigValue::Number(min_length) = min_length {
                                min_length_parse =
                                    Some(min_length.to_string().parse::<usize>().unwrap());
                            }
                        }
                        let mut min_parse = None;
                        if let Some(min) = object.get("min") {
                            if let ConfigValue::Number(min) = min {
                                min_parse = Some(min.to_string().parse::<f64>().unwrap());
                            }
                        }
                        let mut max_parse = None;
                        if let Some(max) = object.get("max") {
                            if let ConfigValue::Number(max) = max {
                                max_parse = Some(max.to_string().parse::<f64>().unwrap());
                            }
                        }

                        field_type = match object.get("type").unwrap().to_string().as_str() {
                            "string" => FieldType::String {
                                max_length: max_length_parse,
                                min_length: min_length_parse,
                            },
                            "number" => FieldType::Number {
                                min: min_parse,
                                max: max_parse,
                            },
                            "boolean" => FieldType::Boolean,
                            _ => FieldType::String {
                                max_length: max_length_parse,
                                min_length: min_length_parse,
                            },
                        };
                    }
                    (k.to_string(), field_type)
                })
                .collect();
        }
    }

    Ok(validation)
}

async fn handle_client(stream: TcpStream, app_state: Arc<Mutex<AppState>>) -> anyhow::Result<()> {
    let stream_addr = stream.local_addr().unwrap();

    let mut reader = BufReader::new(stream);

    loop {
        let mut line = String::new();

        match reader.read_line(&mut line).await {
            Ok(0) => {
                debug!("client closed connection");
                break;
            }
            Ok(_) => {
                let request = line.trim();
                debug!("received request: {}", request);
                let command = CliCommand::from_str(request);
                let mut response = String::new();
                debug!("command: {:?}", command);

                match command {
                    Some(CliCommand::Add { path }) => {
                        debug!("add: {}", path);
                        match read_file(&path) {
                            Ok(content) => match handle_validate(path.clone(), content) {
                                Ok(mut config) => match config.get_env_override_config() {
                                    Ok(config) => {
                                        app_state
                                            .lock()
                                            .unwrap()
                                            .config_map
                                            .insert(path.clone(), config.clone());

                                        // 现在可以安全地使用 await
                                        match write_env_config(
                                            config.clone(),
                                            Path::new(&app_state.lock().unwrap().config_path)
                                                .join(&path)
                                                .to_string_lossy()
                                                .to_string(),
                                        ) {
                                            Ok(_) => {
                                                let config_str = serde_json::to_string(&config)
                                                    .unwrap_or_else(|_| {
                                                        "add config success, but serialize failed"
                                                            .to_string()
                                                    });
                                                response = format!("add result: {}\n", config_str);
                                            }
                                            Err(e) => {
                                                response =
                                                    format!("write config file failed: {}\n", e);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        response = format!("env override failed: {}\n", e);
                                    }
                                },
                                Err(e) => {
                                    response = format!("config validate failed: {}\n", e);
                                }
                            },
                            Err(e) => {
                                response = format!("read file failed: {}\n", e);
                            }
                        }
                    }
                    Some(CliCommand::Remove { path }) => {
                        debug!("remove: {}", path);
                        let removed =
                            { app_state.lock().unwrap().config_map.remove(&path).is_some() }; // MutexGuard 在这里被释放

                        if removed {
                            let removed_path = Path::new(&app_state.lock().unwrap().config_path)
                                .join(path.clone());

                            // 删除文件
                            match tokio::fs::remove_file(&removed_path).await {
                                Ok(_) => {
                                    response = format!("removed config: {}\n", path);
                                }
                                Err(e) => {
                                    response = format!(
                                        "remove config success, but delete file failed: {}\n",
                                        e
                                    );
                                }
                            }
                        } else {
                            response = format!("config not found: {}\n", path);
                        }
                    }
                    Some(CliCommand::Get { path }) => {
                        debug!("get: {}", path);
                        let config_str = {
                            match app_state.lock().unwrap().config_map.get(&path) {
                                Some(config) => match serde_json::to_string(&config) {
                                    Ok(config_str) => Some(config_str),
                                    Err(e) => {
                                        response = format!("serialize config failed: {}\n", e);
                                        None
                                    }
                                },
                                None => {
                                    response = format!("config not found: {}\n", path);
                                    None
                                }
                            }
                        }; // MutexGuard 在这里被释放

                        if let Some(config_str) = config_str {
                            response = format!("{}\n", config_str);
                        }
                    }
                    Some(CliCommand::List) => {
                        debug!("list");
                        let list_response = {
                            if app_state.lock().unwrap().config_map.is_empty() {
                                "no config file loaded".to_string()
                            } else {
                                let mut list_response = String::from("loaded config files:\n");
                                for (key, _) in app_state.lock().unwrap().config_map.iter() {
                                    list_response.push_str(&format!("  - {}\n", key));
                                }
                                list_response
                            }
                        }; // MutexGuard 在这里被释放

                        response = list_response;
                    }

                    Some(CliCommand::Listen { path }) => {
                        debug!("listen: {}", path);

                        // 发送初始响应
                        let initial_config = match app_state.lock().unwrap().config_map.get(&path) {
                            Some(config) => {
                                let mut config_clone = config.clone();
                                format!("{:?}", config_clone.release_config().unwrap().config)
                            }
                            None => format!("config file {} not found", path),
                        };

                        let mut stream = reader.into_inner();
                        let response_bytes_len = initial_config.as_bytes().len();
                        let initial_response =
                            format!("{}\n{}", response_bytes_len, initial_config);

                        if let Err(e) = stream.write_all(initial_response.as_bytes()).await {
                            debug!("send initial response failed: {}", e);
                            break;
                        }
                        if let Err(e) = stream.flush().await {
                            debug!("flush stream failed: {}", e);
                            break;
                        }

                        // 创建通知通道
                        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

                        // 将监听信息存储到 notify_map
                        app_state
                            .lock()
                            .unwrap()
                            .notify_map
                            .insert(stream_addr.to_string(), (path.clone(), tx));

                        debug!("client {} start listen file {}", stream_addr, path);

                        // 启动异步推送任务
                        tokio::spawn(async move {
                            while let Some(config_data) = rx.recv().await {
                                let response_len = config_data.as_bytes().len();
                                let push_response = format!("{}\n{}", response_len, config_data);

                                if let Err(e) = stream.write_all(push_response.as_bytes()).await {
                                    debug!("push data failed: {}", e);
                                    break;
                                }
                                if let Err(e) = stream.flush().await {
                                    debug!("flush stream failed: {}", e);
                                    break;
                                }
                                debug!("push config update success");
                            }
                        });

                        // 跳出循环，该连接现在专门用于推送
                        return Ok(());
                    }

                    None => {
                        debug!("invalid command");
                        response = format!("invalid command: {}\n", request);
                    }
                }

                // 发送响应
                let mut stream = reader.into_inner();
                let response_bytes_len = response.as_bytes().len();
                let response = format!("{}\n{}", response_bytes_len, response);
                if let Err(e) = stream.write_all(response.as_bytes()).await {
                    debug!("send response failed: {}", e);
                    break;
                }
                if let Err(e) = stream.flush().await {
                    debug!("flush stream failed: {}", e);
                    break;
                }

                debug!("send response: {}", response.trim());

                // 重新创建reader以继续读取下一个请求
                reader = BufReader::new(stream);
            }
            Err(e) => {
                debug!("read request failed: {}", e);
                break;
            }
        }
    }

    Ok(())
}

pub async fn handle_serve(
    port: u16,
    host: String,
    config_path: String,
    mut log_manager: LogManager,
) -> anyhow::Result<()> {
    debug!(
        "serve port: {} host: {} config path: {}",
        port, host, config_path
    );
    let app_state = AppState::new(port, host, config_path);
    let app_state = Arc::new(Mutex::new(app_state));

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
            let config = handle_validate(path.to_string_lossy().to_string(), content)?;
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

    // 创建通道用于异步通知
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(String, String)>();

    // 启动异步任务处理通知
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

    let app_state_for_watcher = app_state.clone();
    let mut watcher = RecommendedWatcher::new(
        move |result: notify::Result<Event>| {
            let event = match result {
                Ok(event) => event,
                Err(e) => {
                    debug!("文件监听错误: {}", e);
                    return;
                }
            };

            if event.kind.is_modify() && !event.paths.contains(&PathBuf::from("target")) {
                debug!("event: {:?}", event);
                if let Some(file_path) = event.paths.last() {
                    if let Some(file_name_os) = file_path.file_name() {
                        let file_name = file_name_os.to_string_lossy().to_string();
                        debug!("file_name: {:?}", file_name);
                        
                        // 过滤临时文件和非配置文件
                        if file_name.starts_with('.') || file_name.ends_with(".tmp") || file_name.ends_with("~") {
                            debug!("忽略临时文件: {}", file_name);
                            return;
                        }
                        
                        // 只处理配置文件类型
                        if !file_name.ends_with(".toml") && !file_name.ends_with(".json") && 
                           !file_name.ends_with(".yaml") && !file_name.ends_with(".yml") {
                            debug!("忽略非配置文件: {}", file_name);
                            return;
                        }
                        
                        match std::fs::read_to_string(file_path) {
                            Ok(content) => {
                                match handle_validate(file_name.clone(), content) {
                                    Ok(validated_config) => {
                                        match validated_config.clone().release_config() {
                                            Ok(config_for_notify) => {
                                                let config_str = serde_json::to_string(&config_for_notify.to_serde_value()).unwrap_or_else(|_| "{}".to_string());

                                                app_state_for_watcher
                                                    .lock()
                                                    .unwrap()
                                                    .config_map
                                                    .insert(file_name.clone(), validated_config);

                                                // 通过通道发送通知请求
                                                if let Err(_) = tx.send((file_name.clone(), config_str)) {
                                                    debug!("notify channel is closed");
                                                }

                                                info!("config watcher event: {:?}", event);
                                            }
                                            Err(e) => {
                                                debug!("配置释放失败: {} - {}", file_name, e);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        debug!("配置验证失败: {} - {}", file_name, e);
                                    }
                                }
                            }
                            Err(e) => {
                                debug!("读取文件失败: {} - {}", file_name, e);
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

    let host = app_state.lock().unwrap().host.clone();
    let port = app_state.lock().unwrap().port.clone();
    let listener = TcpListener::bind((host, port)).await?;
    info!("server init finished");
    loop {
        let (stream, _) = listener.accept().await?;
        let app_state_cloned = app_state.clone();
        tokio::spawn(async move {
            let _ = handle_client(stream, app_state_cloned).await;
        });
    }
}

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
            let config = handle_validate(path.to_string_lossy().to_string(), content)?;
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
                    debug!("文件监听错误: {}", e);
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
                        if file_name.starts_with('.') || file_name.ends_with(".tmp") || file_name.ends_with("~") {
                            debug!("忽略临时文件: {}", file_name);
                            return;
                        }
                        
                        // 只处理配置文件类型
                        if !file_name.ends_with(".toml") && !file_name.ends_with(".json") && 
                           !file_name.ends_with(".yaml") && !file_name.ends_with(".yml") {
                            debug!("忽略非配置文件: {}", file_name);
                            return;
                        }
                        
                        match std::fs::read_to_string(file_path) {
                            Ok(content) => {
                                match handle_validate(file_name.clone(), content) {
                                    Ok(validated_config) => {
                                        match validated_config.clone().release_config() {
                                            Ok(config_for_notify) => {
                                                let config_str = serde_json::to_string(&config_for_notify.to_serde_value()).unwrap_or_else(|_| "{}".to_string());

                                                app_state_for_watcher
                                                    .lock()
                                                    .unwrap()
                                                    .config_map
                                                    .insert(file_name.clone(), validated_config);

                                                // 通过通道发送通知请求
                                                if let Err(_) = tx.send((file_name.clone(), config_str)) {
                                                    debug!("notify channel is closed");
                                                }

                                                info!("config watcher event: {:?}", event);
                                            }
                                            Err(e) => {
                                                debug!("配置释放失败: {} - {}", file_name, e);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        debug!("配置验证失败: {} - {}", file_name, e);
                                    }
                                }
                            }
                            Err(e) => {
                                debug!("读取文件失败: {} - {}", file_name, e);
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
        .route("/ws/listen", get(handle_websocket_upgrade)) // 🔌 WebSocket 路由
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
        Some(mut config) => match config.release_config() {
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
    match handle_validate(path.clone(), body) {
        Ok(config) => {
            let mut app_state = state.lock().unwrap();
            app_state.config_map.insert(path.clone(), config.clone());
            let config_path = format!("{}/{}", app_state.config_path, path);
            write_env_config(config, config_path).unwrap();
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

// 📋 WebSocket 查询参数
#[derive(Deserialize)]
struct WsQuery {
    file: String, // 要监听的配置文件名
}

// 🔌 WebSocket 升级处理
async fn handle_websocket_upgrade(
    State(state): State<Arc<Mutex<AppState>>>,
    query: Result<Query<WsQuery>, axum::extract::rejection::QueryRejection>,
    ws: WebSocketUpgrade,
) -> axum::response::Response {
    match query {
        Ok(Query(query)) => {
            info!("WebSocket 升级请求成功 - 文件: {}", query.file);
            
            // 检查文件是否存在于配置映射中
            let file_exists = {
                let app_state = state.lock().unwrap();
                app_state.config_map.contains_key(&query.file)
            };
            
            if !file_exists {
                info!("警告：请求的文件 {} 不在配置映射中", query.file);
            }
            
            ws.on_upgrade(move |socket| handle_websocket_connection(socket, state, query.file))
        }
        Err(e) => {
            info!("WebSocket 查询参数解析失败: {}", e);
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
    info!("新的 WebSocket 连接，监听文件: {}", file_name);

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
                match config_clone.release_config() {
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
                "message": format!("配置文件 {} 不存在", file_name)
            })
            .to_string(),
        }
    };

    // 发送初始配置
    if let Err(e) = socket.send(Message::Text(initial_config.into())).await {
        debug!("发送初始配置失败: {}", e);
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

    info!("WebSocket 客户端 {} 开始监听文件 {}", client_id, file_name);

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
                            debug!("推送配置更新失败: {}", e);
                            break;
                        }
                        debug!("成功推送配置更新到 WebSocket 客户端 {}", client_id_for_send);
                    } else {
                        break;
                    }
                }
                // 处理内部消息（如pong响应）
                internal_msg = internal_rx.recv() => {
                    if let Some(msg) = internal_msg {
                        if let Err(e) = sender.send(Message::Text(msg.into())).await {
                            debug!("发送内部消息失败: {}", e);
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
                debug!("收到 WebSocket 消息: {}", text_str);
                // 处理ping消息
                if text_str == "ping" {
                    let pong = serde_json::json!({
                        "type": "pong",
                        "timestamp": Utc::now().to_rfc3339()
                    })
                    .to_string();

                    if let Err(_) = internal_tx.send(pong) {
                        debug!("发送 pong 到内部通道失败");
                        break;
                    }
                }
            }
            Ok(Message::Close(_)) => {
                debug!("WebSocket 客户端 {} 主动关闭连接", client_id);
                break;
            }
            Err(e) => {
                debug!("WebSocket 接收错误: {}", e);
                break;
            }
            _ => {
                debug!("收到未知 WebSocket 消息类型");
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

    info!("WebSocket 客户端 {} 断开连接", client_id);
}
