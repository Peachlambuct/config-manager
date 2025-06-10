use crate::{domain::entities::configuration::Config, shared::error::ConfigError};
use async_trait::async_trait;

#[async_trait]
pub trait ConfigurationRepository {
    async fn save(&self, config: Config) -> Result<(), ConfigError>;
    async fn get(&self, path: String) -> Result<Config, ConfigError>;
    async fn get_all(&self) -> Result<Vec<Config>, ConfigError>;
    async fn delete(&self, path: String) -> Result<(), ConfigError>;
    async fn update(&self, config: Config, path: String) -> Result<(), ConfigError>;
}
