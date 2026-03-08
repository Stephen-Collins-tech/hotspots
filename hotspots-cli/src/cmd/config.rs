use anyhow::Context;
use hotspots_core::config;

#[derive(clap::Subcommand)]
pub(crate) enum ConfigAction {
    /// Validate a config file without running analysis
    Validate {
        /// Path to config file (default: auto-discover from current directory)
        #[arg(long)]
        path: Option<std::path::PathBuf>,
    },
    /// Show the resolved configuration (merged defaults + config file)
    Show {
        /// Path to config file (default: auto-discover from current directory)
        #[arg(long)]
        path: Option<std::path::PathBuf>,
    },
}

pub(crate) fn handle_config(action: ConfigAction) -> anyhow::Result<()> {
    match action {
        ConfigAction::Validate { path } => {
            let project_root = std::env::current_dir()?;
            let resolved = config::load_and_resolve(&project_root, path.as_deref());
            match resolved {
                Ok(config) => {
                    if let Some(ref p) = config.config_path {
                        println!("Config valid: {}", p.display());
                    } else {
                        println!("No config file found. Using defaults.");
                    }
                }
                Err(e) => {
                    eprintln!("Config validation failed: {:#}", e);
                    std::process::exit(1);
                }
            }
        }
        ConfigAction::Show { path } => {
            let project_root = std::env::current_dir()?;
            let resolved = config::load_and_resolve(&project_root, path.as_deref())
                .context("failed to load configuration")?;

            println!("Configuration:");
            if let Some(ref p) = resolved.config_path {
                println!("  Source: {}", p.display());
            } else {
                println!("  Source: defaults (no config file found)");
            }
            println!();
            println!("Weights:");
            println!("  cc: {}", resolved.weight_cc);
            println!("  nd: {}", resolved.weight_nd);
            println!("  fo: {}", resolved.weight_fo);
            println!("  ns: {}", resolved.weight_ns);
            println!();
            println!("Thresholds:");
            println!("  moderate: {}", resolved.moderate_threshold);
            println!("  high: {}", resolved.high_threshold);
            println!("  critical: {}", resolved.critical_threshold);
            println!();
            println!("Filters:");
            println!(
                "  min_lrs: {}",
                resolved
                    .min_lrs
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "none".to_string())
            );
            println!(
                "  top: {}",
                resolved
                    .top_n
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "none".to_string())
            );
            println!(
                "  include: {}",
                if resolved.include.is_some() {
                    "custom patterns"
                } else {
                    "all files"
                }
            );
            println!(
                "  exclude: active ({} patterns)",
                if resolved.config_path.is_some() {
                    "custom"
                } else {
                    "default"
                }
            );
        }
    }
    Ok(())
}
