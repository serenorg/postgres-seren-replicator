// ABOUTME: Table-level replication rules for schema-only and filtered copies
// ABOUTME: Supports CLI/config inputs and deterministic fingerprints

use crate::utils;
use crate::utils::quote_ident;
use anyhow::{anyhow, bail, Context, Result};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeFilterRule {
    pub column: String,
    pub interval: String,
}

impl TimeFilterRule {
    fn predicate(&self) -> String {
        format!(
            "{} >= NOW() - INTERVAL '{}'",
            quote_ident(&self.column),
            self.interval
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableRuleKind {
    SchemaOnly,
    Predicate(String),
}

#[derive(Debug, Clone, Default)]
pub struct TableRules {
    schema_only: ScopedTableSet,
    table_filters: ScopedTableMap<String>,
    time_filters: ScopedTableMap<TimeFilterRule>,
}

type ScopedTableSet = BTreeMap<ScopeKey, BTreeSet<String>>;
type ScopedTableMap<V> = BTreeMap<ScopeKey, BTreeMap<String, V>>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum ScopeKey {
    Global,
    Database(String),
}

impl ScopeKey {
    fn from_option(value: Option<String>) -> Self {
        match value {
            Some(db) => ScopeKey::Database(db),
            None => ScopeKey::Global,
        }
    }

    fn database(database: &str) -> Self {
        ScopeKey::Database(database.to_string())
    }
}

impl TableRules {
    pub fn add_schema_only_table(&mut self, database: Option<String>, table: String) -> Result<()> {
        if let Some(ref db) = database {
            utils::validate_postgres_identifier(db)?;
        }
        utils::validate_postgres_identifier(&table)?;
        self.schema_only
            .entry(ScopeKey::from_option(database))
            .or_default()
            .insert(table);
        Ok(())
    }

    pub fn add_table_filter(
        &mut self,
        database: Option<String>,
        table: String,
        predicate: String,
    ) -> Result<()> {
        if let Some(ref db) = database {
            utils::validate_postgres_identifier(db)?;
        }
        utils::validate_postgres_identifier(&table)?;
        if predicate.trim().is_empty() {
            bail!("Table filter predicate cannot be empty for '{}'", table);
        }
        let scope = ScopeKey::from_option(database.clone());
        ensure_schema_only_free(
            &self.schema_only,
            database.as_deref(),
            &table,
            "table filter",
        )?;
        self.table_filters
            .entry(scope)
            .or_default()
            .insert(table, predicate);
        Ok(())
    }

    pub fn add_time_filter(
        &mut self,
        database: Option<String>,
        table: String,
        column: String,
        window: String,
    ) -> Result<()> {
        if let Some(ref db) = database {
            utils::validate_postgres_identifier(db)?;
        }
        utils::validate_postgres_identifier(&table)?;
        utils::validate_postgres_identifier(&column)?;
        let interval = normalize_time_window(&window)?;
        let scope = ScopeKey::from_option(database.clone());
        ensure_schema_only_free(
            &self.schema_only,
            database.as_deref(),
            &table,
            "time filter",
        )?;
        if self
            .table_filters
            .get(&scope)
            .and_then(|inner| inner.get(&table))
            .is_some()
        {
            bail!(
                "Cannot apply time filter to table '{}' because a table filter already exists",
                table
            );
        }
        self.time_filters
            .entry(scope)
            .or_default()
            .insert(table, TimeFilterRule { column, interval });
        Ok(())
    }

    pub fn apply_schema_only_cli(&mut self, specs: &[String]) -> Result<()> {
        for spec in specs {
            let (database, table) = parse_table_spec(spec)?;
            self.add_schema_only_table(database, table)?;
        }
        Ok(())
    }

    pub fn apply_table_filter_cli(&mut self, specs: &[String]) -> Result<()> {
        for spec in specs {
            let (table_part, predicate) = spec
                .split_once(':')
                .with_context(|| format!("Table filter '{}' missing ':' separator", spec))?;
            if predicate.trim().is_empty() {
                bail!("Table filter '{}' must include a predicate after ':'", spec);
            }
            let (database, table) = parse_table_spec(table_part)?;
            self.add_table_filter(database, table, predicate.trim().to_string())?;
        }
        Ok(())
    }

    pub fn apply_time_filter_cli(&mut self, specs: &[String]) -> Result<()> {
        for spec in specs {
            let (table_part, rest) = spec
                .split_once(':')
                .with_context(|| format!("Time filter '{}' missing second ':'", spec))?;
            let (column, window) = rest
                .split_once(':')
                .with_context(|| format!("Time filter '{}' must be table:column:window", spec))?;
            if column.trim().is_empty() || window.trim().is_empty() {
                bail!(
                    "Time filter '{}' must include non-empty column and window",
                    spec
                );
            }
            let (database, table) = parse_table_spec(table_part)?;
            self.add_time_filter(
                database,
                table,
                column.trim().to_string(),
                window.trim().to_string(),
            )?;
        }
        Ok(())
    }

    pub fn schema_only_tables(&self, database: &str) -> Vec<String> {
        collect_tables(&self.schema_only, database)
    }

    pub fn table_filter(&self, database: &str, table: &str) -> Option<&String> {
        lookup_scoped(&self.table_filters, database, table)
    }

    pub fn time_filter(&self, database: &str, table: &str) -> Option<&TimeFilterRule> {
        lookup_scoped(&self.time_filters, database, table)
    }

    pub fn predicate_tables(&self, database: &str) -> Vec<(String, String)> {
        let schema_only: BTreeSet<String> = self.schema_only_tables(database).into_iter().collect();
        let mut combined = BTreeMap::new();

        for (table, predicate) in scoped_map_values(&self.table_filters, database) {
            if schema_only.contains(&table) {
                continue;
            }
            combined.insert(table, predicate);
        }

        for (table, rule) in scoped_map_values(&self.time_filters, database) {
            if schema_only.contains(&table) || combined.contains_key(&table) {
                continue;
            }
            combined.insert(table.clone(), rule.predicate());
        }

        combined.into_iter().collect()
    }

    pub fn rule_for_table(&self, database: &str, table: &str) -> Option<TableRuleKind> {
        if has_schema_only_rule(&self.schema_only, database, table) {
            return Some(TableRuleKind::SchemaOnly);
        }
        if let Some(predicate) = self.table_filter(database, table) {
            return Some(TableRuleKind::Predicate(predicate.clone()));
        }
        if let Some(rule) = self.time_filter(database, table) {
            return Some(TableRuleKind::Predicate(rule.predicate()));
        }
        None
    }

    pub fn merge(&mut self, other: TableRules) {
        merge_sets(&mut self.schema_only, other.schema_only);
        merge_maps(&mut self.table_filters, other.table_filters);
        merge_maps(&mut self.time_filters, other.time_filters);
    }

    pub fn fingerprint(&self) -> String {
        let mut hasher = Sha256::new();
        hash_scoped_set(&mut hasher, &self.schema_only);
        hash_scoped_map(&mut hasher, &self.table_filters, |value| value.clone());
        hash_scoped_map(&mut hasher, &self.time_filters, |value| {
            format!("{}|{}", value.column, value.interval)
        });
        format!("{:x}", hasher.finalize())
    }

    pub fn is_empty(&self) -> bool {
        self.schema_only.is_empty() && self.table_filters.is_empty() && self.time_filters.is_empty()
    }
}

fn parse_table_spec(input: &str) -> Result<(Option<String>, String)> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        bail!("Table specification cannot be empty");
    }

    let (database, table) = if let Some((db, tbl)) = trimmed.split_once('.') {
        let db_name = non_empty(db, "database")?;
        utils::validate_postgres_identifier(&db_name)?;
        let table_name = non_empty(tbl, "table")?;
        utils::validate_postgres_identifier(&table_name)?;
        (Some(db_name), table_name)
    } else {
        let table_name = non_empty(trimmed, "table")?;
        utils::validate_postgres_identifier(&table_name)?;
        (None, table_name)
    };

    Ok((database, table))
}

fn non_empty(value: &str, label: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("{} name cannot be empty", label);
    }
    Ok(trimmed.to_string())
}

fn collect_tables(map: &ScopedTableSet, database: &str) -> Vec<String> {
    let mut tables = BTreeSet::new();
    if let Some(global) = map.get(&ScopeKey::Global) {
        tables.extend(global.iter().cloned());
    }
    let scoped = ScopeKey::database(database);
    if let Some(specific) = map.get(&scoped) {
        tables.extend(specific.iter().cloned());
    }
    tables.into_iter().collect()
}

fn lookup_scoped<'a, V>(map: &'a ScopedTableMap<V>, database: &str, table: &str) -> Option<&'a V> {
    let scoped = ScopeKey::database(database);
    map.get(&scoped)
        .and_then(|inner| inner.get(table))
        .or_else(|| {
            map.get(&ScopeKey::Global)
                .and_then(|inner| inner.get(table))
        })
}

fn scoped_map_values<V: Clone>(map: &ScopedTableMap<V>, database: &str) -> BTreeMap<String, V> {
    let mut values = BTreeMap::new();
    if let Some(global) = map.get(&ScopeKey::Global) {
        for (table, value) in global {
            values.insert(table.clone(), value.clone());
        }
    }
    if let Some(specific) = map.get(&ScopeKey::database(database)) {
        for (table, value) in specific {
            values.insert(table.clone(), value.clone());
        }
    }
    values
}

fn has_schema_only_rule(schema_only: &ScopedTableSet, database: &str, table: &str) -> bool {
    schema_only
        .get(&ScopeKey::Global)
        .is_some_and(|set| set.contains(table))
        || schema_only
            .get(&ScopeKey::database(database))
            .is_some_and(|set| set.contains(table))
}

fn ensure_schema_only_free(
    schema_only: &ScopedTableSet,
    database: Option<&str>,
    table: &str,
    rule_name: &str,
) -> Result<()> {
    if schema_only
        .get(&ScopeKey::Global)
        .is_some_and(|set| set.contains(table))
    {
        bail!(
            "Cannot apply {} to table '{}' because it is marked schema-only globally",
            rule_name,
            table
        );
    }
    if let Some(db) = database {
        if schema_only
            .get(&ScopeKey::database(db))
            .is_some_and(|set| set.contains(table))
        {
            bail!(
                "Cannot apply {} to table '{}' in database '{}' because it is schema-only",
                rule_name,
                table,
                db
            );
        }
    }
    Ok(())
}

fn normalize_time_window(window: &str) -> Result<String> {
    let trimmed = window.trim();
    let mut parts = trimmed.split_whitespace();
    let amount_str = parts
        .next()
        .ok_or_else(|| anyhow!("Time filter window '{}' missing amount", window))?;
    let unit_str = parts
        .next()
        .ok_or_else(|| anyhow!("Time filter window '{}' missing unit", window))?;
    if parts.next().is_some() {
        bail!("Time filter window '{}' must be '<amount> <unit>'", window);
    }

    let amount: i64 = amount_str.parse().with_context(|| {
        format!(
            "Invalid time window amount '{}': must be integer",
            amount_str
        )
    })?;
    if amount <= 0 {
        bail!("Time window amount must be positive, got {}", amount);
    }

    let unit = match unit_str.to_lowercase().as_str() {
        "second" | "seconds" | "sec" | "secs" => "second",
        "minute" | "minutes" | "min" | "mins" => "minute",
        "hour" | "hours" | "hr" | "hrs" => "hour",
        "day" | "days" => "day",
        "week" | "weeks" => "week",
        "month" | "months" | "mon" | "mons" => "month",
        "year" | "years" | "yr" | "yrs" => "year",
        other => bail!(
            "Unsupported time window unit '{}'. Use seconds/minutes/hours/days/weeks/months/years",
            other
        ),
    };

    Ok(format!("{} {}", amount, unit))
}

fn merge_sets(target: &mut ScopedTableSet, source: ScopedTableSet) {
    for (scope, tables) in source {
        target.entry(scope).or_default().extend(tables);
    }
}

fn merge_maps<V: Clone>(target: &mut ScopedTableMap<V>, source: ScopedTableMap<V>) {
    for (scope, tables) in source {
        let entry = target.entry(scope).or_default();
        for (table, value) in tables {
            entry.insert(table, value);
        }
    }
}

fn hash_scoped_set(hasher: &mut Sha256, data: &ScopedTableSet) {
    for (scope, tables) in data {
        hash_scope_label(hasher, scope);
        for table in tables {
            hasher.update(table.as_bytes());
            hasher.update(b"|");
        }
    }
}

fn hash_scoped_map<V, F>(hasher: &mut Sha256, data: &ScopedTableMap<V>, mut encode: F)
where
    F: FnMut(&V) -> String,
{
    for (scope, tables) in data {
        hash_scope_label(hasher, scope);
        for (table, value) in tables {
            hasher.update(table.as_bytes());
            hasher.update(b"=");
            hasher.update(encode(value).as_bytes());
            hasher.update(b"|");
        }
    }
}

fn hash_scope_label(hasher: &mut Sha256, scope: &ScopeKey) {
    match scope {
        ScopeKey::Database(db) => {
            hasher.update(b"db:");
            hasher.update(db.as_bytes());
        }
        ScopeKey::Global => {
            hasher.update(b"global");
        }
    }
    hasher.update(b"#");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_schema_only_parsing() {
        let mut rules = TableRules::default();
        rules
            .apply_schema_only_cli(&["db1.orders".to_string(), "invoices".to_string()])
            .unwrap();
        assert_eq!(rules.schema_only_tables("db1"), vec!["invoices", "orders"]);
        assert_eq!(rules.schema_only_tables("db2"), vec!["invoices"]);
    }

    #[test]
    fn cli_table_filter_parsing() {
        let mut rules = TableRules::default();
        rules
            .apply_table_filter_cli(&["db1.logs:created_at > NOW() - INTERVAL '1 day'".into()])
            .unwrap();
        assert!(rules
            .table_filter("db1", "logs")
            .unwrap()
            .contains("created_at"));
    }

    #[test]
    fn cli_time_filter_parsing() {
        let mut rules = TableRules::default();
        rules
            .apply_time_filter_cli(&["metrics:created_at:6 months".into()])
            .unwrap();
        let tf = rules.time_filter("any", "metrics").unwrap();
        assert_eq!(tf.column, "created_at");
        assert_eq!(tf.interval, "6 month");
    }

    #[test]
    fn fingerprint_changes_with_rules() {
        let mut rules_a = TableRules::default();
        rules_a
            .apply_schema_only_cli(&["db.table".to_string()])
            .unwrap();
        let mut rules_b = TableRules::default();
        assert_ne!(rules_a.fingerprint(), rules_b.fingerprint());
        rules_b
            .apply_schema_only_cli(&["db.table".to_string()])
            .unwrap();
        assert_eq!(rules_a.fingerprint(), rules_b.fingerprint());
    }

    #[test]
    fn schema_only_conflicts_with_filters() {
        let mut rules = TableRules::default();
        rules
            .apply_schema_only_cli(&["db1.audit".to_string()])
            .unwrap();
        assert!(rules
            .apply_table_filter_cli(&["db1.audit:1=1".to_string()])
            .is_err());
    }

    #[test]
    fn predicate_tables_include_time_filters() {
        let mut rules = TableRules::default();
        rules
            .apply_time_filter_cli(&["db1.metrics:created_at:6 months".into()])
            .unwrap();
        let predicates = rules.predicate_tables("db1");
        assert_eq!(predicates.len(), 1);
        assert!(predicates[0].1.contains("INTERVAL '6 month'"));
    }
}
