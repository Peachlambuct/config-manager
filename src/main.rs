use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use clap::Parser;
use colored::{Color, Colorize};
use config_manager::command::{Command, Subcommand};
use config_manager::handler::{
    get_validation_by_config, handle_client, handle_convert, handle_get, handle_show,
    handle_template, handle_validate, handle_validate_by_validation_file,
};

use config_manager::model::app::AppState;
use config_manager::model::log::{LogConfig, LogManager};
use config_manager::model::template::TemplateType;
use config_manager::{init_tracing, read_file};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::net::TcpListener;
use tracing::{debug, info};

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let mut log_manager = LogManager::new(LogConfig {
        file: "test.log".to_string(),
        level: "info".to_string(),
    })
    .await;

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
                            debug!("向客户端发送通知失败，可能连接已断开");
                        }
                    }
                    debug!(
                        "已向 {} 个客户端发送 {} 的更新通知",
                        sender_count, file_name
                    );
                }
            });

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
                        let validated_config = handle_validate(file_name.clone(), content).unwrap();
                        let config_for_notify = validated_config.clone().release_config().unwrap();
                        let config_str = format!("{:?}", config_for_notify.config);

                        app_state_for_watcher
                            .lock()
                            .unwrap()
                            .config_map
                            .insert(file_name.clone(), validated_config);

                        // 通过通道发送通知请求
                        if let Err(_) = tx.send((file_name.clone(), config_str)) {
                            debug!("通知通道已关闭");
                        }

                        info!("config watcher event: {:?}", event);
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
