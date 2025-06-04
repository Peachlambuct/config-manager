use anyhow::Result;
use clap::Parser;
use colored::{Color, Colorize};
use config_manager::command::{Command, Subcommand};
use config_manager::handler::{
    get_validation_by_config, handle_convert, handle_get, handle_show, handle_template,
    handle_validate, handle_validate_by_validation_file,
};
use config_manager::model::template::TemplateType;
use config_manager::{init_tracing, read_file};
use tracing::debug;

fn main() -> Result<()> {
    init_tracing();

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
                    "配置文件验证成功, 文件格式为 {}",
                    (config.config_type).to_string().color(Color::Green)
                );
            } else {
                debug!("validate: {}", validate_file);
                let validation_content = read_file(&validate_file)?;
                let validation_config = handle_validate(validate_file, validation_content)?;
                let validation = get_validation_by_config(&validation_config).unwrap();
                let content = read_file(&file)?;
                let config = handle_validate(file, content)?;
                let config_type = config.config_type.clone();
                debug!("config: {:?}", config);
                let validation_result = handle_validate_by_validation_file(validation, config);
                if !validation_result.is_valid {
                    println!("配置文件验证失败: {:?}", validation_result.errors);
                } else {
                    println!(
                        "配置文件验证成功, 文件格式为 {}",
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
    }
    Ok(())
}
