use std::path::Path;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tracing::debug;

use crate::command::CliCommand;
use crate::error::ConfigError;
use crate::model::app::AppState;
use crate::model::config::{Config, ConfigType, ConfigValue};
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

pub async fn handle_client(
    stream: TcpStream,
    app_state: Arc<Mutex<AppState>>,
) -> anyhow::Result<()> {
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
                            None => format!("配置文件 {} 不存在", path)
                        };

                        let mut stream = reader.into_inner();
                        let response_bytes_len = initial_config.as_bytes().len();
                        let initial_response = format!("{}\n{}", response_bytes_len, initial_config);
                        
                        if let Err(e) = stream.write_all(initial_response.as_bytes()).await {
                            debug!("发送初始响应失败: {}", e);
                            break;
                        }
                        if let Err(e) = stream.flush().await {
                            debug!("刷新流失败: {}", e);
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

                        debug!("客户端 {} 开始监听文件 {}", stream_addr, path);
                        
                        // 启动异步推送任务
                        tokio::spawn(async move {
                            while let Some(config_data) = rx.recv().await {
                                let response_len = config_data.as_bytes().len();
                                let push_response = format!("{}\n{}", response_len, config_data);
                                
                                if let Err(e) = stream.write_all(push_response.as_bytes()).await {
                                    debug!("推送数据失败: {}", e);
                                    break;
                                }
                                if let Err(e) = stream.flush().await {
                                    debug!("刷新流失败: {}", e);
                                    break;
                                }
                                debug!("成功推送配置更新");
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
