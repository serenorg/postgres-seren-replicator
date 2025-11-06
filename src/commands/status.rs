// ABOUTME: Status command implementation - Check replication health
// ABOUTME: Displays real-time replication lag and subscription status

use crate::postgres::connect;
use crate::replication::{get_replication_lag, get_subscription_status, is_replication_caught_up};
use anyhow::{Context, Result};

/// Format milliseconds into a human-readable duration string
fn format_duration(ms: i64) -> String {
    if ms < 1000 {
        format!("{}ms", ms)
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else if ms < 3_600_000 {
        let minutes = ms / 60_000;
        let seconds = (ms % 60_000) / 1000;
        format!("{}m {}s", minutes, seconds)
    } else {
        let hours = ms / 3_600_000;
        let minutes = (ms % 3_600_000) / 60_000;
        format!("{}h {}m", hours, minutes)
    }
}

/// Check replication status and display health information
///
/// This command performs Phase 4 of the migration process:
/// 1. Queries pg_stat_replication on source for replication lag
/// 2. Queries pg_stat_subscription on target for subscription status
/// 3. Displays health information in human-readable format
///
/// Provides real-time monitoring of replication health including:
/// - Replication lag (write/flush/replay)
/// - Subscription state
/// - Whether target is caught up with source
///
/// # Arguments
///
/// * `source_url` - PostgreSQL connection string for source (Neon) database
/// * `target_url` - PostgreSQL connection string for target (Seren) database
/// * `subscription_name` - Optional subscription name (defaults to "seren_migration_sub")
///
/// # Returns
///
/// Returns `Ok(())` after displaying status information.
///
/// # Errors
///
/// This function will return an error if:
/// - Cannot connect to source or target database
/// - Cannot query replication statistics
/// - Cannot query subscription status
///
/// # Examples
///
/// ```no_run
/// # use anyhow::Result;
/// # use postgres_seren_replicator::commands::status;
/// # async fn example() -> Result<()> {
/// status(
///     "postgresql://user:pass@neon.tech/sourcedb",
///     "postgresql://user:pass@seren.example.com/targetdb",
///     None  // Use default subscription name
/// ).await?;
/// # Ok(())
/// # }
/// ```
pub async fn status(
    source_url: &str,
    target_url: &str,
    subscription_name: Option<&str>,
) -> Result<()> {
    let sub_name = subscription_name.unwrap_or("seren_migration_sub");

    tracing::info!("Checking replication status...");
    tracing::info!("Subscription: '{}'", sub_name);
    tracing::info!("");

    // Connect to source database
    tracing::info!("Connecting to source database...");
    let source_client = connect(source_url)
        .await
        .context("Failed to connect to source database")?;

    // Connect to target database
    tracing::info!("Connecting to target database...");
    let target_client = connect(target_url)
        .await
        .context("Failed to connect to target database")?;

    // Query replication lag from source
    tracing::info!("Querying source replication statistics...");
    let source_stats = get_replication_lag(&source_client, Some(sub_name))
        .await
        .context("Failed to query replication lag from source")?;

    // Query subscription status from target
    tracing::info!("Querying target subscription statistics...");
    let target_stats = get_subscription_status(&target_client, Some(sub_name))
        .await
        .context("Failed to query subscription status from target")?;

    // Check if caught up
    let caught_up = is_replication_caught_up(&source_client, Some(sub_name))
        .await
        .unwrap_or(false);

    // Display results
    tracing::info!("");
    tracing::info!("========================================");
    tracing::info!("Replication Status Report");
    tracing::info!("========================================");
    tracing::info!("");

    if source_stats.is_empty() {
        tracing::warn!("⚠ No active replication found on source database");
        tracing::warn!("  This could mean:");
        tracing::warn!("  - Subscription is not connected");
        tracing::warn!("  - Subscription name '{}' does not exist", sub_name);
        tracing::warn!("  - Replication has not been set up yet");
        tracing::info!("");
    } else {
        for stat in &source_stats {
            tracing::info!("Source Replication Slot:");
            tracing::info!("  Application: {}", stat.application_name);
            tracing::info!("  State: {}", stat.state);
            tracing::info!("  Sent LSN: {}", stat.sent_lsn);
            tracing::info!("  Write LSN: {}", stat.write_lsn);
            tracing::info!("  Flush LSN: {}", stat.flush_lsn);
            tracing::info!("  Replay LSN: {}", stat.replay_lsn);

            if let Some(lag) = stat.replay_lag_ms {
                tracing::info!("  Replay Lag: {}", format_duration(lag));
            } else {
                tracing::info!("  Replay Lag: N/A");
            }

            if let Some(lag) = stat.flush_lag_ms {
                tracing::info!("  Flush Lag: {}", format_duration(lag));
            }

            if let Some(lag) = stat.write_lag_ms {
                tracing::info!("  Write Lag: {}", format_duration(lag));
            }

            tracing::info!("");
        }
    }

    if target_stats.is_empty() {
        tracing::warn!("⚠ No subscription found on target database");
        tracing::warn!("  Subscription '{}' may not exist", sub_name);
        tracing::info!("");
    } else {
        for stat in &target_stats {
            tracing::info!("Target Subscription:");
            tracing::info!("  Name: {}", stat.subscription_name);

            let state_str = match stat.state.as_str() {
                "i" => "Initializing",
                "d" => "Copying data",
                "s" => "Syncing",
                "r" => "Ready (streaming)",
                _ => &stat.state,
            };
            tracing::info!("  State: {}", state_str);

            if let Some(pid) = stat.pid {
                tracing::info!("  Worker PID: {}", pid);
            } else {
                tracing::info!("  Worker PID: Not running");
            }

            if let Some(lsn) = &stat.received_lsn {
                tracing::info!("  Received LSN: {}", lsn);
            }

            if let Some(lsn) = &stat.latest_end_lsn {
                tracing::info!("  Latest End LSN: {}", lsn);
            }

            tracing::info!("");
        }
    }

    // Overall health summary
    tracing::info!("========================================");
    if caught_up {
        tracing::info!("✓ Replication is CAUGHT UP (lag < 1s)");
        tracing::info!("  Your target database is fully in sync!");
    } else if source_stats.is_empty() || target_stats.is_empty() {
        tracing::warn!("✗ Replication is NOT ACTIVE");
        tracing::warn!("  Run 'sync' command to set up replication");
    } else {
        tracing::warn!("⚠ Replication is LAGGING");
        tracing::warn!("  Wait for replication to catch up before cutover");
    }
    tracing::info!("========================================");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(0), "0ms");
        assert_eq!(format_duration(500), "500ms");
        assert_eq!(format_duration(999), "999ms");
        assert_eq!(format_duration(1000), "1.0s");
        assert_eq!(format_duration(1500), "1.5s");
        assert_eq!(format_duration(59999), "60.0s");
        assert_eq!(format_duration(60000), "1m 0s");
        assert_eq!(format_duration(65000), "1m 5s");
        assert_eq!(format_duration(135000), "2m 15s");
        assert_eq!(format_duration(3600000), "1h 0m");
        assert_eq!(format_duration(3660000), "1h 1m");
    }

    #[tokio::test]
    #[ignore]
    async fn test_status_command() {
        // This test requires both source and target databases with active replication
        let source_url = std::env::var("TEST_SOURCE_URL").unwrap();
        let target_url = std::env::var("TEST_TARGET_URL").unwrap();

        let result = status(&source_url, &target_url, Some("seren_migration_sub")).await;

        match &result {
            Ok(_) => println!("✓ Status command completed successfully"),
            Err(e) => {
                println!("Error in status command: {:?}", e);
                // It's okay if replication is not set up yet
                if !e.to_string().contains("not supported") && !e.to_string().contains("permission")
                {
                    panic!("Unexpected error: {:?}", e);
                }
            }
        }

        assert!(result.is_ok(), "Status command failed: {:?}", result);
    }

    #[tokio::test]
    #[ignore]
    async fn test_status_with_defaults() {
        let source_url = std::env::var("TEST_SOURCE_URL").unwrap();
        let target_url = std::env::var("TEST_TARGET_URL").unwrap();

        let result = status(&source_url, &target_url, None).await;

        match &result {
            Ok(_) => println!("✓ Status with defaults completed successfully"),
            Err(e) => {
                println!("Error in status with defaults: {:?}", e);
            }
        }

        assert!(result.is_ok(), "Status with defaults failed: {:?}", result);
    }
}
