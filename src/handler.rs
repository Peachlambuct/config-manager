use tracing::debug;

use crate::error::ConfigError;
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
