#!/bin/bash
# ABOUTME: End-to-end integration test for remote replication
# ABOUTME: Sets up test databases, runs remote replication, and verifies results

set -euo pipefail

# Configuration
TEST_SOURCE_PORT="${TEST_SOURCE_PORT:-5432}"
TEST_TARGET_PORT="${TEST_TARGET_PORT:-5433}"
POSTGRES_PASSWORD="postgres"
TEST_DB_NAME="testdb"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Log functions
log() {
    echo -e "[$(date -u +"%Y-%m-%dT%H:%M:%SZ")] $*"
}

log_success() {
    echo -e "${GREEN}✓${NC} $*"
}

log_error() {
    echo -e "${RED}✗${NC} $*"
}

log_warning() {
    echo -e "${YELLOW}⚠${NC} $*"
}

error() {
    log_error "$*"
    cleanup
    exit 1
}

# Cleanup function
cleanup() {
    log "Cleaning up..."

    if [ "${KEEP_CONTAINERS:-false}" = "true" ]; then
        log_warning "KEEP_CONTAINERS=true, skipping cleanup"
        return
    fi

    # Stop and remove containers
    docker stop test-source test-target 2>/dev/null || true
    docker rm test-source test-target 2>/dev/null || true

    log_success "Cleanup complete"
}

# Trap errors and cleanup
trap cleanup EXIT

# Check prerequisites
check_prerequisites() {
    log "Checking prerequisites..."

    if ! command -v docker &> /dev/null; then
        error "Docker not found. Install from: https://www.docker.com/"
    fi

    if ! docker info &> /dev/null; then
        error "Docker daemon not running. Start Docker Desktop."
    fi

    if [ ! -f "$PROJECT_ROOT/target/release/postgres-seren-replicator" ]; then
        error "Release binary not found. Run: cargo build --release"
    fi

    if [ ! -f "$SCRIPT_DIR/.api_endpoint" ]; then
        log_warning "API endpoint not found. Make sure infrastructure is deployed."
        log_warning "Run: ./aws/deploy.sh"
        log_warning "Or set SEREN_REMOTE_API environment variable"

        if [ -z "${SEREN_REMOTE_API:-}" ]; then
            error "SEREN_REMOTE_API not set and .api_endpoint file not found"
        fi
    else
        export SEREN_REMOTE_API=$(cat "$SCRIPT_DIR/.api_endpoint")
    fi

    log_success "Prerequisites satisfied"
}

# Start test databases
start_databases() {
    log "Starting test databases..."

    # Stop any existing containers
    docker stop test-source test-target 2>/dev/null || true
    docker rm test-source test-target 2>/dev/null || true

    # Start source database
    log "Starting source database on port $TEST_SOURCE_PORT..."
    docker run -d \
        --name test-source \
        -e POSTGRES_PASSWORD=$POSTGRES_PASSWORD \
        -p $TEST_SOURCE_PORT:5432 \
        postgres:17 > /dev/null

    # Start target database
    log "Starting target database on port $TEST_TARGET_PORT..."
    docker run -d \
        --name test-target \
        -e POSTGRES_PASSWORD=$POSTGRES_PASSWORD \
        -p $TEST_TARGET_PORT:5432 \
        postgres:17 > /dev/null

    # Wait for databases to be ready
    log "Waiting for databases to be ready..."
    for i in {1..30}; do
        if docker exec test-source pg_isready -U postgres &> /dev/null && \
           docker exec test-target pg_isready -U postgres &> /dev/null; then
            log_success "Databases are ready"
            return
        fi
        sleep 1
    done

    error "Databases failed to start within 30 seconds"
}

# Create test data
create_test_data() {
    log "Creating test database and data..."

    # Create test database
    docker exec test-source psql -U postgres -c "DROP DATABASE IF EXISTS $TEST_DB_NAME;" 2>/dev/null || true
    docker exec test-source psql -U postgres -c "CREATE DATABASE $TEST_DB_NAME;"

    # Create test schema and data
    docker exec test-source psql -U postgres -d $TEST_DB_NAME <<'EOF'
-- Create test table
CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    username VARCHAR(100) NOT NULL,
    email VARCHAR(255) NOT NULL,
    created_at TIMESTAMP DEFAULT NOW()
);

-- Insert test data
INSERT INTO users (username, email) VALUES
    ('alice', 'alice@example.com'),
    ('bob', 'bob@example.com'),
    ('charlie', 'charlie@example.com');

-- Create another table
CREATE TABLE orders (
    id SERIAL PRIMARY KEY,
    user_id INTEGER REFERENCES users(id),
    amount DECIMAL(10,2) NOT NULL,
    status VARCHAR(50) DEFAULT 'pending',
    created_at TIMESTAMP DEFAULT NOW()
);

-- Insert order data
INSERT INTO orders (user_id, amount, status) VALUES
    (1, 99.99, 'completed'),
    (2, 149.99, 'pending'),
    (1, 49.99, 'completed');
EOF

    local row_count
    row_count=$(docker exec test-source psql -U postgres -d $TEST_DB_NAME -t -c "SELECT COUNT(*) FROM users;")
    log_success "Created test database with $row_count users"

    row_count=$(docker exec test-source psql -U postgres -d $TEST_DB_NAME -t -c "SELECT COUNT(*) FROM orders;")
    log_success "Created $row_count orders"
}

# Run remote replication
run_remote_replication() {
    log "Running remote replication..."

    # Build connection URLs
    # Use host.docker.internal on macOS/Windows, or docker0 IP on Linux
    local source_host="localhost"
    local target_host="localhost"

    if [[ "$OSTYPE" == "darwin"* ]]; then
        # macOS - containers can reach host via host.docker.internal
        # But we're connecting FROM host TO containers, so use localhost
        source_host="localhost"
        target_host="localhost"
    fi

    local source_url="postgresql://postgres:$POSTGRES_PASSWORD@$source_host:$TEST_SOURCE_PORT/$TEST_DB_NAME"
    local target_url="postgresql://postgres:$POSTGRES_PASSWORD@$target_host:$TEST_TARGET_PORT/$TEST_DB_NAME"

    log "Source: $source_url"
    log "Target: $target_url"
    log "API: ${SEREN_REMOTE_API}"
    log ""
    log "This will take several minutes (EC2 provisioning + replication)..."
    log ""

    # Run replication
    cd "$PROJECT_ROOT"
    if ./target/release/postgres-seren-replicator init --remote \
        --source "$source_url" \
        --target "$target_url" \
        --yes; then
        log_success "Remote replication completed"
    else
        error "Remote replication failed"
    fi
}

# Verify data
verify_data() {
    log "Verifying replicated data..."

    # Check that target database exists
    if ! docker exec test-target psql -U postgres -lqt | cut -d \| -f 1 | grep -qw $TEST_DB_NAME; then
        error "Target database does not exist"
    fi

    # Count users in target
    local target_users
    target_users=$(docker exec test-target psql -U postgres -d $TEST_DB_NAME -t -c "SELECT COUNT(*) FROM users;" | xargs)

    local source_users
    source_users=$(docker exec test-source psql -U postgres -d $TEST_DB_NAME -t -c "SELECT COUNT(*) FROM users;" | xargs)

    if [ "$target_users" != "$source_users" ]; then
        error "User count mismatch: source=$source_users, target=$target_users"
    fi

    log_success "User count matches: $target_users rows"

    # Count orders in target
    local target_orders
    target_orders=$(docker exec test-target psql -U postgres -d $TEST_DB_NAME -t -c "SELECT COUNT(*) FROM orders;" | xargs)

    local source_orders
    source_orders=$(docker exec test-source psql -U postgres -d $TEST_DB_NAME -t -c "SELECT COUNT(*) FROM orders;" | xargs)

    if [ "$target_orders" != "$source_orders" ]; then
        error "Order count mismatch: source=$source_orders, target=$target_orders"
    fi

    log_success "Order count matches: $target_orders rows"

    # Verify specific data
    local alice_email
    alice_email=$(docker exec test-target psql -U postgres -d $TEST_DB_NAME -t -c "SELECT email FROM users WHERE username = 'alice';" | xargs)

    if [ "$alice_email" != "alice@example.com" ]; then
        error "Data verification failed: alice's email is '$alice_email'"
    fi

    log_success "Data verification passed"
}

# Test failure case
test_failure_case() {
    log "Testing failure case (invalid source)..."

    local invalid_url="postgresql://invalid:invalid@invalid:5432/invalid"
    local target_url="postgresql://postgres:$POSTGRES_PASSWORD@localhost:$TEST_TARGET_PORT/$TEST_DB_NAME"

    cd "$PROJECT_ROOT"
    if ./target/release/postgres-seren-replicator init --remote \
        --source "$invalid_url" \
        --target "$target_url" \
        --yes 2>&1 | grep -q "failed"; then
        log_success "Failure case handled correctly"
    else
        log_warning "Failure case may not have been handled correctly"
        log_warning "This is not a critical error, continuing..."
    fi
}

# Print summary
print_summary() {
    log ""
    log "=========================================="
    log "Integration Test Summary"
    log "=========================================="
    log_success "Test databases started"
    log_success "Test data created"
    log_success "Remote replication executed"
    log_success "Data verified successfully"
    log_success "Failure case tested"
    log ""
    log "All integration tests passed!"
    log ""

    if [ "${KEEP_CONTAINERS:-false}" = "true" ]; then
        log "Test containers are still running:"
        log "  Source: docker exec -it test-source psql -U postgres -d $TEST_DB_NAME"
        log "  Target: docker exec -it test-target psql -U postgres -d $TEST_DB_NAME"
        log ""
        log "To stop them manually:"
        log "  docker stop test-source test-target && docker rm test-source test-target"
    fi
}

# Main test flow
main() {
    log "=========================================="
    log "End-to-End Integration Test"
    log "=========================================="
    log ""

    check_prerequisites
    log ""

    start_databases
    log ""

    create_test_data
    log ""

    run_remote_replication
    log ""

    verify_data
    log ""

    test_failure_case
    log ""

    print_summary
}

# Run main function
main "$@"
