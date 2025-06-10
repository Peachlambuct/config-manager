use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::{
    fs::{File, OpenOptions},
    io::{AsyncWriteExt, BufWriter},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct Log {
    pub level: String,
    pub message: String,
    pub timestamp: DateTime<Utc>,
}

pub struct LogConfig {
    pub file: String,
    pub level: String,
}

pub struct LogManager {
    pub config: LogConfig,
    pub writer: BufWriter<File>,
}

impl LogManager {
    pub async fn new(config: LogConfig) -> Self {
        let file_path = Path::new(&config.file);
        if !file_path.exists() {
            File::create(file_path).await.unwrap();
        }
        let file = OpenOptions::new().append(true).open(file_path).await.unwrap();
        let writer = BufWriter::new(file);
        Self { config, writer }
    }

    pub async fn log_info(&mut self, message: String) {
        let log = Log {
            level: "info".to_string(),
            message,
            timestamp: Utc::now(),
        };
        self.write_log_by_level(log).await;
    }

    pub async fn log_error(&mut self, message: String) {
        let log = Log {
            level: "error".to_string(),
            message,
            timestamp: Utc::now(),
        };
        self.write_log_by_level(log).await;
    }

    pub async fn log_debug(&mut self, message: String) {
        let log = Log {
            level: "debug".to_string(),
            message,
            timestamp: Utc::now(),
        };
        self.write_log_by_level(log).await;
    }

    pub async fn log_warn(&mut self, message: String) {

        let log = Log {
            level: "warn".to_string(),
            message,
            timestamp: Utc::now(),
        };
        self.write_log_by_level(log).await;
    }

    pub async fn write_log_by_level(&mut self, log: Log) {
        if self.config.level == "info" {
            if log.level == "info" || log.level == "debug" || log.level == "warn" {
                self.write_log(log).await;
            }
        } else if self.config.level == "error" {
            if log.level == "error" {
                self.write_log(log).await;
            }
        } else if self.config.level == "debug" {
            if log.level == "debug"
                || log.level == "info"
                || log.level == "warn"
                || log.level == "error"
            {
                self.write_log(log).await;
            }
        } else if self.config.level == "warn" {
            if log.level == "warn" || log.level == "error" {
                self.write_log(log).await;
            }
        }
    }

    pub async fn write_log(&mut self, log: Log) {
        let log_str = format!("[{}]:[{}]:{}\n", log.timestamp.format("%Y-%m-%d %H:%M:%S"), log.level.to_uppercase(), log.message);
        self.writer.write_all(log_str.as_bytes()).await.unwrap();
        self.writer.flush().await.unwrap();
    }
}
