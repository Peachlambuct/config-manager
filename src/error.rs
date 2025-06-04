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
    #[error("unsupported template type")]
    UnsupportedTemplateType,
    #[error("invalid path")]
    InvalidPath,
    #[error("environment variable format error: {env_var}")]
    InvalidEnvVar { env_var: String },
}

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("field {field} is required")]
    RequiredField { field: String },
    #[error("field {field} type mismatch, expected {expected}, actual {actual}")]
    TypeMismatch {
        field: String,
        expected: String,
        actual: String,
    },
    #[error("field {field} value does not satisfy custom rule")]
    CustomRuleViolation { field: String, rule: String },
    #[error("field {field} is not defined")]
    UndefinedField { field: String },
}
