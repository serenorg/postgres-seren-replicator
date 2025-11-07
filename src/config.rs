// ABOUTME: Parses replication configuration files for table-level rules
// ABOUTME: Converts TOML format into TableRules structures

use crate::table_rules::TableRules;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

#[derive(Debug, Deserialize)]
struct ReplicationConfig {
    #[serde(default)]
    databases: HashMap<String, DatabaseConfig>,
}

#[derive(Debug, Deserialize, Default)]
struct DatabaseConfig {
    #[serde(default)]
    schema_only: Vec<String>,
    #[serde(default)]
    table_filters: Vec<TableFilterConfig>,
    #[serde(default)]
    time_filters: Vec<TimeFilterConfig>,
}

#[derive(Debug, Deserialize)]
struct TableFilterConfig {
    table: String,
    #[serde(rename = "where")]
    predicate: String,
}

#[derive(Debug, Deserialize)]
struct TimeFilterConfig {
    table: String,
    column: String,
    last: String,
}

pub fn load_table_rules_from_file(path: &str) -> Result<TableRules> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file at {}", path))?;
    let parsed: ReplicationConfig =
        toml::from_str(&raw).with_context(|| format!("Failed to parse TOML config at {}", path))?;

    let mut rules = TableRules::default();
    for (db_name, db) in parsed.databases {
        for table in db.schema_only {
            rules.add_schema_only_table(Some(db_name.clone()), table)?;
        }
        for filter in db.table_filters {
            rules.add_table_filter(Some(db_name.clone()), filter.table, filter.predicate)?;
        }
        for filter in db.time_filters {
            rules.add_time_filter(
                Some(db_name.clone()),
                filter.table,
                filter.column,
                filter.last,
            )?;
        }
    }

    Ok(rules)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn parse_sample_config() {
        let mut tmp = NamedTempFile::new().unwrap();
        let contents = r#"
            [databases.kong]
            schema_only = ["evmlog_strides", "price"]

            [[databases.kong.table_filters]]
            table = "output"
            where = "series_time >= NOW() - INTERVAL '6 months'"

            [[databases.kong.time_filters]]
            table = "metrics"
            column = "created_at"
            last = "1 year"
        "#;
        use std::io::Write;
        write!(tmp, "{}", contents).unwrap();

        let rules = load_table_rules_from_file(tmp.path().to_str().unwrap()).unwrap();
        assert_eq!(
            rules.schema_only_tables("kong"),
            vec!["evmlog_strides", "price"]
        );
        assert!(rules.table_filter("kong", "output").is_some());
        assert!(rules.time_filter("kong", "metrics").is_some());
    }
}
