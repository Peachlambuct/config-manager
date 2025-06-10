use anyhow::Result;
use clap::Parser;
use colored::{Color, Colorize};
use config_manager::domain::value_objects::config_path::ConfigPath;
use config_manager::interfaces::cli::command::{Command, Subcommand};

use config_manager::application::services::configuration_service::ConfigurationService;
use config_manager::application::services::template_service::TemplateService;
use config_manager::application::services::validation_service::ValidationService;
use config_manager::domain::entities::template::TemplateType;
use config_manager::domain::services::config_validation::ConfigValidationService;
use config_manager::domain::services::format_converter::FormatConverterService;
use config_manager::infrastructure::logging::log_manager::{LogConfig, LogManager};
use config_manager::interfaces::http::server::handle_http;
use config_manager::interfaces::tcp::server::handle_serve;
use config_manager::shared::utils::{init_tracing, read_file};
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
                let config = FormatConverterService::new(ConfigPath::new(file, true).unwrap(), content)
                    .validate_config()?;
                println!(
                    "config validate success, file format is {}",
                    (config.config_type).to_string().color(Color::Green)
                );
            } else {
                debug!("validate: {}", validate_file);
                let validation_content = read_file(&validate_file)?;
                let validation_config = FormatConverterService::new(
                    ConfigPath::new(validate_file, true).unwrap(),
                    validation_content,
                )
                .validate_config()?;
                let validation = ValidationService::get_validation_by_config(&validation_config)?;
                let content = read_file(&file)?;
                let config =
                    FormatConverterService::new(ConfigPath::new(file.clone(), true).unwrap(), content)
                        .validate_config()?;
                let config_type = config.config_type.clone();
                debug!("config: {:?}", config);
                let validation_result =
                    ConfigValidationService::validate_with_rules(validation, config);
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
                ConfigurationService::display_configuration(file, deepth)?;
            } else {
                ConfigurationService::get_configuration_value(file, get)?;
            }
        }
        Subcommand::Convert { input, output } => {
            debug!("convert: {} -> {}", input, output);
            ConfigurationService::convert_configuration(input, output)?;
        }
        Subcommand::Template { template, format } => {
            debug!("template: {} {}", template, format);
            TemplateService::write_template(TemplateType::from(template), format)?;
        }
        Subcommand::Serve {
            port,
            host,
            config_path,
            http,
        } => {
            if http {
                // HTTP 模式需要先创建 AppState
                use config_manager::shared::app_state::AppState;
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
