# AWS Remote Execution Setup Guide

This guide covers the deployment, configuration, and maintenance of the SerenAI-managed remote execution infrastructure for `postgres-seren-replicator`.

## Overview

The remote execution service enables users to run replication jobs on AWS infrastructure without requiring their own AWS account or setup. SerenAI operates this as a managed service, handling all infrastructure costs and maintenance.

## Architecture

```
┌──────────────┐
│   User CLI   │  (postgres-seren-replicator binary)
└──────┬───────┘
       │ HTTPS (JSON + API Key Auth)
       ▼
┌─────────────────────────────────────┐
│       API Gateway (HTTP API)         │
│  - Custom domain: api.seren.cloud   │
│  - Throttling: 1000 req/sec         │
└──────┬──────────────────────────────┘
       │ Invoke
       ▼
┌─────────────────────────────────────┐       ┌──────────────────────┐
│     Lambda (Coordinator)             │◄─────►│    SSM Parameter     │
│  - Job submission handler            │       │    (API Key)         │
│  - Job status handler                │       └──────────────────────┘
│  - Enqueues provisioning requests    │
└──────┬──────────────────────────────┘
       │ Send Message
       ▼
┌─────────────────────────────────────┐
│         SQS Queue                    │
│  - Decouples submission/provisioning │
│  - Burst handling (up to 10 jobs)   │
│  - Dead letter queue for failures    │
└──────┬──────────────────────────────┘
       │ Poll
       ▼
┌─────────────────────────────────────┐
│   Lambda (Provisioner)               │
│  - Processes SQS queue               │
│  - Provisions EC2 workers            │
│  - Monitors concurrency limits       │
└──────┬──────────────────────────────┘
       │ RunInstances
       ▼
┌─────────────────────────────────────┐       ┌──────────────────────┐
│       EC2 Worker Instance            │       │     DynamoDB         │
│  - Custom AMI with replicator        │◄─────►│  - Job state/status  │
│  - Fetches job from DynamoDB         │       │  - Encrypted creds   │
│  - Decrypts credentials with KMS     │       │  - TTL: 30 days      │
│  - Runs replication                  │       └──────────────────────┘
│  - Updates progress in DynamoDB      │
│  - Self-terminates when done         │       ┌──────────────────────┐
└──────┬──────────────────────────────┘       │   KMS Key            │
       │                                       │  - Credential encrypt │
       │ Replication Traffic                   └──────────────────────┘
       │
       ▼
┌─────────────────────────────────────┐
│  Source & Target PostgreSQL DBs     │
│  - Customer-owned databases          │
│  - Must be internet-accessible       │
└─────────────────────────────────────┘
```

### Data Flow

1. **Job Submission**: User CLI → API Gateway → Lambda (coordinator) → DynamoDB (encrypted creds) → SQS queue
2. **Provisioning**: SQS → Lambda (provisioner) → EC2 worker launch → DynamoDB (instance_id)
3. **Execution**: EC2 worker → DynamoDB (fetch job) → KMS (decrypt creds) → PostgreSQL replication
4. **Status Updates**: EC2 worker → DynamoDB (progress) → User CLI polls via API Gateway
5. **Completion**: EC2 worker updates status → Self-terminates

### Security Components

- **API Key Authentication**: Stored in SSM Parameter Store (SecureString)
- **KMS Encryption**: Database credentials encrypted at rest in DynamoDB
- **IAM Roles**: Least-privilege permissions for Lambda and EC2
- **VPC**: Optional - can run in default VPC or custom VPC
- **Security Groups**: Minimal egress for HTTPS and PostgreSQL

## Prerequisites

Before deploying the infrastructure, ensure you have:

### Required Tools

```bash
# macOS
brew install terraform packer awscli

# Verify installations
terraform version  # Should be 1.0.0+
packer version     # Should be 1.7.0+
aws --version      # Should be 2.0.0+
```

### AWS Credentials

```bash
# Configure AWS CLI with your credentials
aws configure
# Provide: Access Key ID, Secret Access Key, Region (us-east-1 recommended), Output (json)

# Verify access
aws sts get-caller-identity
```

### Build Requirements

```bash
# Build the replicator binary
cd postgres-seren-replicator
cargo build --release

# Verify binary exists
ls -lh target/release/postgres-seren-replicator
```

## Deployment Steps

### Option 1: Automated Deployment (Recommended)

Use the `deploy.sh` script for end-to-end deployment:

```bash
cd aws
./deploy.sh
```

This script performs the following steps automatically:

1. Builds the release binary
2. Builds the AMI with Packer
3. Packages the Lambda function
4. Initializes Terraform
5. Applies Terraform configuration
6. Outputs API endpoint and key
7. Runs smoke tests

**Expected output:**

```
=== Building Release Binary ===
   Compiling postgres-seren-replicator...
    Finished release [optimized] target(s) in 45.2s

=== Building AMI with Packer ===
==> amazon-ebs: Creating temporary security group...
==> amazon-ebs: Launching a source AWS instance...
==> amazon-ebs: AMI: ami-0abc123def456...
Build 'amazon-ebs.worker' finished.

=== Packaging Lambda Function ===
Lambda function packaged: lambda/lambda.zip

=== Deploying with Terraform ===
Apply complete! Resources: 15 added, 0 changed, 0 destroyed.

Outputs:
api_endpoint = "https://abc123.execute-api.us-east-1.amazonaws.com"
api_key = "sk_live_abc123..."
```

Save the `api_endpoint` and `api_key` values - you'll need them for testing.

### Option 2: Manual Step-by-Step Deployment

If you prefer manual control or need to debug specific steps:

#### Step 1: Build the AMI

```bash
cd aws/ec2
./build-ami.sh
```

This creates an AMI with:

- PostgreSQL client tools (pg_dump, pg_restore, psql, pg_dumpall)
- The postgres-seren-replicator binary
- CloudWatch agent for log shipping
- All dependencies pre-installed

**Output**: AMI ID (e.g., `ami-0abc123def456...`)

#### Step 2: Package Lambda Function

```bash
cd aws/lambda
pip3 install -r requirements.txt -t .
zip -r lambda.zip . -x "*.pyc" -x "__pycache__/*"
```

#### Step 3: Configure Terraform Variables

Create `aws/terraform/terraform.tfvars`:

```hcl
project_name            = "seren-replication"
aws_region              = "us-east-1"
worker_ami_id           = "ami-0abc123def456..."  # From step 1
worker_instance_type    = "c5.2xlarge"
max_concurrent_jobs     = 10
api_key                 = "sk_live_generate_secure_random_key_here"  # Generate with: openssl rand -base64 32
```

#### Step 4: Deploy with Terraform

```bash
cd aws/terraform
terraform init
terraform plan
terraform apply
```

Review the plan carefully before approving.

#### Step 5: Save Outputs

```bash
# Save API endpoint
terraform output -raw api_endpoint > ../.api_endpoint

# Save API key (optional - already in tfvars)
terraform output -raw api_key > ../.api_key

# Export for immediate use
export SEREN_REMOTE_API=$(terraform output -raw api_endpoint)
```

### Post-Deployment Verification

Run the smoke tests to verify everything works:

```bash
cd aws
API_ENDPOINT=$(cat .api_endpoint)
API_KEY=$(terraform output -raw api_key)
./test-smoke.sh
```

Expected output:

```
========================================
Seren Replication API - Smoke Tests
========================================

Running 4 tests...

✓ Test 1/4: API health check
✓ Test 2/4: Invalid job submission (validation)
✓ Test 3/4: Valid job submission
✓ Test 4/4: Job status polling

========================================
All tests passed! (4/4)
========================================
```

## Cost Estimates

### Infrastructure Costs (Monthly)

| Component | Usage Pattern | Cost (us-east-1) |
|-----------|---------------|------------------|
| API Gateway | 1M requests/month | $1.00 |
| Lambda (Coordinator) | 1M invocations × 128MB × 100ms | $0.20 |
| Lambda (Provisioner) | 10K invocations × 256MB × 5s | $1.67 |
| DynamoDB | 5GB storage + 1M RCU + 1M WCU | $3.13 |
| SQS | 1M requests | $0.40 |
| KMS | 10K API calls | $0.30 |
| CloudWatch Logs | 10GB ingested + 5GB stored | $5.50 |
| **Base Monthly Cost** | Always-on components | **~$12.20** |

### Per-Job Costs (Variable)

| Database Size | Instance Type | Runtime | EC2 Cost | Data Transfer | Total |
|---------------|---------------|---------|----------|---------------|-------|
| < 10GB | t3.medium | ~30 min | $0.02 | $0.01 | $0.03 |
| 10-100GB | c5.large | ~3 hours | $0.26 | $0.10 | $0.36 |
| 100GB-1TB | c5.2xlarge | ~12 hours | $4.08 | $1.00 | $5.08 |
| > 1TB | c5.4xlarge | ~24 hours | $16.32 | $10.00 | $26.32 |

**Notes:**

- EC2 costs are for on-demand pricing
- Data transfer assumes replication to/from internet (not within AWS)
- Worker instances self-terminate after completion
- Spot instances can reduce EC2 costs by 70% (requires code changes)

### Cost Optimization Tips

1. **Use Spot Instances**: Modify provisioner to request spot instances for 70% savings
2. **Enable CloudWatch Log Retention**: Set 7-day retention (default) vs longer periods
3. **DynamoDB On-Demand**: For variable workloads, switch to on-demand pricing
4. **Regional Placement**: Deploy in same region as target databases to reduce data transfer costs

### Example Monthly Bill

For a service processing 50 jobs per month:

- Base infrastructure: $12.20
- 10 small jobs (< 10GB): 10 × $0.03 = $0.30
- 30 medium jobs (10-100GB): 30 × $0.36 = $10.80
- 10 large jobs (100GB-1TB): 10 × $5.08 = $50.80
- **Total: ~$74.10/month**

## Security

### Current Security Measures

#### API Authentication

- **API Key**: Stored in SSM Parameter Store as SecureString
- **Header-based**: `x-api-key` header required for all requests
- **Cached**: Lambda caches API key for container lifetime

#### Credential Encryption

- **At Rest**: Database credentials encrypted with KMS before storage in DynamoDB
- **In Transit**: HTTPS for all API communication
- **In Use**: Credentials decrypted by worker only when needed, never logged

#### IAM Roles (Least Privilege)

**Lambda Coordinator Role**:

```json
{
  "DynamoDB": ["PutItem", "GetItem"],
  "SQS": ["SendMessage"],
  "KMS": ["Encrypt"],
  "SSM": ["GetParameter"],
  "CloudWatch": ["PutMetricData", "CreateLogGroup", "CreateLogStream", "PutLogEvents"]
}
```

**Lambda Provisioner Role**:

```json
{
  "DynamoDB": ["GetItem", "UpdateItem"],
  "EC2": ["RunInstances", "DescribeInstances", "CreateTags"],
  "KMS": ["Encrypt"],
  "SQS": ["ReceiveMessage", "DeleteMessage"],
  "IAM": ["PassRole"],
  "CloudWatch": ["PutMetricData", "CreateLogGroup", "CreateLogStream", "PutLogEvents"]
}
```

**EC2 Worker Role**:

```json
{
  "DynamoDB": ["GetItem", "UpdateItem"],
  "KMS": ["Decrypt"],
  "CloudWatch": ["PutMetricData", "PutLogEvents"],
  "EC2": ["DescribeInstances"]
}
```

#### Network Security

- **Security Group**: Allows only HTTPS (443) and PostgreSQL (5432) egress
- **No Ingress**: Workers don't accept inbound connections
- **VPC**: Can run in default or custom VPC

### Security Best Practices

1. **Rotate API Keys Regularly**: Generate new keys every 90 days
2. **Enable CloudTrail**: Audit all API activity
3. **Use VPC Endpoints**: Reduce internet exposure for DynamoDB/SQS
4. **Enable KMS Key Rotation**: Automatic rotation every year
5. **Monitor Failed Auth Attempts**: CloudWatch alarms for 401 responses
6. **Restrict IAM Policies**: Review and tighten permissions quarterly

## Monitoring

### CloudWatch Metrics

The service emits custom metrics under the `SerenReplication` namespace:

| Metric | Description | Dimensions |
|--------|-------------|------------|
| `JobSubmitted` | Job submission count | Command |
| `JobProvisioned` | Successful EC2 launches | InstanceType, InstanceSize |
| `ProvisioningDuration` | Time to provision worker (seconds) | InstanceType |
| `ProvisioningFailed` | Failed provisioning attempts | InstanceType, Reason |

**Recommended CloudWatch Alarms**:

```bash
# High failure rate
aws cloudwatch put-metric-alarm \
  --alarm-name seren-replication-high-failures \
  --metric-name ProvisioningFailed \
  --namespace SerenReplication \
  --statistic Sum \
  --period 300 \
  --threshold 5 \
  --comparison-operator GreaterThanThreshold

# Long provisioning times
aws cloudwatch put-metric-alarm \
  --alarm-name seren-replication-slow-provisioning \
  --metric-name ProvisioningDuration \
  --namespace SerenReplication \
  --statistic Average \
  --period 300 \
  --threshold 300 \
  --comparison-operator GreaterThanThreshold
```

### CloudWatch Logs

**Log Groups**:

- `/aws/lambda/seren-replication-coordinator`: API requests, job submissions
- `/aws/lambda/seren-replication-provisioner`: EC2 provisioning, SQS processing
- `/aws/ec2/seren-replication-worker`: Replication job execution

**Useful Log Insights Queries**:

```sql
-- Find failed jobs in last 24 hours
fields @timestamp, job_id, error
| filter @message like /FAILED/
| sort @timestamp desc
| limit 50

-- Average job duration by database size
fields @timestamp, job_id, duration_seconds, database_size_gb
| stats avg(duration_seconds) as avg_duration by bin(database_size_gb, 100)

-- Trace specific job end-to-end
fields @timestamp, @message
| filter trace_id = "your-trace-id-here"
| sort @timestamp asc
```

### Distributed Tracing

Every job gets a unique `trace_id` (UUID) that flows through:

1. CLI submission → API Gateway → Lambda coordinator
2. Lambda coordinator → SQS → Lambda provisioner
3. Lambda provisioner → EC2 worker
4. All worker logs include `[TRACE:uuid]`

**To trace a job**:

```bash
# Get job details from CLI
postgres-seren-replicator init ...
# Output shows: Job ID: xxx, Trace ID: yyy

# Search CloudWatch Logs for trace ID
aws logs filter-log-events \
  --log-group-name /aws/ec2/seren-replication-worker \
  --filter-pattern "[TRACE:your-trace-id]"
```

### Status API Integration

Users can query job status via the API:

```bash
curl -H "x-api-key: $API_KEY" \
  "$API_ENDPOINT/jobs/$JOB_ID"
```

Response includes:

```json
{
  "job_id": "550e8400-e29b-41d4-a716-446655440000",
  "trace_id": "660e8400-e29b-41d4-a716-446655440000",
  "status": "running",
  "created_at": "2025-11-21T08:00:00Z",
  "started_at": "2025-11-21T08:02:15Z",
  "progress": {
    "current_database": "analytics",
    "databases_completed": 1,
    "databases_total": 2
  },
  "log_url": "https://console.aws.amazon.com/cloudwatch/home?region=us-east-1#logsV2:log-groups/log-group/..."
}
```

The `log_url` field provides a direct link to CloudWatch Logs for the worker.

## Troubleshooting

### Common Issues

#### Provisioning Failures

**Symptom**: Job stuck in "provisioning" state for > 10 minutes

**Possible Causes**:

- Insufficient EC2 capacity in the region
- IAM role misconfiguration
- AMI not found or not accessible
- Security group blocks required egress

**Debug Steps**:

```bash
# Check Lambda provisioner logs
aws logs tail /aws/lambda/seren-replication-provisioner --follow

# Check for EC2 API errors
aws logs filter-log-events \
  --log-group-name /aws/lambda/seren-replication-provisioner \
  --filter-pattern "ClientError"

# Verify AMI exists
aws ec2 describe-images --image-ids ami-0abc123...

# Check IAM role
aws iam get-role --role-name seren-replication-worker
```

#### Worker Execution Failures

**Symptom**: Job transitions to "failed" status quickly after "running"

**Possible Causes**:

- Database credentials invalid
- Database not accessible from internet
- Binary crash or panic
- Permission issues (REPLICATION role, etc.)

**Debug Steps**:

```bash
# Get instance ID from DynamoDB
aws dynamodb get-item \
  --table-name replication-jobs \
  --key '{"job_id": {"S": "your-job-id"}}'

# Check worker logs
aws logs tail /aws/ec2/seren-replication-worker --follow | grep "your-instance-id"

# Look for error messages
aws logs filter-log-events \
  --log-group-name /aws/ec2/seren-replication-worker \
  --log-stream-name "your-instance-id" \
  --filter-pattern "ERROR"
```

#### Authentication Failures

**Symptom**: 401 Unauthorized responses from API

**Possible Causes**:

- API key not set in environment
- API key mismatch between Terraform and SSM
- API key parameter not found

**Debug Steps**:

```bash
# Verify API key in SSM
aws ssm get-parameter \
  --name /seren-replication/api-key \
  --with-decryption

# Check Lambda environment variables
aws lambda get-function-configuration \
  --function-name seren-replication-coordinator

# Test with curl
curl -v -H "x-api-key: your-key" "$API_ENDPOINT/jobs/test-job-id"
```

#### DynamoDB Issues

**Symptom**: 500 Internal Server Error from API

**Possible Causes**:

- DynamoDB table not found
- Throttling (exceeded provisioned capacity)
- IAM permissions missing

**Debug Steps**:

```bash
# Verify table exists
aws dynamodb describe-table --table-name replication-jobs

# Check for throttling
aws cloudwatch get-metric-statistics \
  --namespace AWS/DynamoDB \
  --metric-name UserErrors \
  --dimensions Name=TableName,Value=replication-jobs \
  --statistics Sum \
  --start-time 2025-11-21T00:00:00Z \
  --end-time 2025-11-21T23:59:59Z \
  --period 3600

# Check Lambda logs for DynamoDB errors
aws logs filter-log-events \
  --log-group-name /aws/lambda/seren-replication-coordinator \
  --filter-pattern "DynamoDB"
```

### Performance Issues

#### Slow Job Execution

If replication jobs are slower than expected:

1. **Check instance type**: Ensure worker is sized appropriately for database
2. **Network bandwidth**: Verify source/target databases have good connectivity
3. **Database performance**: Source database may be slow to export data
4. **Parallel workers**: Default is 8 workers - may need tuning for specific workloads

#### High Costs

If AWS bills are higher than expected:

1. **Orphaned workers**: Check for EC2 instances that didn't self-terminate
2. **CloudWatch Logs**: Review retention periods and log volume
3. **Data transfer**: Ensure databases are in same region if possible
4. **Failed jobs**: Failed jobs still incur costs - investigate root causes

## Cleanup

### Removing All Infrastructure

To completely tear down the remote execution service:

```bash
cd aws/terraform
terraform destroy
```

This will remove:

- API Gateway
- Lambda functions
- DynamoDB table
- SQS queues
- IAM roles and policies
- CloudWatch log groups
- SSM parameters

**Note**: This does NOT remove:

- The custom AMI (manually delete via AWS Console or CLI)
- Any running EC2 worker instances (terminate manually)
- CloudWatch Logs data (set retention or delete manually)

### Partial Cleanup

To remove only specific components:

```bash
# Remove only Lambda functions
terraform destroy -target=aws_lambda_function.coordinator
terraform destroy -target=aws_lambda_function.provisioner

# Remove only API Gateway
terraform destroy -target=aws_apigatewayv2_api.this

# Remove only DynamoDB table
terraform destroy -target=aws_dynamodb_table.jobs
```

### Cleaning Up Old AMIs

AMIs are versioned and accumulate over time. Clean up old versions:

```bash
# List all AMIs
aws ec2 describe-images \
  --owners self \
  --filters "Name=name,Values=seren-replication-worker-*" \
  --query 'Images[*].[ImageId,CreationDate,Name]' \
  --output table

# Delete old AMI (keep latest 3)
aws ec2 deregister-image --image-id ami-old-version-123
```

### Cleaning Up Failed Workers

If worker instances didn't self-terminate:

```bash
# Find workers
aws ec2 describe-instances \
  --filters "Name=tag:ManagedBy,Values=seren-replication-system" \
  --query 'Reservations[*].Instances[*].[InstanceId,State.Name,LaunchTime]' \
  --output table

# Terminate stuck workers
aws ec2 terminate-instances --instance-ids i-abc123... i-def456...
```

## Updating the Service

### Updating the Binary

When the replicator binary is updated:

1. **Build new release**: `cargo build --release`
2. **Rebuild AMI**: `cd aws/ec2 && ./build-ami.sh`
3. **Update Terraform**: Edit `worker_ami_id` in `terraform.tfvars`
4. **Apply changes**: `cd aws/terraform && terraform apply`

### Updating Lambda Functions

When Lambda code changes:

1. **Update code**: Edit `aws/lambda/handler.py` or `provisioner.py`
2. **Repackage**: `cd aws/lambda && ./package.sh` (or manually zip)
3. **Apply Terraform**: `cd aws/terraform && terraform apply`

Terraform will automatically detect the Lambda code change and redeploy.

### Updating Infrastructure

For Terraform configuration changes:

```bash
cd aws/terraform
terraform plan  # Review changes
terraform apply  # Apply changes
```

**Zero-downtime updates**:

- API Gateway: Updates are immediate, no downtime
- Lambda: New code deployed to new versions, old requests complete
- DynamoDB: Schema changes may require blue/green migration
- EC2 AMI: Only affects new workers, existing jobs continue

## Additional Resources

- [Integration Testing Guide](./integration-testing.md) - Running integration tests
- [CI/CD Guide](./cicd.md) - Automated deployment pipelines
- [API Schema Documentation](./api-schema.md) - Job spec validation and versioning
- [AWS README](../aws/README.md) - Component-specific documentation
- [Terraform README](../aws/terraform/README.md) - Infrastructure details
- [Lambda README](../aws/lambda/README.md) - Function architecture
- [EC2 README](../aws/ec2/README.md) - Worker AMI build process

## Support

For issues or questions about the AWS infrastructure:

- **GitHub Issues**: <https://github.com/serenorg/postgres-seren-replicator/issues>
- **Internal Slack**: #seren-infrastructure
- **On-call**: PagerDuty rotation for production incidents
