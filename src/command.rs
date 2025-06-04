#[derive(Debug, clap::Parser)]
pub struct Command {
    #[clap(subcommand)]
    pub subcommand: Subcommand,
}

#[derive(Debug, clap::Subcommand)]
pub enum Subcommand {
    #[clap(name = "validate")]
    Validate {
        file: String,
        #[clap(short, long, default_value = "")]
        validate_file: String,
    },

    #[clap(name = "show")]
    Show {
        file: String,
        #[clap(short, long, default_value = "")]
        get: String,
        #[clap(short, long, default_value = "5")]
        deepth: usize,
    },

    #[clap(name = "convert")]
    Convert { input: String, output: String },

    #[clap(name = "template")]
    Template {
        template: String,
        #[clap(short, long, default_value = "toml")]
        format: String,
    },
}
