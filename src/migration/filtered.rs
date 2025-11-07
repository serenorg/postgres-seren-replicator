// ABOUTME: Handles filtered table replication using COPY streaming
// ABOUTME: Applies table-level predicates and time filters during init snapshots

use crate::postgres;
use crate::utils::quote_ident;
use anyhow::{Context, Result};
use futures::{pin_mut, SinkExt, StreamExt};

pub async fn copy_filtered_tables(
    source_url: &str,
    target_url: &str,
    tables: &[(String, String)],
) -> Result<()> {
    if tables.is_empty() {
        return Ok(());
    }

    let source_client = postgres::connect(source_url)
        .await
        .context("Failed to connect to source database for filtered copy")?;
    let target_client = postgres::connect(target_url)
        .await
        .context("Failed to connect to target database for filtered copy")?;

    for (table, predicate) in tables {
        tracing::info!(
            "  Applying filtered copy for table '{}' with predicate: {}",
            table,
            predicate
        );

        let quoted_table = quote_ident(table);

        let truncate_sql = format!("TRUNCATE TABLE {}", quoted_table);
        target_client
            .execute(&truncate_sql, &[])
            .await
            .with_context(|| format!("Failed to truncate target table '{}'", table))?;

        let copy_out_sql = format!(
            "COPY (SELECT * FROM {} WHERE {}) TO STDOUT BINARY",
            quoted_table, predicate
        );
        let reader = source_client
            .copy_out(&copy_out_sql)
            .await
            .with_context(|| format!("Failed to copy data from source table '{}'", table))?;

        let copy_in_sql = format!("COPY {} FROM STDIN BINARY", quoted_table);
        let writer = target_client
            .copy_in(&copy_in_sql)
            .await
            .with_context(|| format!("Failed to copy data into target table '{}'", table))?;

        pin_mut!(reader);
        pin_mut!(writer);

        while let Some(chunk) = reader.next().await {
            let data = chunk?;
            writer.as_mut().send(data).await?;
        }

        writer.finish().await?;
        tracing::info!("  ✓ Filtered copy complete for '{}'", table);
    }

    Ok(())
}
