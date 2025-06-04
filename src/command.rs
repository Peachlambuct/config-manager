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

    #[clap(name = "serve")]
    Serve {
        #[clap(short, long, default_value = "8080")]
        port: u16,
        #[clap(short = 'H', long, default_value = "0.0.0.0")]
        host: String,
        #[clap(short, long, default_value = ".")]
        config_path: String,
    },
}

#[derive(Debug)]
pub enum CliCommand {
    Add { path: String },

    Remove { path: String },

    Get { path: String },

    List,

    Update { old_path: String, new_path: String },
}

impl CliCommand {
    pub fn from_str(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.trim().split_whitespace().collect();
        if parts.is_empty() {
            return None;
        }
        
        let command = parts[0];

        match command {
            "add" => {
                if parts.len() >= 2 {
                    Some(Self::Add {
                        path: parts[1].to_string(),
                    })
                } else {
                    None
                }
            },
            "remove" => {
                if parts.len() >= 2 {
                    Some(Self::Remove {
                        path: parts[1].to_string(),
                    })
                } else {
                    None
                }
            },
            "get" => {
                if parts.len() >= 2 {
                    Some(Self::Get {
                        path: parts[1].to_string(),
                    })
                } else {
                    None
                }
            },
            "list" => Some(Self::List),
            "update" => {
                if parts.len() >= 3 {
                    Some(Self::Update {
                        old_path: parts[1].to_string(),
                        new_path: parts[2].to_string(),
                    })
                } else {
                    None
                }
            },
            _ => None,
        }
    }
}
