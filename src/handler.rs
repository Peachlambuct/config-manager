use crate::error::ConfigError;
use crate::model::ConfigType;
use crate::{delete_ignore_line, read_file};
pub fn handle_validate(path: String) -> Result<(), ConfigError> {
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

    let content = delete_ignore_line(&read_file(&path)?);
    if content.is_empty() {
        return Err(ConfigError::EmptyContent);
    }
    if config_type == ConfigType::Unknown {
        let mut lines = content.lines();
        if let Some(line) = lines.next() {
            if line.starts_with("[")
                && line.ends_with("]")
                && !line.contains(":")
                && !line.contains("{")
                && !line.contains("}")
                && !line.contains(",")
                || (line.contains(" = ") && !line.contains(": "))
            {
                config_type = ConfigType::Toml;
            } else if line.trim().eq("{") || line.trim().eq("[") {
                config_type = ConfigType::Json;
            } else if line.contains(": ")
                && !line.starts_with("\"")
                && !line.contains("[")
                && !line.contains("{")
                || line.trim().starts_with('\"') && (!line.contains("{") || !line.contains("["))
            {
                config_type = ConfigType::Yaml;
            }
        }
    }

    match config_type {
        ConfigType::Toml => {
            println!("file type: toml");
        }
        ConfigType::Json => {
            println!("file type: json");
        }
        ConfigType::Yaml => {
            println!("file type: yaml");
        }
        ConfigType::Unknown => {
            return Err(ConfigError::UnknownConfigType);
        }
    }

    Ok(())
}
