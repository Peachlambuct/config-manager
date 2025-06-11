use crate::{
    domain::{
        entities::{configuration::Config, template::TemplateType},
        repositories::configuration_repository::ConfigurationRepository,
        value_objects::config_format::ConfigType,
    },
    shared::error::ConfigError,
};

pub struct ConfigurationService {
    pub config_repository: Box<dyn ConfigurationRepository>,
}

impl ConfigurationService {
    pub fn new(config_repository: Box<dyn ConfigurationRepository>) -> Self {
        Self { config_repository }
    }

    pub async fn display_configuration(
        &self,
        path: String,
        depth: usize,
    ) -> Result<(), ConfigError> {
        let config = self.config_repository.get(path.clone()).await?;
        config.show(&path, depth);
        Ok(())
    }

    pub async fn get_configuration_value(
        &self,
        path: String,
        key: String,
    ) -> Result<(), ConfigError> {
        let config = self.config_repository.get(path.clone()).await?;
        let value = config.get(&key);
        if let Some(value) = value {
            Config::display_config_value(&key, &value, 0, false, 0);
        } else {
            return Err(ConfigError::KeyNotFound);
        }
        Ok(())
    }

    pub async fn convert_configuration(
        &self,
        input: String,
        output: String,
    ) -> Result<(), ConfigError> {
        let config = self.config_repository.get(input.clone()).await?;

        // 检测目标格式
        let target_format = if output.ends_with(".json") {
            ConfigType::Json
        } else if output.ends_with(".yaml") || output.ends_with(".yml") {
            ConfigType::Yaml
        } else if output.ends_with(".toml") {
            ConfigType::Toml
        } else {
            return Err(ConfigError::UnsupportedFormat {
                format: "not a valid config file".to_string(),
            });
        };

        // 转换为serde_json::Value以避免类型标签
        let serde_value = config.to_serde_value();

        let converted_content = match target_format {
            ConfigType::Json => serde_json::to_string_pretty(&serde_value)
                .map_err(|_| ConfigError::ParseConfigError)?,
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
            "✅ convert success: {} ({:?}) -> {} ({:?})",
            input, config.config_type, output, target_format
        );

        Ok(())
    }

    pub async fn generate_template(
        &self,
        template: TemplateType,
        format: String,
    ) -> Result<(), ConfigError> {
        let format = format.trim().to_lowercase();
        if format.is_empty() {
            return Err(ConfigError::UnsupportedFormat {
                format: "not a valid config file".to_string(),
            });
        }
        let format = match format.as_str() {
            "json" => ConfigType::Json,
            "yaml" => ConfigType::Yaml,
            "toml" => ConfigType::Toml,
            _ => {
                return Err(ConfigError::UnsupportedFormat {
                    format: "not a valid config file".to_string(),
                });
            }
        };

        let config = Config::get_default_config(template.clone(), format.clone())?;
        config.show(".", 5);
        let serde_value = config.to_serde_value();

        let converted_content = match format {
            ConfigType::Json => serde_json::to_string_pretty(&serde_value)
                .map_err(|_| ConfigError::ParseConfigError)?,
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
        println!("🔧 generate config file: {}", converted_content);
        let format_ext = match format {
            ConfigType::Json => "json",
            ConfigType::Yaml => "yaml",
            ConfigType::Toml => "toml",
            ConfigType::Unknown => "txt",
        };
        let output = format!("{}-config.{}", template, format_ext);
        println!("📝 output file name: {}", output);
        // 写入目标文件
        std::fs::write(&output, converted_content).map_err(|e| ConfigError::IoError(e))?;

        println!("✅ template file generated: {}", output);

        Ok(())
    }
}
