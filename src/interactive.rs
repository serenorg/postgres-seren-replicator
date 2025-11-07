// ABOUTME: Interactive terminal UI for database and table selection
// ABOUTME: Provides multi-select interface for selective replication

use crate::{filters::ReplicationFilter, migration, postgres};
use anyhow::{Context, Result};
use dialoguer::{theme::ColorfulTheme, Confirm, MultiSelect};

/// Interactive database and table selection
///
/// Presents a terminal UI for selecting:
/// 1. Which databases to replicate (multi-select)
/// 2. For each selected database, which tables to exclude (multi-select)
/// 3. Summary and confirmation
///
/// Returns a `ReplicationFilter` representing the user's selections.
///
/// # Arguments
///
/// * `source_url` - PostgreSQL connection string for source database
///
/// # Returns
///
/// Returns `Ok(ReplicationFilter)` with the user's selections or an error if:
/// - Cannot connect to source database
/// - Cannot discover databases or tables
/// - User cancels the operation
///
/// # Examples
///
/// ```no_run
/// # use anyhow::Result;
/// # use postgres_seren_replicator::interactive::select_databases_and_tables;
/// # async fn example() -> Result<()> {
/// let filter = select_databases_and_tables(
///     "postgresql://user:pass@source.example.com/postgres"
/// ).await?;
/// # Ok(())
/// # }
/// ```
pub async fn select_databases_and_tables(source_url: &str) -> Result<ReplicationFilter> {
    tracing::info!("Starting interactive database and table selection...");
    tracing::info!("");

    // Connect to source database
    tracing::info!("Connecting to source database...");
    let source_client = postgres::connect(source_url)
        .await
        .context("Failed to connect to source database")?;
    tracing::info!("✓ Connected to source");
    tracing::info!("");

    // Discover databases
    tracing::info!("Discovering databases on source...");
    let all_databases = migration::list_databases(&source_client)
        .await
        .context("Failed to list databases on source")?;

    if all_databases.is_empty() {
        tracing::warn!("⚠ No user databases found on source");
        tracing::warn!("  Source appears to contain only template databases");
        return Ok(ReplicationFilter::empty());
    }

    tracing::info!("✓ Found {} database(s)", all_databases.len());
    tracing::info!("");

    // Step 1: Select databases to replicate
    println!("Select databases to replicate:");
    println!("(Use arrow keys to navigate, Space to select, Enter to confirm)");
    println!();

    let db_names: Vec<String> = all_databases.iter().map(|db| db.name.clone()).collect();

    let db_selections = MultiSelect::with_theme(&ColorfulTheme::default())
        .items(&db_names)
        .interact()
        .context("Failed to get database selection")?;

    if db_selections.is_empty() {
        tracing::warn!("⚠ No databases selected");
        tracing::info!("  Cancelling interactive selection");
        return Ok(ReplicationFilter::empty());
    }

    let selected_databases: Vec<String> = db_selections
        .iter()
        .map(|&idx| db_names[idx].clone())
        .collect();

    tracing::info!("");
    tracing::info!("✓ Selected {} database(s):", selected_databases.len());
    for db in &selected_databases {
        tracing::info!("  - {}", db);
    }
    tracing::info!("");

    // Step 2: For each selected database, optionally exclude tables
    let mut excluded_tables: Vec<String> = Vec::new();

    for db_name in &selected_databases {
        // Build database-specific connection URL
        let db_url = replace_database_in_url(source_url, db_name)
            .context(format!("Failed to build URL for database '{}'", db_name))?;

        // Connect to the specific database
        tracing::info!("Discovering tables in database '{}'...", db_name);
        let db_client = postgres::connect(&db_url)
            .await
            .context(format!("Failed to connect to database '{}'", db_name))?;

        let all_tables = migration::list_tables(&db_client)
            .await
            .context(format!("Failed to list tables from database '{}'", db_name))?;

        if all_tables.is_empty() {
            tracing::info!("  No tables found in database '{}'", db_name);
            tracing::info!("");
            continue;
        }

        tracing::info!("✓ Found {} table(s) in '{}'", all_tables.len(), db_name);
        tracing::info!("");

        // Format table names for display
        let table_display_names: Vec<String> = all_tables
            .iter()
            .map(|t| {
                if t.schema == "public" {
                    t.name.clone()
                } else {
                    format!("{}.{}", t.schema, t.name)
                }
            })
            .collect();

        println!(
            "Select tables to EXCLUDE from '{}' (or press Enter to include all):",
            db_name
        );
        println!("(Use arrow keys to navigate, Space to select, Enter to confirm)");
        println!();

        let table_exclusions = MultiSelect::with_theme(&ColorfulTheme::default())
            .items(&table_display_names)
            .interact()
            .context(format!(
                "Failed to get table exclusion selection for database '{}'",
                db_name
            ))?;

        if !table_exclusions.is_empty() {
            let excluded_in_db: Vec<String> = table_exclusions
                .iter()
                .map(|&idx| {
                    // Build full table name in "database.table" format
                    format!("{}.{}", db_name, table_display_names[idx])
                })
                .collect();

            tracing::info!("");
            tracing::info!(
                "✓ Excluding {} table(s) from '{}':",
                excluded_in_db.len(),
                db_name
            );
            for table in &excluded_in_db {
                tracing::info!("  - {}", table);
            }

            excluded_tables.extend(excluded_in_db);
        } else {
            tracing::info!("");
            tracing::info!("✓ Including all tables from '{}'", db_name);
        }

        tracing::info!("");
    }

    // Step 3: Show summary and confirm
    println!();
    println!("========================================");
    println!("Replication Configuration Summary");
    println!("========================================");
    println!();
    println!("Databases to replicate: {}", selected_databases.len());
    for db in &selected_databases {
        println!("  ✓ {}", db);
    }
    println!();

    if !excluded_tables.is_empty() {
        println!("Tables to exclude: {}", excluded_tables.len());
        for table in &excluded_tables {
            println!("  ✗ {}", table);
        }
        println!();
    } else {
        println!("Tables to exclude: None (all tables will be replicated)");
        println!();
    }

    println!("========================================");
    println!();

    let confirmed = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Proceed with this configuration?")
        .default(true)
        .interact()
        .context("Failed to get confirmation")?;

    if !confirmed {
        tracing::warn!("⚠ User cancelled operation");
        anyhow::bail!("Interactive selection cancelled by user");
    }

    tracing::info!("");
    tracing::info!("✓ Configuration confirmed");
    tracing::info!("");

    // Step 4: Convert selections to ReplicationFilter
    let filter = if excluded_tables.is_empty() {
        // No table exclusions - just filter by databases
        ReplicationFilter::new(Some(selected_databases), None, None, None)?
    } else {
        // Include selected databases and exclude specific tables
        ReplicationFilter::new(Some(selected_databases), None, None, Some(excluded_tables))?
    };

    Ok(filter)
}

/// Replace the database name in a PostgreSQL connection URL
///
/// # Arguments
///
/// * `url` - PostgreSQL connection URL
/// * `new_db_name` - New database name to use
///
/// # Returns
///
/// URL with the database name replaced
fn replace_database_in_url(url: &str, new_db_name: &str) -> Result<String> {
    // Split into base URL and query parameters
    let parts: Vec<&str> = url.splitn(2, '?').collect();
    let base_url = parts[0];
    let query_params = parts.get(1);

    // Split base URL by '/' to replace the database name
    let url_parts: Vec<&str> = base_url.rsplitn(2, '/').collect();

    if url_parts.len() != 2 {
        anyhow::bail!("Invalid connection URL format: cannot replace database name");
    }

    // Rebuild URL with new database name
    let new_url = if let Some(params) = query_params {
        format!("{}/{}?{}", url_parts[1], new_db_name, params)
    } else {
        format!("{}/{}", url_parts[1], new_db_name)
    };

    Ok(new_url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replace_database_in_url() {
        // Basic URL
        let url = "postgresql://user:pass@localhost:5432/olddb";
        let new_url = replace_database_in_url(url, "newdb").unwrap();
        assert_eq!(new_url, "postgresql://user:pass@localhost:5432/newdb");

        // URL with query parameters
        let url = "postgresql://user:pass@localhost:5432/olddb?sslmode=require";
        let new_url = replace_database_in_url(url, "newdb").unwrap();
        assert_eq!(
            new_url,
            "postgresql://user:pass@localhost:5432/newdb?sslmode=require"
        );

        // URL without port
        let url = "postgresql://user:pass@localhost/olddb";
        let new_url = replace_database_in_url(url, "newdb").unwrap();
        assert_eq!(new_url, "postgresql://user:pass@localhost/newdb");
    }

    #[tokio::test]
    #[ignore]
    async fn test_interactive_selection() {
        // This test requires a real source database and manual interaction
        let source_url = std::env::var("TEST_SOURCE_URL").unwrap();

        let result = select_databases_and_tables(&source_url).await;

        // This will only work with manual interaction
        match &result {
            Ok(filter) => {
                println!("✓ Interactive selection completed");
                println!("Filter: {:?}", filter);
            }
            Err(e) => {
                println!("Interactive selection error: {:?}", e);
            }
        }
    }
}
