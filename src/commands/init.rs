// ABOUTME: Initial replication command for snapshot schema and data copy
// ABOUTME: Performs full database dump and restore from source to target

use crate::{migration, postgres};
use anyhow::{bail, Context, Result};
use std::io::{self, Write};
use tempfile::TempDir;

/// Initial replication command for snapshot schema and data copy
///
/// Performs a full database dump and restore from source to target in steps:
/// 1. Estimates database sizes and replication times
/// 2. Prompts for confirmation (unless skip_confirmation is true)
/// 3. Dumps global objects (roles, tablespaces) from source
/// 4. Restores global objects to target
/// 5. Discovers all user databases on source
/// 6. Replicates each database (schema and data)
///
/// Uses temporary directory for dump files, which is automatically cleaned up.
///
/// # Arguments
///
/// * `source_url` - PostgreSQL connection string for source (Neon) database
/// * `target_url` - PostgreSQL connection string for target (Seren) database
/// * `skip_confirmation` - Skip the size estimation and confirmation prompt
///
/// # Returns
///
/// Returns `Ok(())` if replication completes successfully.
///
/// # Errors
///
/// This function will return an error if:
/// - Cannot create temporary directory
/// - Global objects dump/restore fails
/// - Cannot connect to source database
/// - Database discovery fails
/// - Any database replication fails
/// - User declines confirmation prompt
///
/// # Examples
///
/// ```no_run
/// # use anyhow::Result;
/// # use postgres_seren_replicator::commands::init;
/// # async fn example() -> Result<()> {
/// // With confirmation prompt
/// init(
///     "postgresql://user:pass@neon.tech/sourcedb",
///     "postgresql://user:pass@seren.example.com/targetdb",
///     false
/// ).await?;
///
/// // Skip confirmation (automated scripts)
/// init(
///     "postgresql://user:pass@neon.tech/sourcedb",
///     "postgresql://user:pass@seren.example.com/targetdb",
///     true
/// ).await?;
/// # Ok(())
/// # }
/// ```
pub async fn init(source_url: &str, target_url: &str, skip_confirmation: bool) -> Result<()> {
    tracing::info!("Starting initial replication...");

    // Create temporary directory for dump files
    // TempDir automatically cleans up on drop, even if errors occur
    let temp_dir = TempDir::new().context("Failed to create temp directory")?;
    let temp_path = temp_dir.path();
    tracing::debug!("Using temp directory: {}", temp_path.display());

    // Step 1: Dump global objects
    tracing::info!("Step 1/4: Dumping global objects (roles, tablespaces)...");
    let globals_file = temp_path.join("globals.sql");
    migration::dump_globals(source_url, globals_file.to_str().unwrap()).await?;

    // Step 2: Restore global objects
    tracing::info!("Step 2/4: Restoring global objects to target...");
    migration::restore_globals(target_url, globals_file.to_str().unwrap()).await?;

    // Step 3: Discover databases
    tracing::info!("Step 3/4: Discovering databases...");
    let source_client = postgres::connect(source_url).await?;
    let databases = migration::list_databases(&source_client).await?;

    if databases.is_empty() {
        tracing::warn!("⚠ No user databases found on source");
        tracing::warn!("  This is unusual - the source database appears empty");
        tracing::warn!("  Only global objects (roles, tablespaces) will be replicated");
        tracing::info!("✅ Initial replication complete (no databases to replicate)");
        return Ok(());
    }

    tracing::info!("Found {} database(s) to replicate", databases.len());

    // Estimate database sizes and get confirmation
    if !skip_confirmation {
        tracing::info!("Analyzing database sizes...");
        let size_estimates = migration::estimate_database_sizes(&source_client, &databases).await?;

        if !confirm_replication(&size_estimates)? {
            bail!("Replication cancelled by user");
        }
    }

    // Step 4: Replicate each database
    tracing::info!("Step 4/4: Replicating databases...");
    for (idx, db_info) in databases.iter().enumerate() {
        tracing::info!(
            "Replicating database {}/{}: '{}'",
            idx + 1,
            databases.len(),
            db_info.name
        );

        // Build connection URLs for this specific database
        let source_db_url = replace_database_in_url(source_url, &db_info.name)?;
        let target_db_url = replace_database_in_url(target_url, &db_info.name)?;

        // Create database on target if it doesn't exist
        let target_client = postgres::connect(target_url).await?;
        create_database_if_not_exists(&target_client, &db_info.name).await?;

        // Dump and restore schema
        tracing::info!("  Dumping schema for '{}'...", db_info.name);
        let schema_file = temp_path.join(format!("{}_schema.sql", db_info.name));
        migration::dump_schema(&source_db_url, &db_info.name, schema_file.to_str().unwrap())
            .await?;

        tracing::info!("  Restoring schema for '{}'...", db_info.name);
        migration::restore_schema(&target_db_url, schema_file.to_str().unwrap()).await?;

        // Dump and restore data (using directory format for parallel operations)
        tracing::info!("  Dumping data for '{}'...", db_info.name);
        let data_dir = temp_path.join(format!("{}_data.dump", db_info.name));
        migration::dump_data(&source_db_url, &db_info.name, data_dir.to_str().unwrap()).await?;

        tracing::info!("  Restoring data for '{}'...", db_info.name);
        migration::restore_data(&target_db_url, data_dir.to_str().unwrap()).await?;

        tracing::info!("✓ Database '{}' replicated successfully", db_info.name);
    }

    tracing::info!("✅ Initial replication complete");
    Ok(())
}

/// Replace the database name in a connection URL
fn replace_database_in_url(url: &str, new_database: &str) -> Result<String> {
    // Parse URL to find database name
    // Format: postgresql://user:pass@host:port/database?params

    // Split by '?' to separate params
    let parts: Vec<&str> = url.split('?').collect();
    let base_url = parts[0];
    let params = if parts.len() > 1 {
        Some(parts[1])
    } else {
        None
    };

    // Split base by '/' to get everything before database name
    let url_parts: Vec<&str> = base_url.rsplitn(2, '/').collect();
    if url_parts.len() != 2 {
        anyhow::bail!("Invalid connection URL format");
    }

    // Reconstruct URL with new database name
    let mut new_url = format!("{}/{}", url_parts[1], new_database);
    if let Some(p) = params {
        new_url = format!("{}?{}", new_url, p);
    }

    Ok(new_url)
}

/// Create database if it doesn't already exist
async fn create_database_if_not_exists(
    client: &tokio_postgres::Client,
    database: &str,
) -> Result<()> {
    let query = format!("CREATE DATABASE \"{}\"", database);

    match client.execute(&query, &[]).await {
        Ok(_) => {
            tracing::info!("  Created database '{}'", database);
            Ok(())
        }
        Err(e) => {
            // Database might already exist - that's okay
            if e.to_string().contains("already exists") {
                tracing::info!("  Database '{}' already exists", database);
                Ok(())
            } else {
                Err(e).context(format!("Failed to create database '{}'", database))
            }
        }
    }
}

/// Display database size estimates and prompt for confirmation
///
/// Shows a table with database names, sizes, and estimated replication times.
/// Prompts the user to proceed with the replication.
///
/// # Arguments
///
/// * `sizes` - Vector of database size estimates
///
/// # Returns
///
/// Returns `true` if user confirms (enters 'y'), `false` otherwise.
///
/// # Errors
///
/// Returns an error if stdin/stdout operations fail.
fn confirm_replication(sizes: &[migration::DatabaseSizeInfo]) -> Result<bool> {
    use std::time::Duration;

    // Calculate totals
    let total_bytes: i64 = sizes.iter().map(|s| s.size_bytes).sum();
    let total_duration: Duration = sizes.iter().map(|s| s.estimated_duration).sum();

    // Print table header
    println!();
    println!("{:<20} {:<12} {:<15}", "Database", "Size", "Est. Time");
    println!("{}", "─".repeat(50));

    // Print each database
    for size in sizes {
        println!(
            "{:<20} {:<12} {:<15}",
            size.name,
            size.size_human,
            migration::format_duration(size.estimated_duration)
        );
    }

    // Print totals
    println!("{}", "─".repeat(50));
    println!(
        "Total: {} (estimated {})",
        migration::format_bytes(total_bytes),
        migration::format_duration(total_duration)
    );
    println!();

    // Prompt for confirmation
    print!("Proceed with replication? [y/N]: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("Failed to read user input")?;

    Ok(input.trim().to_lowercase() == "y")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn test_init_replicates_database() {
        let source = std::env::var("TEST_SOURCE_URL").unwrap();
        let target = std::env::var("TEST_TARGET_URL").unwrap();

        // Skip confirmation for automated tests
        let result = init(&source, &target, true).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_replace_database_in_url() {
        let url = "postgresql://user:pass@host:5432/olddb?sslmode=require";
        let result = replace_database_in_url(url, "newdb").unwrap();
        assert_eq!(
            result,
            "postgresql://user:pass@host:5432/newdb?sslmode=require"
        );

        let url_no_params = "postgresql://user:pass@host:5432/olddb";
        let result = replace_database_in_url(url_no_params, "newdb").unwrap();
        assert_eq!(result, "postgresql://user:pass@host:5432/newdb");
    }
}
