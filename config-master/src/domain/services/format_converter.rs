use crate::{
    domain::{
        entities::configuration::Config,
        value_objects::{config_format::ConfigType, config_path::ConfigPath},
    }, shared::{error::ConfigError, utils::delete_ignore_line}
};

pub struct FormatConverterService {
    pub config_path: ConfigPath,
    pub content: String,
}

impl FormatConverterService {
    pub fn new(config_path: ConfigPath, content: String) -> Self {
        Self { config_path, content }
    }

    pub fn validate_config(&self) -> Result<Config, ConfigError> {
        let mut config_type = ConfigType::Unknown;
        let path = self.config_path.as_str().trim().to_lowercase();
        if path.is_empty() {
            return Err(ConfigError::EmptyPath);
        } else if path.ends_with(".toml") {
            config_type = ConfigType::Toml;
        } else if path.ends_with(".json") {
            config_type = ConfigType::Json;
        } else if path.ends_with(".yaml") || path.ends_with(".yml") {
            config_type = ConfigType::Yaml;
        }
    
        let processed_content = delete_ignore_line(&self.content);
    
        if config_type == ConfigType::Unknown {
            config_type = Self::detect_format(&processed_content)?;
        }
    
        let conf = Config::from(path, processed_content.clone(), config_type.clone())?;
        Ok(conf)
    }

    fn detect_format(content: &str) -> Result<ConfigType, ConfigError> {
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
}
