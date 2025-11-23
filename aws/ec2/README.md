# EC2 Worker for Remote Replication

This directory contains scripts for EC2 worker instances that execute remote replication jobs.

## Architecture

1. **Lambda Function** provisions an EC2 instance with a job specification
2. **EC2 Worker** (`worker.sh`) executes the replication job
3. **Worker** updates DynamoDB with progress and status
4. **Worker** self-terminates when complete

## Files

- `worker.sh` - Bootstrap script that runs on EC2 instances
- `setup-worker.sh` - AMI setup script (installs dependencies)
- `build-ami.sh` - Automated AMI build with Packer

## AMI Requirements

The worker AMI must have the following installed:

### Required Software

- **PostgreSQL 17 Client Tools**:
  - `psql` - Interactive terminal
  - `pg_dump` - Database dump utility
  - `pg_dumpall` - Cluster dump utility
  - `pg_restore` - Database restore utility

- **AWS CLI v2**: For DynamoDB updates and EC2 metadata
- **jq**: JSON parsing for job specifications
- **ec2-metadata**: EC2 instance metadata helper

### Required Files

- `/opt/seren-replicator/seren-replicator` - Replicator binary (executable)
- `/opt/seren-replicator/worker.sh` - Worker bootstrap script (executable)

### IAM Role

The EC2 instance profile must have permissions for:
- `dynamodb:UpdateItem` - Update job status
- `dynamodb:GetItem` - Read job status
- `ec2:TerminateInstances` - Self-terminate
- `logs:CreateLogStream` - CloudWatch logging
- `logs:PutLogEvents` - CloudWatch logging

## Building an AMI

### Prerequisites

```bash
# Install Packer
brew install packer

# Build release binary
cargo build --release

# Verify binary
./target/release/seren-replicator --version
```

### Build AMI

```bash
# Run automated build script
./build-ami.sh

# Or manually with Packer
packer init .
packer build -var "binary_path=../../target/release/seren-replicator" worker-ami.pkr.hcl
```

The build process:
1. Launches base Ubuntu 24.04 instance
2. Installs PostgreSQL 17 client tools
3. Installs AWS CLI v2, jq, ec2-metadata
4. Copies replicator binary to `/opt/seren-replicator/`
5. Copies worker script to `/opt/seren-replicator/`
6. Sets executable permissions
7. Creates AMI snapshot

Build time: ~10 minutes

### Get AMI ID

```bash
# After build completes
aws ec2 describe-images \
  --owners self \
  --filters "Name=name,Values=seren-replicator-worker-*" \
  --query 'Images | sort_by(@, &CreationDate) | [-1].ImageId' \
  --output text
```

Use this AMI ID in Terraform's `worker_ami_id` variable.

## Worker Script Usage

The worker script is invoked automatically by the EC2 user data script:

```bash
/opt/seren-replicator/worker.sh "<job_id>" "/tmp/job_spec.json"
```

### Job Specification Format

```json
{
  "version": "1.0.0",
  "command": "init",
  "source_url": "postgresql://user:pass@source:5432/db",
  "target_url": "postgresql://user:pass@target:5432/db",
  "filter": {
    "include_databases": ["db1", "db2"],
    "exclude_tables": ["db1.logs", "db2.cache"]
  },
  "options": {
    "drop_existing": true,
    "no_sync": false
  }
}
```

### Worker Workflow

1. **Parse Job Spec**: Read JSON file and extract parameters
2. **Update Status**: Set DynamoDB status to "running"
3. **Build Command**: Construct `seren-replicator` command with all flags
4. **Execute**: Run replication with proper error handling
5. **Update Status**: Set "completed" or "failed" based on result
6. **Self-Terminate**: Shut down EC2 instance to stop charges

### Environment Variables

- `DYNAMODB_TABLE`: DynamoDB table name (default: replication-jobs)
- `AWS_REGION`: AWS region (default: us-east-1)

These are automatically set by the Lambda function via EC2 user data.

## Testing Locally

You can test the worker script locally (without EC2 metadata):

```bash
# Create test job spec
cat > /tmp/test_job.json <<EOF
{
  "version": "1.0.0",
  "command": "validate",
  "source_url": "postgresql://postgres:postgres@localhost:5432/postgres",
  "target_url": "postgresql://postgres:postgres@localhost:5433/postgres",
  "filter": {},
  "options": {}
}
EOF

# Run worker script (will fail on DynamoDB/EC2 operations)
export DYNAMODB_TABLE=test-jobs
export AWS_REGION=us-east-1
./worker.sh test-job-123 /tmp/test_job.json
```

**Note**: Local testing will fail on:
- DynamoDB updates (requires valid AWS credentials and table)
- EC2 metadata retrieval (requires running on EC2)
- Self-termination (requires EC2 instance)

For full testing, deploy the AMI and trigger a real job via the API.

## Troubleshooting

### Worker script fails immediately

Check CloudWatch Logs for the instance:
```bash
aws logs tail /aws/ec2/seren-replication --follow
```

### DynamoDB permission errors

Ensure the IAM instance profile has `dynamodb:UpdateItem` and `dynamodb:GetItem` permissions.

### Replicator binary not found

Verify the binary was copied to `/opt/seren-replicator/seren-replicator` and is executable:
```bash
ssh ec2-user@instance
ls -la /opt/seren-replicator/
```

### Instance doesn't self-terminate

Check that:
1. EC2 instance profile has `ec2:TerminateInstances` permission
2. `ec2-metadata` is installed and working
3. Instance wasn't launched with termination protection

## Cost Optimization

- Instances are charged per second while running
- Use spot instances for non-critical jobs (up to 90% savings)
- Workers self-terminate immediately after completion
- Failed jobs also terminate (no orphaned instances)
- DynamoDB items expire after 30 days (TTL)

Typical costs per job:
- c5.2xlarge: $0.34/hour = $0.0057/minute
- 30-minute replication: ~$0.17
- 2-hour replication: ~$0.68
