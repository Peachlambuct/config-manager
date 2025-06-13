use tracing_subscriber::fmt;

use crate::shared::error::ConfigError;

pub fn init_tracing() {
    let subscriber = fmt::Subscriber::builder()
        .with_max_level(tracing::Level::DEBUG)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set default subscriber");
}

pub fn read_file(path: &str) -> Result<String, ConfigError> {
    let content = std::fs::read_to_string(path).map_err(ConfigError::IoError)?;
    Ok(content)
}

pub fn delete_ignore_line(content: &str) -> String {
    content
        .lines()
        .filter(|line| {
            if line.contains("#") || line.is_empty() || line.trim().starts_with("---") {
                return false;
            }
            true
        })
        .collect::<Vec<&str>>()
        .join("\n")
}
