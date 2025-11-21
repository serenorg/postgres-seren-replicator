# AWS Remote Replication Infrastructure

This directory contains all infrastructure code and scripts for remote replication on AWS.

## Architecture

```
┌─────────────┐
│   Client    │
│   (CLI)     │
└──────┬──────┘
       │ HTTP/JSON
       ▼
┌──────────────────┐
│  API Gateway     │
│  (HTTP API)      │
└──────┬───────────┘
       │
       ▼
┌──────────────────┐       ┌──────────────────┐
│  Lambda          │◄─────►│   DynamoDB       │
│  (Coordinator)   │       │   (Job State)    │
└──────┬───────────┘       └──────────────────┘
       │
       │ Provisions
       ▼
┌──────────────────┐       ┌──────────────────┐
│  EC2 Worker      │◄─────►│  PostgreSQL      │
│  (Replicator)    │       │  (Source/Target) │
└──────────────────┘       └──────────────────┘
```

### Components

1. **API Gateway**: HTTP API endpoint for job submission and status queries
2. **Lambda Function**: Orchestrates job lifecycle, provisions EC2 workers
3. **DynamoDB**: Stores job state with TTL for automatic cleanup
4. **EC2 Workers**: Run replication jobs, self-terminate when complete
5. **IAM Roles**: Separate roles for Lambda and EC2 with minimal permissions

## Directory Structure

```
aws/
├── lambda/              # Lambda function code
│   ├── handler.py       # Request routing and job orchestration
│   ├── requirements.txt # Python dependencies
│   ├── lambda.zip       # Packaged function (generated)
│   └── README.md        # Lambda documentation
├── terraform/           # Infrastructure as Code
│   ├── main.tf          # Resource definitions
│   ├── variables.tf     # Configuration variables
│   ├── outputs.tf       # Output values
│   ├── terraform.tfvars # User configuration (gitignored)
│   └── README.md        # Terraform documentation
├── ec2/                 # EC2 worker scripts
│   ├── worker.sh        # Bootstrap script for replication
│   ├── setup-worker.sh  # AMI dependency installation
│   ├── build-ami.sh     # Automated AMI build with Packer
│   └── README.md        # EC2 documentation
├── deploy.sh            # Automated deployment script
├── test-integration.sh  # End-to-end integration tests
└── README.md            # This file
```

## Quick Start

### Prerequisites

```bash
# Install dependencies (macOS)
brew install terraform packer awscli

# Configure AWS credentials
aws configure

# Build release binary
cargo build --release
```

### Deploy Infrastructure

**Option 1: Automated (Recommended)**

```bash
# Single command deployment
./aws/deploy.sh
```

This script will:
1. Build the release binary
2. Build the worker AMI (~10 minutes)
3. Package the Lambda function
4. Deploy with Terraform
5. Test the API endpoint

**Option 2: Manual**

```bash
# 1. Build worker AMI
cd aws/ec2
./build-ami.sh
export WORKER_AMI_ID=$(aws ec2 describe-images \
  --owners self \
  --filters "Name=name,Values=postgres-seren-replicator-worker-*" \
  --query 'Images | sort_by(@, &CreationDate) | [-1].ImageId' \
  --output text)

# 2. Package Lambda
cd ../lambda
zip lambda.zip handler.py requirements.txt

# 3. Deploy with Terraform
cd ../terraform
terraform init
terraform apply -var="worker_ami_id=$WORKER_AMI_ID"

# 4. Get API endpoint
export SEREN_REMOTE_API=$(terraform output -raw api_endpoint)
```

### Run Integration Tests

```bash
# Automated end-to-end test
./aws/test-integration.sh
```

This script will:
1. Start test PostgreSQL databases with Docker
2. Create test data
3. Run remote replication
4. Verify data was replicated correctly
5. Test failure handling
6. Clean up containers

## Usage

### Submit Remote Replication Job

```bash
# Set API endpoint (from deployment)
export SEREN_REMOTE_API="https://xxx.execute-api.us-east-1.amazonaws.com"

# Run remote replication
postgres-seren-replicator init --remote \
  --source "postgresql://user:pass@source:5432/db" \
  --target "postgresql://user:pass@target:5432/db" \
  --yes
```

The CLI will:
1. Submit job to API
2. Wait for EC2 worker to provision
3. Stream status updates
4. Report final result

### Monitor Jobs

```bash
# Watch EC2 instances
aws ec2 describe-instances \
  --filters "Name=tag:ManagedBy,Values=seren-replication-system" \
  --query 'Reservations[].Instances[].[InstanceId,State.Name,Tags[?Key==`JobId`].Value|[0]]' \
  --output table

# Query DynamoDB for jobs
aws dynamodb scan \
  --table-name replication-jobs \
  --query 'Items[].{JobId:job_id.S,Status:status.S,Created:created_at.S}'

# View Lambda logs
aws logs tail /aws/lambda/seren-replication-coordinator --follow

# View worker logs (get instance ID first)
aws ec2 get-console-output --instance-id i-xxx
```

## Cost Estimation

### Fixed Costs (Monthly)

- **DynamoDB**: ~$1-2 (on-demand, minimal usage)
- **API Gateway**: ~$1 (first million requests free, then $1/million)
- **Lambda**: ~$0.20-1 (256MB, 30s per invocation)
- **CloudWatch Logs**: ~$0.50 (7-day retention)

**Total fixed**: ~$3-5/month

### Variable Costs (Per Job)

- **EC2 Worker**: Charged per second while running
  - c5.2xlarge: $0.34/hour = $0.0057/minute
  - 30-minute job: ~$0.17
  - 2-hour job: ~$0.68
  - 8-hour job: ~$2.72

- **Data Transfer**: $0.09/GB out to internet (if target is outside AWS)
  - Replicating 100GB: ~$9
  - Staying within AWS: Free (same region)

### Example Monthly Costs

- **Light usage** (10 jobs/month, 30 min each): ~$5 fixed + $2 workers = **$7/month**
- **Medium usage** (50 jobs/month, 1 hour each): ~$5 fixed + $17 workers = **$22/month**
- **Heavy usage** (200 jobs/month, 2 hours each): ~$5 fixed + $136 workers = **$141/month**

### Cost Optimization Tips

1. **Use Spot Instances**: Up to 90% savings (modify Terraform)
2. **Right-size Instance Type**: Use smaller instances for small databases
3. **Enable TTL**: DynamoDB items auto-expire after 30 days
4. **Regional Strategy**: Keep source/target/workers in same region
5. **Monitor Failures**: Failed jobs still incur costs

## Infrastructure Updates

### Update Lambda Code

```bash
cd aws/lambda
zip lambda.zip handler.py
cd ../terraform
terraform apply -target=aws_lambda_function.coordinator
```

### Update Worker AMI

```bash
# Rebuild AMI with new binary
cargo build --release
./aws/ec2/build-ami.sh

# Update Terraform with new AMI ID
export NEW_AMI_ID=$(aws ec2 describe-images \
  --owners self \
  --filters "Name=name,Values=postgres-seren-replicator-worker-*" \
  --query 'Images | sort_by(@, &CreationDate) | [-1].ImageId' \
  --output text)

cd aws/terraform
terraform apply -var="worker_ami_id=$NEW_AMI_ID"
```

### Destroy Infrastructure

```bash
cd aws/terraform
terraform destroy
```

**WARNING**: This will delete all infrastructure including job history in DynamoDB.

## Troubleshooting

### Job stuck in "provisioning"

Check if EC2 instance launched:
```bash
aws ec2 describe-instances \
  --filters "Name=tag:ManagedBy,Values=seren-replication-system" \
  --query 'Reservations[].Instances[].[InstanceId,State.Name,LaunchTime]'
```

If no instances, check Lambda logs for provisioning errors.

### Job fails immediately

Check CloudWatch logs for the Lambda function:
```bash
aws logs tail /aws/lambda/seren-replication-coordinator --follow
```

Common issues:
- Invalid AMI ID
- IAM permissions missing
- DynamoDB table doesn't exist

### Worker instance doesn't self-terminate

Check that:
1. Worker IAM role has `ec2:TerminateInstances` permission
2. `ec2-metadata` tool is installed in AMI
3. Worker script completed successfully

Manually terminate:
```bash
aws ec2 terminate-instances --instance-ids i-xxx
```

### High AWS costs

Check for orphaned resources:
```bash
# Find running workers
aws ec2 describe-instances \
  --filters "Name=tag:ManagedBy,Values=seren-replication-system" \
            "Name=instance-state-name,Values=running"

# Check DynamoDB table size
aws dynamodb describe-table --table-name replication-jobs \
  --query 'Table.TableSizeBytes'
```

## Security

### Secrets Management

- **Never commit credentials**: Use AWS Secrets Manager or Parameter Store
- **Connection strings**: Pass through API securely, not stored in DynamoDB plaintext
- **IAM roles**: Use instance profiles, never hardcoded keys
- **API authentication**: Add API keys or Cognito (not included in basic setup)

### Network Security

- **VPC**: Deploy Lambda and EC2 in private subnets (advanced)
- **Security groups**: Restrict outbound to PostgreSQL ports only
- **Encryption**: Enable encryption at rest for DynamoDB (default enabled)

### Compliance

- **Logging**: All API calls logged to CloudWatch
- **Audit trail**: DynamoDB provides complete job history
- **Data retention**: 30-day TTL on DynamoDB records
- **GDPR**: Customer responsible for data handling in source/target

## Advanced Configuration

### Custom Worker Instance Types

Edit `aws/terraform/terraform.tfvars`:
```hcl
worker_instance_type = "c5.4xlarge"  # More powerful for large databases
```

### Multi-Region Deployment

Deploy infrastructure in multiple regions:
```bash
export AWS_REGION=eu-west-1
./aws/deploy.sh
```

### Spot Instances for Workers

Modify `aws/terraform/main.tf` to use spot requests:
```hcl
resource "aws_spot_instance_request" "worker" {
  # ... configuration
}
```

### Private VPC Deployment

Requires:
- VPC endpoints for DynamoDB and EC2
- NAT gateway for outbound internet access
- Lambda in VPC with subnets
- Security groups for PostgreSQL access

## Development

### Local Testing

```bash
# Test worker script locally
export DYNAMODB_TABLE=test-jobs
export AWS_REGION=us-east-1
./aws/ec2/worker.sh test-job-id /tmp/job_spec.json
```

### Lambda Local Invocation

```bash
# Install AWS SAM CLI
brew install aws-sam-cli

# Create test event
cat > event.json <<EOF
{
  "httpMethod": "POST",
  "path": "/jobs",
  "body": "{\"command\":\"init\",\"source_url\":\"test\",\"target_url\":\"test\"}"
}
EOF

# Invoke locally
sam local invoke -e event.json
```

## Support

- **Issues**: https://github.com/serenorg/postgres-seren-replicator/issues
- **Documentation**: See README.md files in subdirectories
- **AWS Support**: https://console.aws.amazon.com/support/

## License

Apache-2.0 - See LICENSE file in repository root
