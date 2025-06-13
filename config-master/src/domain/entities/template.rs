use std::fmt::Display;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TemplateType {
    Database,
    Redis,
    WebServer,
    Logger,
    Monitor,
    Unknown,
}

impl Display for TemplateType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            TemplateType::Database => "database",
            TemplateType::Redis => "redis",
            TemplateType::WebServer => "webserver",
            TemplateType::Logger => "logger",
            TemplateType::Monitor => "monitor",
            TemplateType::Unknown => "unknown",
        };
        write!(f, "{}", text)
    }
}

impl From<String> for TemplateType {
    fn from(value: String) -> Self {
        match value.as_str() {
            "database" => TemplateType::Database,
            "redis" => TemplateType::Redis,
            "webserver" => TemplateType::WebServer,
            "logger" => TemplateType::Logger,
            "monitor" => TemplateType::Monitor,
            _ => TemplateType::Unknown,
        }
    }
}
