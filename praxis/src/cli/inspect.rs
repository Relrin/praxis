use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;

use praxis_core::inspect::{
    context_audit_json, detect_bundle_type, diff_audit_json, format_context_bundle,
    format_diff_bundle, validate_context_bundle, validate_diff_bundle, BundleType,
};

#[derive(Parser)]
pub struct InspectArgs {
    /// Path to the bundle file to inspect (context.json or diff.json).
    pub file: PathBuf,

    /// Include dropped/skipped file list in output.
    #[arg(long, default_value_t = false)]
    pub verbose: bool,

    /// Output the audit as structured JSON instead of formatted text.
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

pub fn execute(args: InspectArgs) -> Result<()> {
    let content = std::fs::read_to_string(&args.file)
        .with_context(|| format!("File not found: {}", args.file.display()))?;

    let value: serde_json::Value = serde_json::from_str(&content)
        .with_context(|| format!("Invalid JSON in {}", args.file.display()))?;

    let bundle_type = detect_bundle_type(&value)?;

    match bundle_type {
        BundleType::Context => {
            let bundle: praxis_core::output::ContextBundle =
                serde_json::from_value(value).with_context(|| {
                    format!(
                        "Failed to parse ContextBundle from {}",
                        args.file.display()
                    )
                })?;

            let warnings = validate_context_bundle(&bundle);

            if args.json {
                let json = context_audit_json(&bundle, warnings)?;
                println!("{json}");
            } else {
                let text = format_context_bundle(&bundle, args.verbose);
                print!("{text}");
                print_warnings(&warnings);
            }
        }
        BundleType::Diff => {
            let bundle: praxis_core::diff::DiffBundle =
                serde_json::from_value(value).with_context(|| {
                    format!(
                        "Failed to parse DiffBundle from {}",
                        args.file.display()
                    )
                })?;

            let warnings = validate_diff_bundle(&bundle);

            if args.json {
                let json = diff_audit_json(&bundle, warnings)?;
                println!("{json}");
            } else {
                let text = format_diff_bundle(&bundle, args.verbose);
                print!("{text}");
                print_warnings(&warnings);
            }
        }
    }

    Ok(())
}

fn print_warnings(warnings: &[String]) {
    if warnings.is_empty() {
        println!("Warnings:  none");
    } else {
        println!("Warnings:  {}", warnings.len());
        for w in warnings {
            println!("  - {w}");
        }
    }
}
