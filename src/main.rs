use anyhow::Result;
use clap::Parser;
use colored::{Color, Colorize};
use config_manager::command::{Command, Subcommand};
use config_manager::handler::{
    get_validation_by_config, handle_convert, handle_get, handle_http, handle_serve, handle_show,
    handle_template, handle_validate, handle_validate_by_validation_file,
};

use config_manager::model::log::{LogConfig, LogManager};
use config_manager::model::template::TemplateType;
use config_manager::{init_tracing, read_file};
use tracing::debug;

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let log_manager = LogManager::new(LogConfig {
        file: "test.log".to_string(),
        level: "info".to_string(),
    })
    .await;

    let command = Command::parse();

    match command.subcommand {
        Subcommand::Validate {
            file,
            validate_file,
        } => {
            if validate_file.is_empty() {
                debug!("validate: {}", file);
                let content = read_file(&file)?;
                let config = handle_validate(file, content)?;
                println!(
                    "config validate success, file format is {}",
                    (config.config_type).to_string().color(Color::Green)
                );
            } else {
                debug!("validate: {}", validate_file);
                let validation_content = read_file(&validate_file)?;
                let validation_config = handle_validate(validate_file, validation_content)?;
                let validation = get_validation_by_config(&validation_config).unwrap();
                let content = read_file(&file)?;
                let config = handle_validate(file.clone(), content)?;
                let config_type = config.config_type.clone();
                debug!("config: {:?}", config);
                let validation_result = handle_validate_by_validation_file(validation, config);
                if !validation_result.is_valid {
                    println!(
                        "{} config validate failed: {:?}",
                        file.color(Color::Red),
                        validation_result.errors
                    );
                } else {
                    println!(
                        "{} config validate success, file format is {}",
                        file.color(Color::Green),
                        config_type.to_string().color(Color::Green)
                    );
                }
            }
        }
        Subcommand::Show { file, get, deepth } => {
            if get.is_empty() {
                handle_show(file, deepth)?;
            } else {
                handle_get(file, get)?;
            }
        }
        Subcommand::Convert { input, output } => {
            debug!("convert: {} -> {}", input, output);
            handle_convert(input, output)?;
        }
        Subcommand::Template { template, format } => {
            debug!("template: {} {}", template, format);
            handle_template(TemplateType::from(template), format)?;
        }
        Subcommand::Serve {
            port,
            host,
            config_path,
            http,
        } => {
            if http {
                // HTTP 模式需要先创建 AppState
                use config_manager::model::app::AppState;
                use std::sync::{Arc, Mutex};

                let app_state = AppState::new(port, host.clone(), config_path);
                let app_state = Arc::new(Mutex::new(app_state));

                handle_http(port, host, app_state, log_manager).await?;
            } else {
                handle_serve(port, host, config_path, log_manager).await?;
            }
        }
    }
    Ok(())
}
