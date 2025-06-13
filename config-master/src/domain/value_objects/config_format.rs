use std::fmt::Display;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ConfigType {
    Yaml,
    Json,
    Toml,
    Unknown,
}

impl From<&str> for ConfigType {
    fn from(s: &str) -> Self {
        let format = match s {
            "json" => ConfigType::Json,
            "yaml" => ConfigType::Yaml,
            "toml" => ConfigType::Toml,
            _ => ConfigType::Unknown,
        };
        format
    }
}

impl Display for ConfigType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}