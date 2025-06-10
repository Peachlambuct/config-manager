use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use notify::{Event, RecursiveMode, Watcher};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
};
use tracing::{debug, info};

use crate::{
    domain::{services::{
        env_override::EnvOverrideService, format_converter::FormatConverterService,
    }, value_objects::config_path::ConfigPath},
    infrastructure::{
        logging::log_manager::LogManager,
        repositories::file_config_repository::FileConfigRepository,
    },
    interfaces::cli::command::CliCommand,
    shared::{app_state::AppState, utils::read_file},
};

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
                            Ok(content) => {
                                match FormatConverterService::new(
                                    ConfigPath::new(path.clone(), true).unwrap(),
                                    content,
                                )
                                .validate_config()
                                {
                                    Ok(mut config) => {
                                        match EnvOverrideService::apply_env_override(&mut config) {
                                            Ok(config) => {
                                                app_state
                                                    .lock()
                                                    .unwrap()
                                                    .config_map
                                                    .insert(path.clone(), config.clone());

                                                // 现在可以安全地使用 await
                                                match FileConfigRepository::new(
                                                    app_state.lock().unwrap().config_path.clone(),
                                                )
                                                .save(config.clone())
                                                {
                                                    Ok(_) => {
                                                        let config_str = serde_json::to_string(&config)
                                                    .unwrap_or_else(|_| {
                                                        "add config success, but serialize failed"
                                                            .to_string()
                                                    });
                                                        response =
                                                            format!("add result: {}\n", config_str);
                                                    }
                                                    Err(e) => {
                                                        response = format!(
                                                            "write config file failed: {}\n",
                                                            e
                                                        );
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                response = format!("env override failed: {}\n", e);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        response = format!("config validate failed: {}\n", e);
                                    }
                                }
                            }
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
                                format!(
                                    "{:?}",
                                    EnvOverrideService::apply_env_override(&mut config_clone)
                                        .unwrap()
                                        .config
                                )
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
    let mut watcher = notify::RecommendedWatcher::new(
        move |result: notify::Result<Event>| {
            let event = match result {
                Ok(event) => event,
                Err(e) => {
                    debug!("file watch error: {}", e);
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
