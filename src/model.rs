#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigType {
    Yaml,
    Json,
    Toml,
    Unknown,
}
