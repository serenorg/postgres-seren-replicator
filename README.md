# postgres-seren-replicator

[![CI](https://github.com/serenorg/postgres-seren-replicator/actions/workflows/ci.yml/badge.svg)](https://github.com/serenorg/postgres-seren-replicator/actions/workflows/ci.yml)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Rust Version](https://img.shields.io/badge/rust-1.70%2B-blue.svg)](https://www.rust-lang.org)
[![Latest Release](https://img.shields.io/github/v/release/serenorg/postgres-seren-replicator)](https://github.com/serenorg/postgres-seren-replicator/releases)

Zero-downtime PostgreSQL replication tool from PostgreSQL to Seren with continuous sync and real-time monitoring.

## Overview

This tool enables safe, zero-downtime replication of PostgreSQL databases from any PostgreSQL provider (Neon, AWS RDS, Hetzner, self-hosted, etc.) to Seren Cloud. It uses PostgreSQL's logical replication for continuous data synchronization with real-time monitoring and supports selective replication for fine-grained control over what gets replicated.

## Features

- **Zero Downtime**: Uses logical replication to keep databases continuously in sync
- **Selective Replication**: Choose specific databases and tables to replicate
- **Interactive Mode**: User-friendly terminal UI for selecting what to replicate
- **Multi-Provider Support**: Works with any PostgreSQL provider (Neon, AWS RDS, Hetzner, self-hosted, etc.)
- **Size Estimation**: Analyze database sizes and view estimated replication times before starting
- **High Performance**: Parallel dump/restore with automatic CPU core detection
- **Optimized Compression**: Maximum compression (level 9) for faster transfers
- **Large Object Support**: Handles BLOBs and large binary objects efficiently
- **Complete Replication**: Replicates schema, data, roles, and permissions
- **Data Validation**: Checksum-based verification of data integrity
- **Real-time Monitoring**: Track replication lag and status continuously
- **Safe & Fail-fast**: Validates prerequisites before starting replication

## Replication Workflow

The replication process follows 5 phases:

1. **Validate** - Check source and target databases meet replication requirements
2. **Init** - Perform initial snapshot replication (schema + data) using pg_dump/restore
3. **Sync** - Set up continuous logical replication between databases
4. **Status** - Monitor replication lag and health in real-time
5. **Verify** - Validate data integrity with checksums

## Installation

### Prerequisites

- PostgreSQL client tools (pg_dump, pg_dumpall, psql)
- Access to both source and target databases with appropriate permissions

### Download Pre-built Binaries

Download the latest release for your platform from [GitHub Releases](https://github.com/serenorg/postgres-seren-replicator/releases/latest):

- **Linux (x64)**: `postgres-seren-replicator-linux-x64-binary`
- **macOS (Intel)**: `postgres-seren-replicator-macos-x64-binary`
- **macOS (Apple Silicon)**: `postgres-seren-replicator-macos-arm64-binary`

Make the binary executable:

```bash
chmod +x postgres-seren-replicator-*-binary
./postgres-seren-replicator-*-binary --help
```

### Build from Source

Requires Rust 1.70 or later:

```bash
git clone https://github.com/serenorg/postgres-seren-replicator.git
cd postgres-seren-replicator
cargo build --release
```

The binary will be available at `target/release/postgres-seren-replicator`.

## Usage

### 1. Validate Databases

Check that both databases meet replication requirements:

```bash
./postgres-seren-replicator validate \
  --source "postgresql://user:pass@source-host:5432/db" \
  --target "postgresql://user:pass@seren-host:5432/db"
```

### 2. Initialize Replication

Perform initial snapshot replication. The tool will first analyze database sizes and show estimated replication times:

```bash
./postgres-seren-replicator init \
  --source "postgresql://user:pass@source-host:5432/db" \
  --target "postgresql://user:pass@seren-host:5432/db"
```

Example output:
```
Analyzing database sizes...

Database             Size         Est. Time
──────────────────────────────────────────────────
myapp               15.0 GB      ~45.0 minutes
analytics           250.0 GB     ~12.5 hours
staging             2.0 GB       ~6.0 minutes
──────────────────────────────────────────────────
Total: 267.0 GB (estimated ~13.3 hours)

Proceed with replication? [y/N]:
```

For automated scripts, skip the confirmation prompt with `--yes` or `-y`:

```bash
./postgres-seren-replicator init \
  --source "postgresql://user:pass@source-host:5432/db" \
  --target "postgresql://user:pass@seren-host:5432/db" \
  --yes
```

If the target database already exists, you can drop and recreate it with `--drop-existing`:

```bash
./postgres-seren-replicator init \
  --source "postgresql://user:pass@source-host:5432/db" \
  --target "postgresql://user:pass@seren-host:5432/db" \
  --drop-existing
```

### 3. Set Up Continuous Replication

Enable logical replication for ongoing change synchronization:

```bash
./postgres-seren-replicator sync \
  --source "postgresql://user:pass@source-host:5432/db" \
  --target "postgresql://user:pass@seren-host:5432/db"
```

### 4. Monitor Replication Status

Check replication health and lag in real-time:

```bash
./postgres-seren-replicator status \
  --source "postgresql://user:pass@source-host:5432/db" \
  --target "postgresql://user:pass@seren-host:5432/db"
```

### 5. Verify Data Integrity

Validate that all tables match:

```bash
./postgres-seren-replicator verify \
  --source "postgresql://user:pass@source-host:5432/db" \
  --target "postgresql://user:pass@seren-host:5432/db"
```

## Selective Replication

Selective replication allows you to choose exactly which databases and tables to replicate, giving you fine-grained control over your migration.

### Database-Level Filtering

Replicate only specific databases:

```bash
# Include only specific databases
./postgres-seren-replicator init \
  --source "postgresql://user:pass@source-host:5432/postgres" \
  --target "postgresql://user:pass@seren-host:5432/postgres" \
  --include-databases "myapp,analytics"

# Exclude specific databases
./postgres-seren-replicator init \
  --source "postgresql://user:pass@source-host:5432/postgres" \
  --target "postgresql://user:pass@seren-host:5432/postgres" \
  --exclude-databases "test,staging"
```

### Table-Level Filtering

Replicate only specific tables or exclude certain tables:

```bash
# Include only specific tables (format: database.table)
./postgres-seren-replicator init \
  --source "postgresql://user:pass@source-host:5432/postgres" \
  --target "postgresql://user:pass@seren-host:5432/postgres" \
  --include-tables "myapp.users,myapp.orders,analytics.events"

# Exclude specific tables
./postgres-seren-replicator init \
  --source "postgresql://user:pass@source-host:5432/postgres" \
  --target "postgresql://user:pass@seren-host:5432/postgres" \
  --exclude-tables "myapp.logs,myapp.cache,analytics.temp_data"
```

### Combined Filtering

Combine database and table filtering for precise control:

```bash
# Replicate specific databases but exclude certain tables
./postgres-seren-replicator init \
  --source "postgresql://user:pass@source-host:5432/postgres" \
  --target "postgresql://user:pass@seren-host:5432/postgres" \
  --include-databases "myapp,analytics" \
  --exclude-tables "myapp.logs,analytics.temp_data"
```

### Filtering with Other Commands

Filtering works with all commands that support it:

```bash
# Validate with filtering
./postgres-seren-replicator validate \
  --source "postgresql://user:pass@source-host:5432/postgres" \
  --target "postgresql://user:pass@seren-host:5432/postgres" \
  --include-databases "myapp"

# Sync with filtering
./postgres-seren-replicator sync \
  --source "postgresql://user:pass@source-host:5432/postgres" \
  --target "postgresql://user:pass@seren-host:5432/postgres" \
  --include-databases "myapp,analytics" \
  --exclude-tables "myapp.logs"

# Status with filtering
./postgres-seren-replicator status \
  --source "postgresql://user:pass@source-host:5432/postgres" \
  --target "postgresql://user:pass@seren-host:5432/postgres" \
  --include-databases "myapp"

# Verify with filtering
./postgres-seren-replicator verify \
  --source "postgresql://user:pass@source-host:5432/postgres" \
  --target "postgresql://user:pass@seren-host:5432/postgres" \
  --include-databases "myapp" \
  --exclude-tables "myapp.logs"
```

## Interactive Mode

Interactive mode provides a user-friendly terminal UI for selecting databases and tables to replicate. This is ideal for exploratory migrations or when you're not sure exactly what you want to replicate.

### Using Interactive Mode

Add the `--interactive` flag to `init`, `validate`, or `sync` commands:

```bash
./postgres-seren-replicator init \
  --source "postgresql://user:pass@source-host:5432/postgres" \
  --target "postgresql://user:pass@seren-host:5432/postgres" \
  --interactive
```

### Interactive Workflow

1. **Select Databases**: A multi-select checklist shows all available databases. Use arrow keys to navigate, space to select, and enter to confirm.

2. **Select Tables to Exclude** (optional): For each selected database, you can optionally exclude specific tables. If you don't want to exclude any tables, just press enter.

3. **Review Configuration**: The tool shows a summary of what will be replicated, including:
   - Databases to replicate
   - Tables to exclude (if any)

4. **Confirm**: You'll be asked to confirm before proceeding.

### Example Interactive Session

```
Connecting to source database...
✓ Connected to source

Discovering databases on source...
✓ Found 4 database(s)

Select databases to replicate:
(Use arrow keys to navigate, Space to select, Enter to confirm)

> [x] myapp
  [x] analytics
  [ ] staging
  [ ] test

✓ Selected 2 database(s):
  - myapp
  - analytics

Discovering tables in database 'myapp'...
✓ Found 15 table(s) in 'myapp'

Select tables to EXCLUDE from 'myapp' (or press Enter to include all):
(Use arrow keys to navigate, Space to select, Enter to confirm)

  [ ] users
  [ ] orders
  [x] logs
  [x] cache
  [ ] products

✓ Excluding 2 table(s) from 'myapp':
  - myapp.logs
  - myapp.cache

========================================
Replication Configuration Summary
========================================

Databases to replicate: 2
  ✓ myapp
  ✓ analytics

Tables to exclude: 2
  ✗ myapp.logs
  ✗ myapp.cache

========================================

Proceed with this configuration? [Y/n]:
```

## Multi-Provider Support

The tool works seamlessly with any PostgreSQL-compatible database provider. Here are examples for common providers:

### Neon

```bash
./postgres-seren-replicator init \
  --source "postgresql://user:pass@ep-cool-name-123456.us-east-2.aws.neon.tech/mydb" \
  --target "postgresql://user:pass@seren-host:5432/mydb"
```

### AWS RDS

```bash
./postgres-seren-replicator init \
  --source "postgresql://user:pass@mydb.abc123.us-east-1.rds.amazonaws.com:5432/mydb" \
  --target "postgresql://user:pass@seren-host:5432/mydb"
```

### Hetzner Cloud

```bash
./postgres-seren-replicator init \
  --source "postgresql://user:pass@postgres-server.hetzner-cloud.de:5432/mydb" \
  --target "postgresql://user:pass@seren-host:5432/mydb"
```

### Self-Hosted PostgreSQL

```bash
./postgres-seren-replicator init \
  --source "postgresql://user:pass@192.168.1.100:5432/mydb" \
  --target "postgresql://user:pass@seren-host:5432/mydb"
```

### Provider-Specific Considerations

#### Connection Parameters

All providers support standard PostgreSQL connection strings. Add SSL/TLS parameters as needed:

```bash
# With SSL mode
--source "postgresql://user:pass@host:5432/db?sslmode=require"

# With SSL and certificate verification
--source "postgresql://user:pass@host:5432/db?sslmode=verify-full&sslrootcert=/path/to/ca.crt"
```

#### Privileges

Ensure your source database user has the required privileges:

```sql
-- On source (works for all providers)
ALTER USER myuser WITH REPLICATION;
GRANT USAGE ON SCHEMA public TO myuser;
GRANT SELECT ON ALL TABLES IN SCHEMA public TO myuser;
```

#### Provider Limitations

- **AWS RDS**: Requires `rds_replication` role for logical replication
- **Neon**: Full support for logical replication out of the box
- **Hetzner**: Standard PostgreSQL, full support
- **Self-hosted**: Full control, ensure `wal_level = logical` in postgresql.conf

## Testing

### Unit Tests

Run unit tests:

```bash
cargo test
```

### Integration Tests

Integration tests require real database connections. Set environment variables:

```bash
export TEST_SOURCE_URL="postgresql://user:pass@source-host:5432/db"
export TEST_TARGET_URL="postgresql://user:pass@target-host:5432/db"
```

Run integration tests:

```bash
# Run all integration tests
cargo test --test integration_test -- --ignored

# Run specific integration test
cargo test --test integration_test test_validate_command_integration -- --ignored

# Run full workflow test (read-only by default)
cargo test --test integration_test test_full_migration_workflow -- --ignored
```

**Note**: Some integration tests (init, sync) are commented out by default because they perform destructive operations. Uncomment them in `tests/integration_test.rs` to test the full workflow.

### Test Environment Setup

For local testing, you can use Docker to run PostgreSQL instances:

```bash
# Source database
docker run -d --name pg-source \
  -e POSTGRES_PASSWORD=postgres \
  -p 5432:5432 \
  postgres:17

# Target database
docker run -d --name pg-target \
  -e POSTGRES_PASSWORD=postgres \
  -p 5433:5432 \
  postgres:17

# Set test environment variables
export TEST_SOURCE_URL="postgresql://postgres:postgres@localhost:5432/postgres"
export TEST_TARGET_URL="postgresql://postgres:postgres@localhost:5433/postgres"
```

## Requirements

### Source Database

- PostgreSQL 12 or later
- Replication privilege (`REPLICATION` role attribute)
- Ability to create publications

### Target Database (Seren)

- PostgreSQL 12 or later
- Superuser or database owner privileges
- Ability to create subscriptions
- Network connectivity to source database

## Performance Optimizations

The tool uses several optimizations for fast, efficient database replication:

### Parallel Operations

- **Auto-detected parallelism**: Automatically uses up to 8 parallel workers based on CPU cores
- **Parallel dump**: pg_dump with `--jobs` flag for concurrent table exports
- **Parallel restore**: pg_restore with `--jobs` flag for concurrent table imports
- **Directory format**: Uses PostgreSQL directory format to enable parallel operations

### Compression

- **Maximum compression**: Level 9 compression for smaller dump sizes
- **Faster transfers**: Reduced network bandwidth and storage requirements
- **Per-file compression**: Each table compressed independently for parallel efficiency

### Large Objects

- **Blob support**: Includes large objects (BLOBs) with `--blobs` flag
- **Binary data**: Handles images, documents, and other binary data efficiently

These optimizations can significantly reduce replication time, especially for large databases with many tables.

## Architecture

- **src/commands/** - CLI command implementations
- **src/postgres/** - PostgreSQL connection and utilities
- **src/migration/** - Schema introspection, dump/restore, checksums
- **src/replication/** - Logical replication management
- **tests/** - Integration tests

## Troubleshooting

### "Permission denied" errors

Ensure your user has the required privileges:

```sql
-- On source (Neon)
ALTER USER myuser WITH REPLICATION;

-- On target (Seren)
ALTER USER myuser WITH SUPERUSER;
```

### "Publication already exists"

The tool handles existing publications gracefully. If you need to start over:

```sql
-- On target
DROP SUBSCRIPTION IF EXISTS seren_replication_sub;

-- On source
DROP PUBLICATION IF EXISTS seren_replication_pub;
```

### Replication lag

Check status frequently during replication:

```bash
# Monitor until lag < 1 second
watch -n 5 './postgres-seren-replicator status --source "$SOURCE" --target "$TARGET"'
```

## License

This project is licensed under the Apache License 2.0 - see the [LICENSE](LICENSE) file for details.
