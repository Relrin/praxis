mod build;
mod common;
mod diff;
mod inspect;
mod summarize;

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
    Summarize(summarize::SummarizeArgs),
    Diff(diff::DiffArgs),
    Inspect(inspect::InspectArgs),
}

/// Parses CLI arguments and dispatches to the appropriate subcommand.
pub fn execute() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Build(args) => build::execute(args),
        Command::Summarize(args) => summarize::execute(args),
        Command::Diff(args) => diff::execute(args),
        Command::Inspect(args) => inspect::execute(args),
    }
}
