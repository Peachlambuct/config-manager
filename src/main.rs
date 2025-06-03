use anyhow::Result;
use clap::Parser;
use config_manager::command::{Command, Subcommand};
use config_manager::handler::{handle_convert, handle_get, handle_show, handle_validate};
use config_manager::{init_tracing, read_file};
use tracing::debug;

fn main() -> Result<()> {
    init_tracing();

    let command = Command::parse();

    match command.subcommand {
        Subcommand::Validate { file } => {
            debug!("validate: {}", file);
            let content = read_file(&file)?;
            handle_validate(file, content)?;
        }
        Subcommand::Show { file, get } => {
            if get.is_empty() {
                handle_show(file)?;
            } else {
                handle_get(file, get)?;
            }
        }
        Subcommand::Convert { input, output } => {
            debug!("convert: {} -> {}", input, output);
            handle_convert(input, output)?;
        }
    }
    Ok(())
}
