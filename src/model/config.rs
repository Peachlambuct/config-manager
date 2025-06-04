use std::{collections::HashMap, fmt::Display};

use colored::{Color, Colorize};
use serde_json::Number;

use crate::error::ConfigError;

use super::template::TemplateType;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ConfigType {
    Yaml,
    Json,
    Toml,
    Unknown,
}

impl Display for ConfigType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Config {
    pub path: String,
    pub config: HashMap<String, ConfigValue>,
    pub config_type: ConfigType,
}

impl Config {
    pub fn new() -> Self {
        Self {
            path: String::new(),
            config: HashMap::new(),
            config_type: ConfigType::Unknown,
        }
    }

    pub fn from(
        path: String,
        config_str: String,
        config_type: ConfigType,
    ) -> Result<Self, ConfigError> {
        let config_map = match config_type {
            ConfigType::Json => {
                let json_value: serde_json::Value =
                    serde_json::from_str(&config_str).map_err(|_| ConfigError::ParseConfigError)?;
                ConfigValue::from_serde_json(json_value)?.into_object()?
            }
            ConfigType::Yaml => {
                let yaml_value: serde_yaml::Value =
                    serde_yaml::from_str(&config_str).map_err(|_| ConfigError::ParseConfigError)?;
                let json_value =
                    serde_json::to_value(yaml_value).map_err(|_| ConfigError::ParseConfigError)?;
                ConfigValue::from_serde_json(json_value)?.into_object()?
            }
            ConfigType::Toml => {
                let toml_value: toml::Value =
                    toml::from_str(&config_str).map_err(|_| ConfigError::ParseConfigError)?;
                let json_value =
                    serde_json::to_value(toml_value).map_err(|_| ConfigError::ParseConfigError)?;
                ConfigValue::from_serde_json(json_value)?.into_object()?
            }
            ConfigType::Unknown => {
                return Err(ConfigError::UnsupportedFormat {
                    format: config_type.to_string(),
                });
            }
        };

        Ok(Self {
            path: path.clone(),
            config: config_map,
            config_type,
        })
    }

    pub fn get(&self, key: &str) -> Option<ConfigValue> {
        let keys: Vec<&str> = key.split(".").collect();
        let mut current_config = &self.config;
        let mut current = &ConfigValue::Null;
        for k in keys {
            current = current_config.get(k)?;
            if let ConfigValue::Object(obj) = current {
                current_config = &obj;
            }
        }
        Some(current.clone())
    }

    pub fn show(&self, path: &str, print_deepth: usize) {
        println!(
            "📄 配置文件: {} ({}格式)",
            path.blue(),
            self.config_type.to_string().color(Color::Yellow)
        );
        println!("📊 配置项数量: {}\n", self.config.len());
        println!("🔧 配置内容:");

        let keys: Vec<&String> = self.config.keys().collect();
        for (index, key) in keys.iter().enumerate() {
            let is_last = index == keys.len() - 1;
            let value = &self.config[*key];
            Self::display_config_value(key, value, 0, is_last, print_deepth);
        }
    }

    // 递归辅助函数，处理ConfigValue的显示
    pub fn display_config_value(
        key: &str,
        value: &ConfigValue,
        indent_level: usize,
        is_last: bool,
        print_deepth: usize,
    ) {
        let prefix = Self::get_tree_prefix(indent_level, is_last);

        match value {
            ConfigValue::String(s) => {
                println!(
                    "{}{}: \"{}\" (String)",
                    prefix,
                    key.to_string().blue(),
                    s.to_string().green()
                );
            }
            ConfigValue::Number(n) => {
                println!(
                    "{}{}: {} (Number)",
                    prefix,
                    key.to_string().blue(),
                    n.to_string().green()
                );
            }
            ConfigValue::Boolean(b) => {
                println!(
                    "{}{}: {} (Boolean)",
                    prefix,
                    key.to_string().blue(),
                    b.to_string().purple()
                );
            }
            ConfigValue::Null => {
                println!("{}{}: {}", prefix, key.to_string().blue(), "null".red());
            }
            ConfigValue::Array(arr) => {
                if print_deepth > indent_level {
                    println!(
                        "{}{}: (Array[{}])",
                        prefix,
                        key.to_string().blue(),
                        arr.len()
                    );
                    for (index, item) in arr.iter().enumerate() {
                        let item_is_last = index == arr.len() - 1;
                        let item_key = format!("[{}]", index.to_string().yellow());
                        Self::display_config_value(
                            &item_key,
                            item,
                            indent_level + 1,
                            item_is_last,
                            print_deepth,
                        );
                    }
                } else {
                    println!("{}... (Array[{}])", prefix, arr.len());
                }
            }
            ConfigValue::Object(obj) => {
                if print_deepth > indent_level {
                    println!("{}{}: (Object)", prefix, key.to_string().blue());
                    let obj_keys: Vec<&String> = obj.keys().collect();
                    for (index, obj_key) in obj_keys.iter().enumerate() {
                        let obj_is_last = index == obj_keys.len() - 1;
                        let obj_value = &obj[*obj_key];
                        Self::display_config_value(
                            obj_key,
                            obj_value,
                            indent_level + 1,
                            obj_is_last,
                            print_deepth,
                        );
                    }
                } else {
                    println!("{}{}: (Object)", prefix, key.to_string().blue());
                }
            }
        }
    }

    // 生成树状结构的前缀
    fn get_tree_prefix(indent_level: usize, is_last: bool) -> String {
        let mut prefix = String::new();

        // 添加缩进
        for _ in 0..indent_level {
            prefix.push_str("│  ");
        }

        // 添加树状字符
        if is_last {
            prefix.push_str("└─ ");
        } else {
            prefix.push_str("├─ ");
        }

        prefix
    }

    // 将整个配置转换为serde_json::Value（用于序列化）
    pub fn to_serde_value(&self) -> serde_json::Value {
        let mut serde_obj = serde_json::Map::new();
        for (key, value) in &self.config {
            serde_obj.insert(key.clone(), value.to_serde_value());
        }
        serde_json::Value::Object(serde_obj)
    }

    pub fn get_default_config(
        template: TemplateType,
        format: ConfigType,
    ) -> Result<Self, ConfigError> {
        match template {
            TemplateType::Database => Ok(Self::get_default_database_config(format)),
            TemplateType::Redis => Ok(Self::get_default_redis_config(format)),
            TemplateType::WebServer => Ok(Self::get_default_webserver_config(format)),
            TemplateType::Logger => Ok(Self::get_default_logger_config(format)),
            TemplateType::Monitor => Ok(Self::get_default_monitor_config(format)),
            TemplateType::Unknown => {
                return Err(ConfigError::UnsupportedTemplateType);
            }
        }
    }

    pub fn get_default_database_config(config_type: ConfigType) -> Self {
        let mut config = Self::new();
        let mut database_config = HashMap::new();
        database_config.insert(
            "host".to_string(),
            ConfigValue::String("localhost".to_string()),
        );
        database_config.insert("port".to_string(), ConfigValue::Number(Number::from(3306)));
        database_config.insert(
            "username".to_string(),
            ConfigValue::String("root".to_string()),
        );
        database_config.insert(
            "password".to_string(),
            ConfigValue::String("password".to_string()),
        );
        let database_config = ConfigValue::Object(database_config);
        config
            .config
            .insert("database".to_string(), database_config);
        config.config_type = config_type;
        config
    }

    pub fn get_default_redis_config(config_type: ConfigType) -> Self {
        let mut config = Self::new();
        let mut redis_config = HashMap::new();
        redis_config.insert(
            "host".to_string(),
            ConfigValue::String("localhost".to_string()),
        );
        redis_config.insert("port".to_string(), ConfigValue::Number(Number::from(6379)));
        redis_config.insert(
            "password".to_string(),
            ConfigValue::String("password".to_string()),
        );
        let redis_config = ConfigValue::Object(redis_config);
        config.config.insert("redis".to_string(), redis_config);
        config.config_type = config_type;
        config
    }

    pub fn get_default_webserver_config(config_type: ConfigType) -> Self {
        let mut config = Self::new();
        let mut webserver_config = HashMap::new();
        webserver_config.insert(
            "host".to_string(),
            ConfigValue::String("localhost".to_string()),
        );
        webserver_config.insert("port".to_string(), ConfigValue::Number(Number::from(8080)));
        let webserver_config = ConfigValue::Object(webserver_config);
        config
            .config
            .insert("webserver".to_string(), webserver_config);
        config.config_type = config_type;
        config
    }

    pub fn get_default_logger_config(config_type: ConfigType) -> Self {
        let mut config = Self::new();
        let mut logger_config = HashMap::new();
        logger_config.insert("level".to_string(), ConfigValue::String("info".to_string()));
        let logger_config = ConfigValue::Object(logger_config);
        config.config.insert("logger".to_string(), logger_config);
        config.config_type = config_type;
        config
    }

    pub fn get_default_monitor_config(config_type: ConfigType) -> Self {
        let mut config = Self::new();
        let mut monitor_config = HashMap::new();
        monitor_config.insert(
            "host".to_string(),
            ConfigValue::String("localhost".to_string()),
        );
        monitor_config.insert("port".to_string(), ConfigValue::Number(Number::from(9090)));
        let monitor_config = ConfigValue::Object(monitor_config);
        config.config.insert("monitor".to_string(), monitor_config);
        config.config_type = config_type;
        config
    }

    pub fn get_env_override_config(&mut self) -> Result<Self, ConfigError> {
        let envs = Self::get_envs();
        for (key, value) in envs {
            // 将环境变量键转换为路径 (例如: DATABASE_HOST -> database.host)
            let path = Self::env_key_to_path(&key)?;
            let config_value = ConfigValue::from_string(value);
            Self::set_by_path_recursive(&mut self.config, &path, config_value)?;
        }
        Ok(self.clone())
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

    pub fn release_config(&mut self) -> Result<Config, ConfigError> {
        let config = self.get_env_override_config()?;

        Ok(config)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ConfigValue {
    Null,
    String(String),
    Number(Number),
    Boolean(bool),
    Array(Vec<ConfigValue>),
    Object(HashMap<String, ConfigValue>),
}

impl Display for ConfigValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl ConfigValue {
    // 从serde_json::Value转换为ConfigValue
    pub fn from_serde_json(value: serde_json::Value) -> Result<Self, ConfigError> {
        match value {
            serde_json::Value::Null => Ok(ConfigValue::Null),
            serde_json::Value::Bool(b) => Ok(ConfigValue::Boolean(b)),
            serde_json::Value::Number(n) => Ok(ConfigValue::Number(n)),
            serde_json::Value::String(s) => Ok(ConfigValue::String(s)),
            serde_json::Value::Array(arr) => {
                let config_arr: Result<Vec<ConfigValue>, ConfigError> = arr
                    .into_iter()
                    .map(|v| ConfigValue::from_serde_json(v))
                    .collect();
                Ok(ConfigValue::Array(config_arr?))
            }
            serde_json::Value::Object(obj) => {
                let mut config_obj = HashMap::new();
                for (key, value) in obj {
                    config_obj.insert(key, ConfigValue::from_serde_json(value)?);
                }
                Ok(ConfigValue::Object(config_obj))
            }
        }
    }

    // 将ConfigValue转换为serde_json::Value（用于序列化）
    pub fn to_serde_value(&self) -> serde_json::Value {
        match self {
            ConfigValue::Null => serde_json::Value::Null,
            ConfigValue::Boolean(b) => serde_json::Value::Bool(*b),
            ConfigValue::Number(n) => serde_json::Value::Number(n.clone()),
            ConfigValue::String(s) => serde_json::Value::String(s.clone()),
            ConfigValue::Array(arr) => {
                let serde_arr: Vec<serde_json::Value> =
                    arr.iter().map(|v| v.to_serde_value()).collect();
                serde_json::Value::Array(serde_arr)
            }
            ConfigValue::Object(obj) => {
                let mut serde_obj = serde_json::Map::new();
                for (key, value) in obj {
                    serde_obj.insert(key.clone(), value.to_serde_value());
                }
                serde_json::Value::Object(serde_obj)
            }
        }
    }

    // 辅助方法：将ConfigValue转换为HashMap（用于顶级对象）
    pub fn into_object(self) -> Result<HashMap<String, ConfigValue>, ConfigError> {
        match self {
            ConfigValue::Object(obj) => Ok(obj),
            _ => Err(ConfigError::ParseConfigError), // 配置文件顶级必须是对象
        }
    }

    // 简化的from方法，用于基础类型解析
    pub fn from_string(value: String) -> ConfigValue {
        // 尝试解析为不同类型
        if let Ok(num) = value.parse::<i32>() {
            ConfigValue::Number(Number::from(num))
        } else if let Ok(num) = value.parse::<f64>() {
            if let Some(num) = Number::from_f64(num) {
                ConfigValue::Number(num)
            } else {
                ConfigValue::String(value)
            }
        } else if let Ok(b) = value.parse::<bool>() {
            ConfigValue::Boolean(b)
        } else {
            ConfigValue::String(value)
        }
    }

    // 获取字符串长度（用于验证）
    pub fn len(&self) -> Option<usize> {
        match self {
            ConfigValue::String(s) => Some(s.len()),
            ConfigValue::Array(arr) => Some(arr.len()),
            ConfigValue::Object(obj) => Some(obj.len()),
            _ => None,
        }
    }

    // 检查是否为null
    pub fn is_null(&self) -> bool {
        matches!(self, ConfigValue::Null)
    }

    // 获取字符串值
    pub fn as_string(&self) -> Option<&String> {
        match self {
            ConfigValue::String(s) => Some(s),
            _ => None,
        }
    }

    // 获取数字值
    pub fn as_number(&self) -> Option<f64> {
        match self {
            ConfigValue::Number(n) => n.as_f64(),
            _ => None,
        }
    }

    // 获取布尔值
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            ConfigValue::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    // 获取数组
    pub fn as_array(&self) -> Option<&Vec<ConfigValue>> {
        match self {
            ConfigValue::Array(arr) => Some(arr),
            _ => None,
        }
    }

    // 获取对象
    pub fn as_object(&self) -> Option<&HashMap<String, ConfigValue>> {
        match self {
            ConfigValue::Object(obj) => Some(obj),
            _ => None,
        }
    }
}

// 实现PartialEq<&str>用于字符串比较
impl PartialEq<&str> for ConfigValue {
    fn eq(&self, other: &&str) -> bool {
        match self {
            ConfigValue::String(s) => s == *other,
            _ => false,
        }
    }
}

// 实现PartialEq<&f64>用于数字比较
impl PartialEq<&f64> for ConfigValue {
    fn eq(&self, other: &&f64) -> bool {
        match self {
            ConfigValue::Number(n) => n.as_f64() == Some(**other),
            _ => false,
        }
    }
}

// 实现PartialOrd<&f64>用于数字比较
impl PartialOrd<&f64> for ConfigValue {
    fn partial_cmp(&self, other: &&f64) -> Option<std::cmp::Ordering> {
        match self {
            ConfigValue::Number(n) => n.as_f64()?.partial_cmp(*other),
            _ => None,
        }
    }
}

/// 用于提供serve下的缓存
pub type ConfigMap = HashMap<String, Config>;
