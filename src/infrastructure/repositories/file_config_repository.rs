use std::path::Path;

use async_trait::async_trait;

use crate::{
    domain::{
        entities::configuration::Config,
        repositories::configuration_repository::ConfigurationRepository,
        services::format_converter::FormatConverterService,
        value_objects::{config_format::ConfigType, config_path::ConfigPath},
    },
    shared::{error::ConfigError, utils::read_file},
};

pub struct FileConfigRepository {
    pub config_path: String,
}

impl FileConfigRepository {
    pub fn new(config_path: String) -> Self {
        if !Path::new(&config_path).exists() {
            std::fs::create_dir_all(config_path.clone()).unwrap();
        }

        Self { config_path }
    }

    pub fn save(&self, config: Config) -> Result<(), ConfigError> {
        // 转换为serde_json::Value以避免类型标签
        let serde_value = config.to_serde_value();

        let converted_content = match config.config_type {
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
        std::fs::write(&self.config_path, converted_content)
            .map_err(|e| ConfigError::IoError(e))?;
        Ok(())
    }

    pub fn get_config_save_path(&self, config_name: &str) -> String {
        format!("{}/{}", self.config_path, config_name)
    }
}

#[async_trait]
impl ConfigurationRepository for FileConfigRepository {
    async fn save(&self, config: Config) -> Result<(), ConfigError> {
        self.save(config)
    }

    async fn get(&self, path: String) -> Result<Config, ConfigError> {
        let content = read_file(&path)?;
        let config = FormatConverterService::new(ConfigPath::new(path).unwrap(), content)
            .validate_config()?;
        Ok(config)
    }

    async fn get_all(&self) -> Result<Vec<Config>, ConfigError> {
        Err(ConfigError::NowRepositoryConfigNotSupportFunction)
    }
    async fn delete(&self, _path: String) -> Result<(), ConfigError> {
        Err(ConfigError::NowRepositoryConfigNotSupportFunction)
    }
    async fn update(&self, _config: Config, _path: String) -> Result<(), ConfigError> {
        Err(ConfigError::NowRepositoryConfigNotSupportFunction)
    }
}
