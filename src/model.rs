use crate::error::ConfigError;
use serde_json::Number;
use std::{collections::HashMap, fmt::Display};

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

    pub fn from(path: String, config_str: String, config_type: ConfigType) -> Result<Self, ConfigError> {
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

    pub fn show(&self, path: &str) {
        println!("📄 配置文件: {} ({}格式)", path, self.config_type);
        println!("📊 配置项数量: {}\n", self.config.len());
        println!("🔧 配置内容:");
        
        let keys: Vec<&String> = self.config.keys().collect();
        for (index, key) in keys.iter().enumerate() {
            let is_last = index == keys.len() - 1;
            let value = &self.config[*key];
            Self::display_config_value(key, value, 0, is_last);
        }
    }

    // 递归辅助函数，处理ConfigValue的显示
    pub fn display_config_value(key: &str, value: &ConfigValue, indent_level: usize, is_last: bool) {
        let prefix = Self::get_tree_prefix(indent_level, is_last);
        
        match value {
            ConfigValue::String(s) => {
                println!("{}{}: \"{}\" (String)", prefix, key, s);
            }
            ConfigValue::Number(n) => {
                println!("{}{}: {} (Number)", prefix, key, n);
            }
            ConfigValue::Boolean(b) => {
                println!("{}{}: {} (Boolean)", prefix, key, b);
            }
            ConfigValue::Null => {
                println!("{}{}: null", prefix, key);
            }
            ConfigValue::Array(arr) => {
                println!("{}{}: (Array[{}])", prefix, key, arr.len());
                for (index, item) in arr.iter().enumerate() {
                    let item_is_last = index == arr.len() - 1;
                    let item_key = format!("[{}]", index);
                    Self::display_config_value(&item_key, item, indent_level + 1, item_is_last);
                }
            }
            ConfigValue::Object(obj) => {
                println!("{}{}: (Object)", prefix, key);
                let obj_keys: Vec<&String> = obj.keys().collect();
                for (index, obj_key) in obj_keys.iter().enumerate() {
                    let obj_is_last = index == obj_keys.len() - 1;
                    let obj_value = &obj[*obj_key];
                    Self::display_config_value(obj_key, obj_value, indent_level + 1, obj_is_last);
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
                let serde_arr: Vec<serde_json::Value> = arr
                    .iter()
                    .map(|v| v.to_serde_value())
                    .collect();
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
}
