use std::collections::HashMap;

use crate::domain::entities::configuration::{Config, ConfigValue};
use crate::shared::error::ConfigError;

pub struct EnvOverrideService;

impl EnvOverrideService {
    pub fn apply_env_override(config: &mut Config) -> Result<Config, ConfigError> {
        Self::get_env_override_config(config)
    }

    pub fn get_env_override_config(config: &mut Config) -> Result<Config, ConfigError> {
        let envs = Self::get_envs();
        for (key, value) in envs {
            // 将环境变量键转换为路径 (例如: DATABASE_HOST -> database.host)
            let path = Self::env_key_to_path(&key)?;
            let config_value = ConfigValue::from_string(value);
            Self::set_by_path_recursive(&mut config.config, &path, config_value)?;
        }
        Ok(config.clone())
    }

    pub fn get_envs() -> HashMap<String, String> {
        std::env::vars()
            .filter(|(key, _)| key.starts_with("APP_"))
            .map(|(key, value)| (key.to_string().replace("APP_", ""), value.to_string()))
            .collect::<HashMap<String, String>>()
    }

    // 将环境变量键转换为配置路径
    // 例如: DATABASE_HOST -> ["database", "host"]
    fn env_key_to_path(env_key: &str) -> Result<Vec<String>, ConfigError> {
        if env_key.is_empty() {
            return Err(ConfigError::InvalidEnvVar {
                env_var: env_key.to_string(),
            });
        }

        let path: Vec<String> = env_key
            .to_lowercase()
            .split('_')
            .map(|s| s.to_string())
            .collect();

        if path.is_empty() {
            return Err(ConfigError::InvalidEnvVar {
                env_var: env_key.to_string(),
            });
        }

        Ok(path)
    }

    fn set_by_path_recursive(
        config: &mut HashMap<String, ConfigValue>,
        path: &[String],
        value: ConfigValue,
    ) -> Result<(), ConfigError> {
        if path.is_empty() {
            return Err(ConfigError::InvalidPath);
        }

        if path.len() == 1 {
            // 基础情况：直接设置值
            config.insert(path[0].clone(), value);
            return Ok(());
        }

        // 递归情况：需要进入下一层
        let key = &path[0];
        let remaining_path = &path[1..];

        // 确保当前键存在且是Object类型
        if !config.contains_key(key) {
            config.insert(key.clone(), ConfigValue::Object(HashMap::new()));
        }

        // 获取可变引用并递归
        let current_value = config.get_mut(key).unwrap();
        match current_value {
            ConfigValue::Object(obj) => Self::set_by_path_recursive(obj, remaining_path, value),
            _ => {
                // 如果不是Object类型，需要替换为Object
                *current_value = ConfigValue::Object(HashMap::new());
                if let ConfigValue::Object(obj) = current_value {
                    Self::set_by_path_recursive(obj, remaining_path, value)
                } else {
                    unreachable!()
                }
            }
        }
    }
}
