// ABOUTME: CLI entry point for postgres-seren-replicator
// ABOUTME: Parses commands and routes to appropriate handlers

use clap::{Parser, Subcommand};
use postgres_seren_replicator::commands;

#[derive(Parser)]
#[command(name = "postgres-seren-replicator")]
#[command(about = "Zero-downtime PostgreSQL replication to Seren Cloud", long_about = None)]
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
    // Initialize logging - default to INFO level if RUST_LOG not set
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

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
