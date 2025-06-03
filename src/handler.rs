use crate::error::ConfigError;
use crate::model::{Config, ConfigType};
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

pub fn handle_show(path: String) -> Result<(), ConfigError> {
    let content = read_file(&path)?;
    let config = handle_validate(path.clone(), content)?;
    config.show(&path);
    Ok(())
}

pub fn handle_get(path: String, key: String) -> Result<(), ConfigError> {
    let content = read_file(&path)?;
    let config = handle_validate(path.clone(), content)?;
    let value = config.get(&key);
    if let Some(value) = value {
        Config::display_config_value(&key, &value, 0, false);
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
            serde_json::to_string_pretty(&serde_value)
                .map_err(|_| ConfigError::ParseConfigError)?
        }
        ConfigType::Yaml => {
            serde_yaml::to_string(&serde_value)
                .map_err(|_| ConfigError::ParseConfigError)?
        }
        ConfigType::Toml => {
            // TOML需要特殊处理，因为它不支持所有JSON类型
            toml::to_string_pretty(&serde_value)
                .map_err(|_| ConfigError::ParseConfigError)?
        }
        ConfigType::Unknown => {
            return Err(ConfigError::UnknownConfigType);
        }
    };

    // 写入目标文件
    std::fs::write(&output, converted_content)
        .map_err(|e| ConfigError::IoError(e))?;

    println!("✅ 转换完成: {} ({:?}) -> {} ({:?})", 
             input, config.config_type, output, target_format);
    
    Ok(())
}
