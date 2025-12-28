//! NervusDB Migration Tool
//!
//! Command-line utility to migrate databases from v1.x (.synapsedb) to v2.0 (redb)
//!
//! Usage:
//!   nervus-migrate <source.synapsedb> <target.redb> [--verify]
//!
//! Options:
//!   --verify    Perform SHA256 integrity verification
//!   --help      Show this help message

use std::path::PathBuf;
use std::process;

use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};

use nervusdb_core::migration::{MigrationStats, migrate_database};

#[derive(Parser, Debug)]
#[command(name = "nervus-migrate")]
#[command(about = "Migrate NervusDB from v1.x to v2.0", long_about = None)]
struct Args {
    /// Path to the legacy .synapsedb file
    #[arg(value_name = "SOURCE")]
    source: PathBuf,

    /// Path for the new .redb database
    #[arg(value_name = "TARGET")]
    target: PathBuf,

    /// Perform SHA256 integrity verification
    #[arg(long, default_value_t = false)]
    verify: bool,

    /// Verbose output
    #[arg(short, long, default_value_t = false)]
    verbose: bool,
}

fn main() {
    let args = Args::parse();

    // Validate source file exists
    if !args.source.exists() {
        eprintln!("Error: Source file does not exist: {:?}", args.source);
        process::exit(1);
    }

    // Validate source has .synapsedb extension
    if args.source.extension().and_then(|s| s.to_str()) != Some("synapsedb") {
        eprintln!(
            "Warning: Source file does not have .synapsedb extension: {:?}",
            args.source
        );
    }

    // Validate target doesn't exist
    if args.target.exists() {
        eprintln!(
            "Error: Target file already exists: {:?}\nPlease remove it first or choose a different path.",
            args.target
        );
        process::exit(1);
    }

    println!("🔄 NervusDB Migration Tool v2.0");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Source: {:?}", args.source);
    println!("Target: {:?}", args.target);
    println!("Verify: {}", if args.verify { "yes" } else { "no" });
    println!();

    // Create progress spinner
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );

    spinner.set_message("Reading legacy database...");
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    // Perform migration
    let result = migrate_database(&args.source, &args.target, args.verify);

    spinner.finish_and_clear();

    match result {
        Ok(stats) => {
            print_success(&stats, args.verify);
            process::exit(0);
        }
        Err(e) => {
            eprintln!("❌ Migration failed: {}", e);
            process::exit(1);
        }
    }
}

fn print_success(stats: &MigrationStats, verify: bool) {
    println!("✅ Migration completed successfully!");
    println!();
    println!("📊 Statistics:");
    println!("  Dictionary entries: {}", stats.dictionary_entries);
    println!("  Triples migrated:   {}", stats.triples_migrated);
    println!("  Properties migrated: {}", stats.properties_migrated);
    println!("  Duration:           {:.2}s", stats.duration_secs);

    if verify {
        println!();
        println!("🔐 Integrity verification:");
        println!("  Source SHA256:  {}", stats.source_sha256);
        println!("  Target SHA256:  {}", stats.target_sha256);
    }

    println!();
    println!("🎉 Your database has been migrated to the new v2.0 format!");
    println!("   You can now use it with NervusDB v2.0+");
}
