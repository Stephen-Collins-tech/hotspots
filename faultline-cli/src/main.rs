//! Faultline CLI - command-line interface for TypeScript analysis

#![deny(warnings)]

// Global invariants enforced:
// - Deterministic output ordering
// - Identical input yields byte-for-byte identical output

use clap::{Parser, Subcommand};
use faultline_core::{analyze, render_json, render_text, AnalysisOptions};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "faultline")]
#[command(about = "Static analysis tool for TypeScript functions")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Analyze TypeScript files
    Analyze {
        /// Path to TypeScript file or directory
        path: PathBuf,
        
        /// Output format
        #[arg(long, default_value = "text")]
        format: OutputFormat,
        
        /// Show only top N results
        #[arg(long)]
        top: Option<usize>,
        
        /// Minimum LRS threshold
        #[arg(long)]
        min_lrs: Option<f64>,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Analyze { path, format, top, min_lrs } => {
            // Normalize path to absolute
            let normalized_path = if path.is_relative() {
                std::env::current_dir()?.join(&path)
            } else {
                path
            };
            
            // Validate path exists
            if !normalized_path.exists() {
                anyhow::bail!("Path does not exist: {}", normalized_path.display());
            }
            
            // Build options
            let options = AnalysisOptions {
                min_lrs,
                top_n: top,
            };
            
            // Analyze
            let reports = analyze(&normalized_path, options)?;
            
            // Render output
            match format {
                OutputFormat::Text => {
                    print!("{}", render_text(&reports));
                }
                OutputFormat::Json => {
                    println!("{}", render_json(&reports));
                }
            }
        }
    }
    
    Ok(())
}
