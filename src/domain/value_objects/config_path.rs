use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::shared::error::ConfigError;

#[derive(Debug, Eq, Hash, PartialEq, Clone, Serialize, Deserialize)]
pub struct ConfigPath {
    inner: String,
}

impl ConfigPath {
    pub fn new(inner: impl Into<String>) -> Result<Self, ConfigError> {
        let path: String = inner.into();

        if path.is_empty() {
            return Err(ConfigError::InvalidConfigPath(path.clone()));
        }

        if !Self::is_config_file(&path) {
            return Err(ConfigError::UnsupportedFormat {
                format: "not a valid config file".to_string(),
            });
        }

        Ok(Self { inner: path })
    }

    pub fn as_str(&self) -> &str {
        &self.inner
    }

    pub fn as_string(&self) -> String {
        self.inner.clone()
    }

    fn is_config_file(path: &str) -> bool {
        let path = Path::new(path);
        path.extension().map_or(false, |ext| {
            ext == "json" || ext == "yaml" || ext == "toml" || ext == "yml"
        })
    }
}
