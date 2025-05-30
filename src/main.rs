use anyhow::Result;
use clap::Parser;
use config_manager::command::{Command, Subcommand};
use config_manager::handler::handle_validate;
use config_manager::{delete_ignore_line, init_tracing, read_file};
use tracing::debug;

fn main() -> Result<()> {
    init_tracing();

    let command = Command::parse();

    match command.subcommand {
        Subcommand::Validate { file } => {
            debug!("validate: {}", file);
            handle_validate(file)?;
        }
        Subcommand::Show { file } => {
            debug!("show: {}", file);
            let content = read_file(&file)?;
            debug!("content: \n{}", delete_ignore_line(&content));
        }
        Subcommand::Convert { input, output } => {
            debug!("convert: {} -> {}", input, output);
        }
    }
    Ok(())
}
