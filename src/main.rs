// ABOUTME: CLI entry point for neon-seren-replicator
// ABOUTME: Parses commands and routes to appropriate handlers

use clap::{Parser, Subcommand};
use neon_seren_replicator::commands;

#[derive(Parser)]
#[command(name = "neon-seren-replicator")]
#[command(about = "Zero-downtime PostgreSQL replication from Neon to Seren", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Validate source and target databases are ready for replication
    Validate {
        #[arg(long)]
        source: String,
        #[arg(long)]
        target: String,
    },
    /// Initialize replication with snapshot copy of schema and data
    Init {
        #[arg(long)]
        source: String,
        #[arg(long)]
        target: String,
        /// Skip confirmation prompt
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// Set up continuous logical replication from source to target
    Sync {
        #[arg(long)]
        source: String,
        #[arg(long)]
        target: String,
    },
    /// Check replication status and lag in real-time
    Status {
        #[arg(long)]
        source: String,
        #[arg(long)]
        target: String,
    },
    /// Verify data integrity between source and target
    Verify {
        #[arg(long)]
        source: String,
        #[arg(long)]
        target: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Validate { source, target } => commands::validate(&source, &target).await,
        Commands::Init {
            source,
            target,
            yes,
        } => commands::init(&source, &target, yes).await,
        Commands::Sync { source, target } => {
            commands::sync(&source, &target, None, None, None).await
        }
        Commands::Status { source, target } => commands::status(&source, &target, None).await,
        Commands::Verify { source, target } => commands::verify(&source, &target).await,
    }
}
