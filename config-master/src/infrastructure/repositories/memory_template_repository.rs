use async_trait::async_trait;

use crate::{
    domain::{
        entities::{configuration::Config, template::TemplateType},
        repositories::template_repository::TemplateRepository,
        value_objects::config_format::ConfigType,
    },
    shared::error::TemplateError,
};

pub struct MemoryTemplateRepository;

impl MemoryTemplateRepository {
    pub fn new() -> Self {
        Self {}
    }

    pub fn get_supported_templates() -> Vec<TemplateType> {
        vec![
            TemplateType::Database,
            TemplateType::Redis,
            TemplateType::WebServer,
        ]
    }
}

#[async_trait]
impl TemplateRepository for MemoryTemplateRepository {
    async fn get(&self, _path: String) -> Result<TemplateType, TemplateError> {
        Err(TemplateError::NowRepositoryTemplateNotSupportFunction)
    }

    async fn get_all(&self) -> Result<Vec<TemplateType>, TemplateError> {
        Ok(Self::get_supported_templates())
    }

    async fn save(&self, _template: TemplateType, _path: String) -> Result<(), TemplateError> {
        Err(TemplateError::NowRepositoryTemplateNotSupportFunction)
    }

    async fn delete(&self, _path: String) -> Result<(), TemplateError> {
        Err(TemplateError::NowRepositoryTemplateNotSupportFunction)
    }

    async fn update(&self, _template: TemplateType, _path: String) -> Result<(), TemplateError> {
        Err(TemplateError::NowRepositoryTemplateNotSupportFunction)
    }

    async fn get_default_template(
        &self,
        template: TemplateType,
    ) -> Result<TemplateType, TemplateError> {
        Ok(template)
    }

    async fn write_template_by_type_and_format(
        &self,
        template: TemplateType,
        format: String,
    ) -> Result<(), TemplateError> {
        let format = format.trim().to_lowercase();
        if format.is_empty() {
            return Err(TemplateError::UnsupportedFormat {
                format: "not a valid config file".to_string(),
            });
        }

        let format = ConfigType::from(format.as_str());
        if format == ConfigType::Unknown {
            return Err(TemplateError::UnsupportedFormat {
                format: "not a valid config file".to_string(),
            });
        }

        let config =
            Config::get_default_config(template.clone(), format.clone()).map_err(|_| {
                TemplateError::UnsupportedFormat {
                    format: "not a valid config file".to_string(),
                }
            })?;
        config.show(".", 5);
        let serde_value = config.to_serde_value();

        let converted_content = match format {
            ConfigType::Json => serde_json::to_string_pretty(&serde_value)
                .map_err(|_| TemplateError::ParseTemplateError)?,
            ConfigType::Yaml => serde_yaml::to_string(&serde_value)
                .map_err(|_| TemplateError::ParseTemplateError)?,
            ConfigType::Toml => {
                // TOML需要特殊处理，因为它不支持所有JSON类型
                toml::to_string_pretty(&serde_value)
                    .map_err(|_| TemplateError::ParseTemplateError)?
            }
            ConfigType::Unknown => {
                return Err(TemplateError::UnknownConfigType);
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
        std::fs::write(&output, converted_content).map_err(|e| TemplateError::IoError(e))?;

        println!("✅ template file generated: {}", output);

        Ok(())
    }
}
