use thiserror::Error;
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config parse error")]
    ParseConfigError,
    #[error("io error")]
    IoError(#[from] std::io::Error),
    #[error("empty line")]
    EmptyLine,
    #[error("invalid file extension")]
    InvalidFileExtension,
    #[error("empty path")]
    EmptyPath,
    #[error("unsupported config type, we only support json, yaml, toml")]
    UnknownConfigType,
    #[error("empty content")]
    EmptyContent,
    #[error("unsupported format")]
    UnsupportedFormat { format: String },
    #[error("key not found")]
    KeyNotFound,
}
