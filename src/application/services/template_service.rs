use crate::{
    domain::{
        entities::{configuration::Config, template::TemplateType},
        value_objects::config_format::ConfigType,
    },
    shared::error::ConfigError,
};

pub struct TemplateService;

impl TemplateService {
    pub fn write_template(template: TemplateType, format: String) -> Result<(), ConfigError> {
        let format = format.trim().to_lowercase();
        if format.is_empty() {
            return Err(ConfigError::UnsupportedFormat {
                format: "无法从文件扩展名识别目标格式".to_string(),
            });
        }

        let format = ConfigType::from(format.as_str());
        if format == ConfigType::Unknown {
            return Err(ConfigError::UnsupportedFormat {
                format: "无法从文件扩展名识别目标格式".to_string(),
            });
        }

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
