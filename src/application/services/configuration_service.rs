use crate::{
    domain::{
        entities::{configuration::Config, template::TemplateType},
        services::format_converter::FormatConverterService,
        value_objects::config_format::ConfigType,
    },
    shared::{error::ConfigError, utils::read_file},
};

pub struct ConfigurationService;

impl ConfigurationService {
    pub fn display_configuration(path: String, depth: usize) -> Result<(), ConfigError> {
        let content = read_file(&path)?;
        let config = FormatConverterService::validate_config(path.clone(), content)?;
        config.show(&path, depth);
        Ok(())
    }

    pub fn get_configuration_value(path: String, key: String) -> Result<(), ConfigError> {
        let content = read_file(&path)?;
        let config = FormatConverterService::validate_config(path.clone(), content)?;
        let value = config.get(&key);
        if let Some(value) = value {
            Config::display_config_value(&key, &value, 0, false, 0);
        } else {
            return Err(ConfigError::KeyNotFound);
        }
        Ok(())
    }

    pub fn convert_configuration(input: String, output: String) -> Result<(), ConfigError> {
        let content = read_file(&input)?;
        let config = FormatConverterService::validate_config(input.clone(), content)?;

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
            "✅ 转换完成: {} ({:?}) -> {} ({:?})",
            input, config.config_type, output, target_format
        );

        Ok(())
    }

    pub fn generate_template(template: TemplateType, format: String) -> Result<(), ConfigError> {
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
}
