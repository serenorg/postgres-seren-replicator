// ABOUTME: Wrapper for pg_dump command to export database objects
// ABOUTME: Handles global objects, schema, and data export

use crate::filters::ReplicationFilter;
use anyhow::{bail, Context, Result};
use std::process::Command;

/// Dump global objects (roles, tablespaces) using pg_dumpall
pub async fn dump_globals(source_url: &str, output_path: &str) -> Result<()> {
    tracing::info!("Dumping global objects to {}", output_path);

    let output = Command::new("pg_dumpall")
        .arg("--globals-only")
        .arg("--no-role-passwords") // Don't dump passwords
        .arg(format!("--dbname={}", source_url))
        .arg(format!("--file={}", output_path))
        .output()
        .context(
            "Failed to execute pg_dumpall. Is PostgreSQL client installed?\n\
             Install with:\n\
             - Ubuntu/Debian: sudo apt-get install postgresql-client\n\
             - macOS: brew install postgresql\n\
             - RHEL/CentOS: sudo yum install postgresql",
        )?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "pg_dumpall failed to dump global objects.\n\
             Error: {}\n\
             \n\
             Common causes:\n\
             - Connection authentication failed\n\
             - User lacks sufficient privileges (need SUPERUSER or pg_read_all_settings role)\n\
             - Network connectivity issues\n\
             - Invalid connection string",
            stderr
        );
    }

    tracing::info!("✓ Global objects dumped successfully");
    Ok(())
}

/// Dump schema (DDL) for a specific database
pub async fn dump_schema(
    source_url: &str,
    database: &str,
    output_path: &str,
    filter: &ReplicationFilter,
) -> Result<()> {
    tracing::info!(
        "Dumping schema for database '{}' to {}",
        database,
        output_path
    );

    let mut cmd = Command::new("pg_dump");
    cmd.arg("--schema-only")
        .arg("--no-owner") // Don't include ownership commands
        .arg("--no-privileges"); // We'll handle privileges separately

    // Add table filtering if specified
    if let Some(exclude_tables) = get_excluded_tables_for_db(filter, database) {
        if !exclude_tables.is_empty() {
            for table in exclude_tables {
                cmd.arg("--exclude-table").arg(&table);
            }
        }
    }

    // If include_tables is specified, only dump those tables
    if let Some(include_tables) = get_included_tables_for_db(filter, database) {
        if !include_tables.is_empty() {
            for table in include_tables {
                cmd.arg("--table").arg(&table);
            }
        }
    }

    cmd.arg(format!("--dbname={}", source_url))
        .arg(format!("--file={}", output_path));

    let output = cmd.output().context(
        "Failed to execute pg_dump. Is PostgreSQL client installed?\n\
         Install with:\n\
         - Ubuntu/Debian: sudo apt-get install postgresql-client\n\
         - macOS: brew install postgresql\n\
         - RHEL/CentOS: sudo yum install postgresql",
    )?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "pg_dump failed to dump schema for database '{}'.\n\
             Error: {}\n\
             \n\
             Common causes:\n\
             - Database does not exist\n\
             - Connection authentication failed\n\
             - User lacks privileges to read database schema\n\
             - Network connectivity issues",
            database,
            stderr
        );
    }

    tracing::info!("✓ Schema dumped successfully");
    Ok(())
}

/// Dump data for a specific database using optimized directory format
///
/// Uses PostgreSQL directory format dump with:
/// - Parallel dumps for faster performance
/// - Maximum compression (level 9)
/// - Large object (blob) support
/// - Directory output for efficient parallel restore
///
/// The number of parallel jobs is automatically determined based on available CPU cores.
pub async fn dump_data(
    source_url: &str,
    database: &str,
    output_path: &str,
    filter: &ReplicationFilter,
) -> Result<()> {
    // Determine optimal number of parallel jobs (number of CPUs, capped at 8)
    let num_cpus = std::thread::available_parallelism()
        .map(|n| n.get().min(8))
        .unwrap_or(4);

    tracing::info!(
        "Dumping data for database '{}' to {} (parallel={}, compression=9, format=directory)",
        database,
        output_path,
        num_cpus
    );

    let mut cmd = Command::new("pg_dump");
    cmd.arg("--data-only")
        .arg("--no-owner")
        .arg("--format=directory") // Directory format enables parallel operations
        .arg("--blobs") // Include large objects (blobs)
        .arg("--compress=9") // Maximum compression for smaller dump size
        .arg(format!("--jobs={}", num_cpus)); // Parallel dump jobs

    // Add table filtering if specified
    if let Some(exclude_tables) = get_excluded_tables_for_db(filter, database) {
        if !exclude_tables.is_empty() {
            for table in exclude_tables {
                cmd.arg("--exclude-table-data").arg(&table);
            }
        }
    }

    // If include_tables is specified, only dump data for those tables
    if let Some(include_tables) = get_included_tables_for_db(filter, database) {
        if !include_tables.is_empty() {
            for table in include_tables {
                cmd.arg("--table").arg(&table);
            }
        }
    }

    cmd.arg(format!("--dbname={}", source_url))
        .arg(format!("--file={}", output_path));

    let output = cmd.output().context(
        "Failed to execute pg_dump. Is PostgreSQL client installed?\n\
         Install with:\n\
         - Ubuntu/Debian: sudo apt-get install postgresql-client\n\
         - macOS: brew install postgresql\n\
         - RHEL/CentOS: sudo yum install postgresql",
    )?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "pg_dump failed to dump data for database '{}'.\n\
             Error: {}\n\
             \n\
             Common causes:\n\
             - Database does not exist\n\
             - Connection authentication failed\n\
             - User lacks privileges to read table data\n\
             - Network connectivity issues\n\
             - Insufficient disk space for dump directory\n\
             - Output directory already exists (pg_dump requires non-existent path)",
            database,
            stderr
        );
    }

    tracing::info!(
        "✓ Data dumped successfully using {} parallel jobs",
        num_cpus
    );
    Ok(())
}

/// Extract table names for a specific database from exclude_tables filter
fn get_excluded_tables_for_db(filter: &ReplicationFilter, db_name: &str) -> Option<Vec<String>> {
    filter.exclude_tables().map(|tables| {
        tables
            .iter()
            .filter_map(|full_name| {
                let parts: Vec<&str> = full_name.split('.').collect();
                if parts.len() == 2 && parts[0] == db_name {
                    Some(parts[1].to_string())
                } else {
                    None
                }
            })
            .collect()
    })
}

/// Extract table names for a specific database from include_tables filter
fn get_included_tables_for_db(filter: &ReplicationFilter, db_name: &str) -> Option<Vec<String>> {
    filter.include_tables().map(|tables| {
        tables
            .iter()
            .filter_map(|full_name| {
                let parts: Vec<&str> = full_name.split('.').collect();
                if parts.len() == 2 && parts[0] == db_name {
                    Some(parts[1].to_string())
                } else {
                    None
                }
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    #[ignore]
    async fn test_dump_globals() {
        let url = std::env::var("TEST_SOURCE_URL").unwrap();
        let dir = tempdir().unwrap();
        let output = dir.path().join("globals.sql");

        let result = dump_globals(&url, output.to_str().unwrap()).await;

        assert!(result.is_ok());
        assert!(output.exists());

        // Verify file contains SQL
        let content = std::fs::read_to_string(&output).unwrap();
        assert!(content.contains("CREATE ROLE") || !content.is_empty());
    }

    #[tokio::test]
    #[ignore]
    async fn test_dump_schema() {
        let url = std::env::var("TEST_SOURCE_URL").unwrap();
        let dir = tempdir().unwrap();
        let output = dir.path().join("schema.sql");

        // Extract database name from URL
        let db = url.split('/').next_back().unwrap_or("postgres");

        let filter = crate::filters::ReplicationFilter::empty();
        let result = dump_schema(&url, db, output.to_str().unwrap(), &filter).await;

        assert!(result.is_ok());
        assert!(output.exists());
    }

    #[test]
    fn test_get_excluded_tables_for_db() {
        let filter = crate::filters::ReplicationFilter::new(
            None,
            None,
            None,
            Some(vec![
                "db1.table1".to_string(),
                "db1.table2".to_string(),
                "db2.table3".to_string(),
            ]),
        )
        .unwrap();

        let tables = get_excluded_tables_for_db(&filter, "db1").unwrap();
        assert_eq!(tables, vec!["table1", "table2"]);

        let tables = get_excluded_tables_for_db(&filter, "db2").unwrap();
        assert_eq!(tables, vec!["table3"]);

        let tables = get_excluded_tables_for_db(&filter, "db3");
        assert!(tables.is_none() || tables.unwrap().is_empty());
    }

    #[test]
    fn test_get_included_tables_for_db() {
        let filter = crate::filters::ReplicationFilter::new(
            None,
            None,
            Some(vec![
                "db1.users".to_string(),
                "db1.orders".to_string(),
                "db2.products".to_string(),
            ]),
            None,
        )
        .unwrap();

        let tables = get_included_tables_for_db(&filter, "db1").unwrap();
        assert_eq!(tables, vec!["users", "orders"]);

        let tables = get_included_tables_for_db(&filter, "db2").unwrap();
        assert_eq!(tables, vec!["products"]);

        let tables = get_included_tables_for_db(&filter, "db3");
        assert!(tables.is_none() || tables.unwrap().is_empty());
    }

    #[test]
    fn test_get_excluded_tables_for_db_with_empty_filter() {
        let filter = crate::filters::ReplicationFilter::empty();
        let tables = get_excluded_tables_for_db(&filter, "db1");
        assert!(tables.is_none());
    }

    #[test]
    fn test_get_included_tables_for_db_with_empty_filter() {
        let filter = crate::filters::ReplicationFilter::empty();
        let tables = get_included_tables_for_db(&filter, "db1");
        assert!(tables.is_none());
    }
}
