mod build;
mod common;
mod diff;
mod inspect;
mod prune;
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
    /// Build a context bundle from a repository
    Build(build::BuildArgs),
    /// Extract structured memory from a conversation file
    Summarize(summarize::SummarizeArgs),
    /// Compute file and symbol-level changes between two git refs
    Diff(diff::DiffArgs),
    /// Inspect an existing bundle with a human-readable audit
    Inspect(inspect::InspectArgs),
    /// Re-run budget allocation on an existing bundle with a new budget
    Prune(prune::PruneArgs),
}

/// Parses CLI arguments and dispatches to the appropriate subcommand.
pub fn execute() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Build(args) => build::execute(args),
        Command::Summarize(args) => summarize::execute(args),
        Command::Diff(args) => diff::execute(args),
        Command::Inspect(args) => inspect::execute(args),
        Command::Prune(args) => prune::execute(args),
    }
}
