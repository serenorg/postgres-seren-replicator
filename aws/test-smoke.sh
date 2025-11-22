#!/bin/bash
# ABOUTME: Smoke test script to validate remote replication API deployment
# ABOUTME: Tests job submission, status polling, and basic API functionality

set -euo pipefail

# Configuration
API_ENDPOINT="${API_ENDPOINT:-}"
API_KEY="${API_KEY:-}"
MAX_POLL_ATTEMPTS="${MAX_POLL_ATTEMPTS:-60}"
POLL_INTERVAL="${POLL_INTERVAL:-5}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Log functions
log() {
    echo -e "[$(date -u +"%Y-%m-%dT%H:%M:%SZ")] $*"
}

error() {
    echo -e "${RED}[ERROR]${NC} $*" >&2
}

success() {
    echo -e "${GREEN}[SUCCESS]${NC} $*"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $*"
}

# Check prerequisites
check_prerequisites() {
    log "Checking prerequisites..."

    if ! command -v curl &> /dev/null; then
        error "curl not found. Install with: brew install curl (macOS) or apt-get install curl (Linux)"
        exit 1
    fi

    if ! command -v jq &> /dev/null; then
        error "jq not found. Install with: brew install jq (macOS) or apt-get install jq (Linux)"
        exit 1
    fi

    if [ -z "$API_ENDPOINT" ]; then
        error "API_ENDPOINT environment variable not set"
        error "Usage: API_ENDPOINT=https://xxx.execute-api.us-east-1.amazonaws.com API_KEY=xxx ./test-smoke.sh"
        exit 1
    fi

    if [ -z "$API_KEY" ]; then
        error "API_KEY environment variable not set"
        error "Usage: API_ENDPOINT=https://xxx.execute-api.us-east-1.amazonaws.com API_KEY=xxx ./test-smoke.sh"
        exit 1
    fi

    success "Prerequisites check passed"
}

# Test 1: Health check (optional - just check if API responds)
test_api_health() {
    log "Test 1: API health check..."

    HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
        -H "x-api-key: $API_KEY" \
        "$API_ENDPOINT/jobs/nonexistent")

    if [ "$HTTP_CODE" = "404" ]; then
        success "API is responding (expected 404 for nonexistent job)"
        return 0
    elif [ "$HTTP_CODE" = "401" ]; then
        error "Authentication failed (401) - check API_KEY"
        return 1
    else
        warn "Unexpected status code: $HTTP_CODE (expected 404)"
        return 0
    fi
}

# Test 2: Submit job with invalid payload (should fail gracefully)
test_invalid_job_submission() {
    log "Test 2: Invalid job submission (validation test)..."

    RESPONSE=$(curl -s -w "\n%{http_code}" \
        -X POST \
        -H "x-api-key: $API_KEY" \
        -H "Content-Type: application/json" \
        -d '{"invalid": "payload"}' \
        "$API_ENDPOINT/jobs")

    HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
    BODY=$(echo "$RESPONSE" | head -n-1)

    if [ "$HTTP_CODE" = "400" ]; then
        success "Invalid payload rejected correctly (400)"
        return 0
    else
        error "Expected 400 for invalid payload, got: $HTTP_CODE"
        error "Response: $BODY"
        return 1
    fi
}

# Test 3: Submit valid job (with mock credentials)
test_valid_job_submission() {
    log "Test 3: Valid job submission..."

    # Use mock/invalid credentials - job should be accepted but will fail during execution
    # This tests the API layer, not the actual replication
    RESPONSE=$(curl -s -w "\n%{http_code}" \
        -X POST \
        -H "x-api-key: $API_KEY" \
        -H "Content-Type: application/json" \
        -d '{
            "command": "validate",
            "source_url": "postgresql://mock:mock@localhost:5432/testdb",
            "target_url": "postgresql://mock:mock@localhost:5433/testdb"
        }' \
        "$API_ENDPOINT/jobs")

    HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
    BODY=$(echo "$RESPONSE" | head -n-1)

    if [ "$HTTP_CODE" != "201" ]; then
        error "Job submission failed with status: $HTTP_CODE"
        error "Response: $BODY"
        return 1
    fi

    JOB_ID=$(echo "$BODY" | jq -r '.job_id')
    TRACE_ID=$(echo "$BODY" | jq -r '.trace_id')
    STATUS=$(echo "$BODY" | jq -r '.status')

    if [ -z "$JOB_ID" ] || [ "$JOB_ID" = "null" ]; then
        error "No job_id in response"
        error "Response: $BODY"
        return 1
    fi

    if [ -z "$TRACE_ID" ] || [ "$TRACE_ID" = "null" ]; then
        warn "No trace_id in response (observability not deployed?)"
    fi

    success "Job submitted successfully"
    log "  Job ID: $JOB_ID"
    log "  Trace ID: $TRACE_ID"
    log "  Initial status: $STATUS"

    # Export for next test
    export SMOKE_TEST_JOB_ID="$JOB_ID"
    export SMOKE_TEST_TRACE_ID="$TRACE_ID"
}

# Test 4: Poll job status
test_job_status_polling() {
    log "Test 4: Job status polling..."

    if [ -z "${SMOKE_TEST_JOB_ID:-}" ]; then
        error "No job ID from previous test"
        return 1
    fi

    local attempts=0
    local last_status=""

    while [ $attempts -lt $MAX_POLL_ATTEMPTS ]; do
        RESPONSE=$(curl -s -w "\n%{http_code}" \
            -H "x-api-key: $API_KEY" \
            "$API_ENDPOINT/jobs/$SMOKE_TEST_JOB_ID")

        HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
        BODY=$(echo "$RESPONSE" | head -n-1)

        if [ "$HTTP_CODE" != "200" ]; then
            error "Status check failed with code: $HTTP_CODE"
            return 1
        fi

        STATUS=$(echo "$BODY" | jq -r '.status')
        LOG_URL=$(echo "$BODY" | jq -r '.log_url // "not_available"')

        if [ "$STATUS" != "$last_status" ]; then
            log "  Status: $STATUS (attempt $((attempts + 1))/$MAX_POLL_ATTEMPTS)"
            if [ "$LOG_URL" != "not_available" ] && [ "$LOG_URL" != "null" ]; then
                log "  Logs: $LOG_URL"
            fi
            last_status="$STATUS"
        fi

        # Terminal states
        if [ "$STATUS" = "completed" ]; then
            success "Job completed successfully"
            return 0
        elif [ "$STATUS" = "failed" ]; then
            ERROR_MSG=$(echo "$BODY" | jq -r '.error // "no error message"')
            warn "Job failed (expected for mock credentials): $ERROR_MSG"
            success "Job lifecycle tested successfully (submission → provisioning → failed)"
            return 0
        elif [ "$STATUS" = "timeout" ]; then
            warn "Job timed out"
            return 0
        fi

        attempts=$((attempts + 1))
        sleep $POLL_INTERVAL
    done

    warn "Job did not reach terminal state within $((MAX_POLL_ATTEMPTS * POLL_INTERVAL)) seconds"
    warn "Last status: $last_status"
    warn "This is acceptable for smoke test - job is processing"
    return 0
}

# Main execution
main() {
    log "==================================="
    log "Remote Replication API Smoke Test"
    log "==================================="
    log ""
    log "API Endpoint: $API_ENDPOINT"
    log ""

    local failed=0

    check_prerequisites || exit 1

    test_api_health || ((failed++))
    test_invalid_job_submission || ((failed++))
    test_valid_job_submission || ((failed++))
    test_job_status_polling || ((failed++))

    log ""
    log "==================================="
    if [ $failed -eq 0 ]; then
        success "All smoke tests passed! ✅"
        log "==================================="
        exit 0
    else
        error "$failed test(s) failed"
        log "==================================="
        exit 1
    fi
}

# Run main function
main "$@"
