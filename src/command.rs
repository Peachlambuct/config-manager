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
    },

    #[clap(name = "show")]
    Show {
        file: String,
        #[clap(short, long, default_value = "")]
        get: String,
    },

    #[clap(name = "convert")]
    Convert {
        input: String,
        output: String,
    },
}
