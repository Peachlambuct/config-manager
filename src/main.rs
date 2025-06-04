use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

use anyhow::Result;
use clap::Parser;
use colored::{Color, Colorize};
use config_manager::command::{CliCommand, Command, Subcommand};
use config_manager::handler::{
    get_validation_by_config, handle_convert, handle_get, handle_show, handle_template,
    handle_validate, handle_validate_by_validation_file, write_env_config,
};
use config_manager::model::config::ConfigMap;
use config_manager::model::template::TemplateType;
use config_manager::{init_tracing, read_file};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, info};

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let command = Command::parse();

    match command.subcommand {
        Subcommand::Validate {
            file,
            validate_file,
        } => {
            if validate_file.is_empty() {
                debug!("validate: {}", file);
                let content = read_file(&file)?;
                let config = handle_validate(file, content)?;
                println!(
                    "config validate success, file format is {}",
                    (config.config_type).to_string().color(Color::Green)
                );
            } else {
                debug!("validate: {}", validate_file);
                let validation_content = read_file(&validate_file)?;
                let validation_config = handle_validate(validate_file, validation_content)?;
                let validation = get_validation_by_config(&validation_config).unwrap();
                let content = read_file(&file)?;
                let config = handle_validate(file.clone(), content)?;
                let config_type = config.config_type.clone();
                debug!("config: {:?}", config);
                let validation_result = handle_validate_by_validation_file(validation, config);
                if !validation_result.is_valid {
                    println!(
                        "{} config validate failed: {:?}",
                        file.color(Color::Red),
                        validation_result.errors
                    );
                } else {
                    println!(
                        "{} config validate success, file format is {}",
                        file.color(Color::Green),
                        config_type.to_string().color(Color::Green)
                    );
                }
            }
        }
        Subcommand::Show { file, get, deepth } => {
            if get.is_empty() {
                handle_show(file, deepth)?;
            } else {
                handle_get(file, get)?;
            }
        }
        Subcommand::Convert { input, output } => {
            debug!("convert: {} -> {}", input, output);
            handle_convert(input, output)?;
        }
        Subcommand::Template { template, format } => {
            debug!("template: {} {}", template, format);
            handle_template(TemplateType::from(template), format)?;
        }
        Subcommand::Serve {
            port,
            host,
            config_path,
        } => {
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

            let app_state_for_watcher = app_state.clone();
            let mut watcher = RecommendedWatcher::new(
                move |result: notify::Result<Event>| {
                    let event = result.unwrap();

                    if event.kind.is_modify() && !event.paths.contains(&PathBuf::from("target")) {
                        debug!("event: {:?}", event);
                        let file_path = event.paths.last().unwrap();
                        let file_name =
                            file_path.file_name().unwrap().to_string_lossy().to_string();
                        debug!("file_name: {:?}", file_name);
                        let content = std::fs::read_to_string(file_path).unwrap();
                        let config = handle_validate(file_name.clone(), content).unwrap();
                        app_state_for_watcher
                            .lock()
                            .unwrap()
                            .config_map
                            .insert(file_name, config);
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
    }
    Ok(())
}

async fn handle_client(stream: TcpStream, app_state: Arc<Mutex<AppState>>) -> Result<()> {
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
                                    list_response.push_str(&format!("  - {} ", key));
                                }
                                list_response
                            }
                        }; // MutexGuard 在这里被释放

                        response = list_response;
                    }
                    Some(CliCommand::Update { old_path, new_path }) => {
                        debug!("update: {} -> {}", old_path, new_path);
                        let update_result = {
                            if let Some(config) =
                                app_state.lock().unwrap().config_map.remove(&old_path)
                            {
                                app_state
                                    .lock()
                                    .unwrap()
                                    .config_map
                                    .insert(new_path.clone(), config);
                                true
                            } else {
                                false
                            }
                        }; // MutexGuard 在这里被释放

                        if update_result {
                            response =
                                format!("updated config path: {} -> {}\n", old_path, new_path);
                        } else {
                            response = format!("source config not found: {}\n", old_path);
                        }
                    }
                    None => {
                        debug!("invalid command");
                        response = format!("invalid command: {}\n", request);
                    }
                }

                // 发送响应
                let mut stream = reader.into_inner();

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

struct AppState {
    config_map: ConfigMap,
    port: u16,
    host: String,
    config_path: String,
}

impl AppState {
    pub fn new(port: u16, host: String, config_path: String) -> Self {
        Self {
            config_map: ConfigMap::new(),
            port,
            host,
            config_path,
        }
    }
}

