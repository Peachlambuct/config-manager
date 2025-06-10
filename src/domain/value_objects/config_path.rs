use serde::{Deserialize, Serialize};

use crate::shared::error::ConfigError;

#[derive(Debug, Eq, Hash, PartialEq, Clone, Serialize, Deserialize)]
pub struct ConfigPath {
    inner: String,
}

impl ConfigPath {
    pub fn new(inner: impl Into<String>) -> Result<Self, ConfigError> {
        let path: String = inner.into();

        Ok(Self { inner: path })
    }

    pub fn as_str(&self) -> &str {
        &self.inner
    }

    pub fn as_string(&self) -> String {
        self.inner.clone()
    }
}
