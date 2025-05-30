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
    #[error("unknown config type")]
    UnknownConfigType,
    #[error("empty content")]
    EmptyContent,
}
