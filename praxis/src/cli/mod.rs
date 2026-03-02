mod build;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "praxis", version, about = "Build deterministic context bundles from repositories.")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Build(build::BuildArgs),
}

/// Parses CLI arguments and dispatches to the appropriate subcommand.
pub fn execute() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Build(args) => build::execute(args),
    }
}
