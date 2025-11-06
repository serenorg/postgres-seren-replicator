// ABOUTME: Pre-flight validation command for migration readiness
// ABOUTME: Checks connectivity, privileges, and version compatibility

use crate::{postgres, utils};
use anyhow::{bail, Context, Result};

/// Pre-flight validation command for migration readiness
///
/// Performs comprehensive validation before migration:
/// - Checks for required PostgreSQL client tools (pg_dump, pg_dumpall, psql)
/// - Validates connection string format
/// - Tests connectivity to both source and target databases
/// - Verifies source user has REPLICATION privilege
/// - Verifies target user has CREATEDB privilege
/// - Confirms PostgreSQL major versions match
/// - Validates extension compatibility and preload requirements
///
/// # Arguments
///
/// * `source_url` - PostgreSQL connection string for source (Neon) database
/// * `target_url` - PostgreSQL connection string for target (Seren) database
///
/// # Returns
///
/// Returns `Ok(())` if all validation checks pass.
///
/// # Errors
///
/// This function will return an error if:
/// - Required PostgreSQL tools are not installed
/// - Connection strings are invalid
/// - Cannot connect to source or target database
/// - Source user lacks REPLICATION privilege
/// - Target user lacks CREATEDB privilege
/// - PostgreSQL major versions don't match
///
/// # Examples
///
/// ```no_run
/// # use anyhow::Result;
/// # use postgres_seren_replicator::commands::validate;
/// # async fn example() -> Result<()> {
/// validate(
///     "postgresql://user:pass@neon.tech/sourcedb",
///     "postgresql://user:pass@seren.example.com/targetdb"
/// ).await?;
/// # Ok(())
/// # }
/// ```
pub async fn validate(source_url: &str, target_url: &str) -> Result<()> {
    tracing::info!("Starting validation...");

    // Step 0a: Check for required tools
    tracing::info!("Checking for required PostgreSQL client tools...");
    utils::check_required_tools().context("Required tools check failed")?;
    tracing::info!("✓ Required tools found (pg_dump, pg_dumpall, psql)");

    // Step 0b: Validate connection strings
    tracing::info!("Validating connection strings...");
    utils::validate_connection_string(source_url).context("Invalid source connection string")?;
    utils::validate_connection_string(target_url).context("Invalid target connection string")?;
    tracing::info!("✓ Connection strings are valid");

    // Step 1: Connect to source
    tracing::info!("Connecting to source database...");
    let source_client = postgres::connect(source_url)
        .await
        .context("Failed to connect to source database")?;
    tracing::info!("✓ Connected to source");

    // Step 2: Connect to target
    tracing::info!("Connecting to target database...");
    let target_client = postgres::connect(target_url)
        .await
        .context("Failed to connect to target database")?;
    tracing::info!("✓ Connected to target");

    // Step 3: Check source privileges
    tracing::info!("Checking source privileges...");
    let source_privs = postgres::check_source_privileges(&source_client).await?;
    if !source_privs.has_replication && !source_privs.is_superuser {
        bail!("Source user lacks REPLICATION privilege. Grant with: ALTER USER <user> WITH REPLICATION;");
    }
    tracing::info!("✓ Source has replication privileges");

    // Step 4: Check target privileges
    tracing::info!("Checking target privileges...");
    let target_privs = postgres::check_target_privileges(&target_client).await?;
    if !target_privs.has_create_db && !target_privs.is_superuser {
        bail!(
            "Target user lacks CREATE DATABASE privilege. Grant with: ALTER USER <user> CREATEDB;"
        );
    }
    if !target_privs.has_create_role && !target_privs.is_superuser {
        tracing::warn!("⚠ Target user lacks CREATE ROLE privilege. Role migration may fail.");
    }
    tracing::info!("✓ Target has sufficient privileges");

    // Step 5: Check PostgreSQL versions
    tracing::info!("Checking PostgreSQL versions...");
    let source_version = get_pg_version(&source_client).await?;
    let target_version = get_pg_version(&target_client).await?;

    if source_version.major != target_version.major {
        bail!(
            "PostgreSQL major version mismatch: source={}.{}, target={}.{}. Logical replication requires same major version.",
            source_version.major, source_version.minor,
            target_version.major, target_version.minor
        );
    }
    tracing::info!(
        "✓ Version compatibility confirmed (both {}.{})",
        source_version.major,
        source_version.minor
    );

    // Step 6: Check extension compatibility
    tracing::info!("Checking extension compatibility...");
    check_extension_compatibility(&source_client, &target_client).await?;
    tracing::info!("✓ Extension compatibility confirmed");

    tracing::info!("✅ Validation complete - ready for migration");
    Ok(())
}

struct PgVersion {
    major: u32,
    minor: u32,
}

async fn get_pg_version(client: &tokio_postgres::Client) -> Result<PgVersion> {
    let row = client
        .query_one("SHOW server_version", &[])
        .await
        .context("Failed to get PostgreSQL version")?;

    let version_str: String = row.get(0);

    // Parse version string like "16.2 (Debian 16.2-1.pgdg120+1)"
    let parts: Vec<&str> = version_str
        .split_whitespace()
        .next()
        .unwrap_or("0.0")
        .split('.')
        .collect();

    let major = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);

    Ok(PgVersion { major, minor })
}

async fn check_extension_compatibility(
    source_client: &tokio_postgres::Client,
    target_client: &tokio_postgres::Client,
) -> Result<()> {
    // Get installed extensions from source
    let source_extensions = postgres::get_installed_extensions(source_client)
        .await
        .context("Failed to get source extensions")?;

    // If no extensions on source (besides plpgsql), skip checks
    if source_extensions.is_empty() {
        tracing::info!("  No extensions found on source database");
        return Ok(());
    }

    tracing::info!(
        "  Found {} extension(s) on source: {}",
        source_extensions.len(),
        source_extensions
            .iter()
            .map(|e| &e.name)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    );

    // Get available extensions on target
    let target_available = postgres::get_available_extensions(target_client)
        .await
        .context("Failed to get target available extensions")?;

    // Get preloaded libraries on target
    let target_preloaded = postgres::get_preloaded_libraries(target_client)
        .await
        .context("Failed to get target preloaded libraries")?;

    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // Check each source extension
    for source_ext in &source_extensions {
        // Check if extension is available on target
        let target_ext = target_available.iter().find(|e| e.name == source_ext.name);

        match target_ext {
            None => {
                errors.push(format!(
                    "Extension '{}' (version {}) is required but not available on target",
                    source_ext.name, source_ext.version
                ));
            }
            Some(target) => {
                // Check if extension requires preloading
                if postgres::requires_preload(&source_ext.name) {
                    let is_preloaded = target_preloaded.iter().any(|lib| lib == &source_ext.name);

                    if !is_preloaded {
                        errors.push(format!(
                            "Extension '{}' requires preloading but is not in shared_preload_libraries on target. \
                             Add to postgresql.conf: shared_preload_libraries = '{}' and restart PostgreSQL.",
                            source_ext.name, source_ext.name
                        ));
                    }
                }

                // Warn on version mismatch
                if let Some(target_version) = &target.default_version {
                    let source_major = source_ext.version.split('.').next().unwrap_or("0");
                    let target_major = target_version.split('.').next().unwrap_or("0");

                    if source_major != target_major {
                        warnings.push(format!(
                            "Extension '{}' version mismatch: source={}, target default={}",
                            source_ext.name, source_ext.version, target_version
                        ));
                    }
                }
            }
        }
    }

    // Report warnings
    for warning in &warnings {
        tracing::warn!("  ⚠ {}", warning);
    }

    // Report errors and fail if any
    if !errors.is_empty() {
        tracing::error!("Extension compatibility check failed:");
        for error in &errors {
            tracing::error!("  ✗ {}", error);
        }
        bail!("Target database is missing required extensions or configuration. See errors above.");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn test_validate_with_valid_databases_succeeds() {
        let source = std::env::var("TEST_SOURCE_URL").unwrap();
        let target = std::env::var("TEST_TARGET_URL").unwrap();

        let result = validate(&source, &target).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_validate_with_invalid_source_fails() {
        let result = validate("invalid-url", "postgresql://localhost/db").await;
        assert!(result.is_err());
    }
}
