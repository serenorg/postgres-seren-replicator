# Terraform Configuration for Remote Replication

This directory contains Terraform configuration to deploy the AWS infrastructure for remote replication.

## Architecture

- **DynamoDB Table**: Stores job state with TTL for automatic cleanup
- **Lambda Function**: Orchestrates job submission and status queries
- **API Gateway**: HTTP API for client communication
- **IAM Roles**: Separate roles for Lambda execution and EC2 workers
- **CloudWatch Logs**: Lambda execution logs with 7-day retention

## Prerequisites

1. **Install Terraform**:
   ```bash
   # macOS
   brew install terraform

   # Linux
   wget https://releases.hashicorp.com/terraform/1.6.0/terraform_1.6.0_linux_amd64.zip
   unzip terraform_1.6.0_linux_amd64.zip
   sudo mv terraform /usr/local/bin/
   ```

2. **AWS Credentials**:
   ```bash
   # Configure AWS CLI
   aws configure

   # Or set environment variables
   export AWS_ACCESS_KEY_ID="..."
   export AWS_SECRET_ACCESS_KEY="..."
   export AWS_REGION="us-east-1"
   ```

3. **Lambda Package**:
   ```bash
   # Package Lambda function
   cd ../lambda
   zip -r lambda.zip handler.py
   cd ../terraform
   ```

4. **Worker AMI**:
   Build an AMI with `seren-replicator` installed (see AMI creation guide).

## Deployment

### Initialize Terraform

```bash
terraform init
```

### Create terraform.tfvars

Create a `terraform.tfvars` file with your configuration:

```hcl
aws_region            = "us-east-1"
project_name          = "seren-replication"
dynamodb_table_name   = "replication-jobs"
worker_ami_id         = "ami-xxxxxxxxx"  # Your custom AMI
worker_instance_type  = "c5.2xlarge"
worker_iam_role_name  = "seren-replication-worker"
```

### Plan Deployment

```bash
terraform plan
```

Review the plan to ensure it matches your expectations.

### Apply Configuration

```bash
terraform apply
```

Type `yes` when prompted to create the resources.

### Get Outputs

```bash
terraform output
```

This will display:
- `api_endpoint`: Use this as the value for `SEREN_REMOTE_API` environment variable
- `dynamodb_table_name`: DynamoDB table name
- `lambda_function_name`: Lambda function name
- `worker_iam_role_name`: IAM role for workers

## Testing

```bash
# Get API endpoint
API_ENDPOINT=$(terraform output -raw api_endpoint)

# Test job submission
curl -X POST "${API_ENDPOINT}/jobs" \
  -H "Content-Type: application/json" \
  -d '{
    "command": "init",
    "source_url": "postgresql://user:pass@source:5432/db",
    "target_url": "postgresql://user:pass@target:5432/db",
    "filter": {},
    "options": {}
  }'

# Test job status (replace JOB_ID with actual job ID)
curl "${API_ENDPOINT}/jobs/JOB_ID"
```

## Updating Lambda Code

```bash
# Package new code
cd ../lambda
zip -r lambda.zip handler.py

# Update Lambda
cd ../terraform
terraform apply -target=aws_lambda_function.coordinator
```

## Cleanup

To destroy all resources:

```bash
terraform destroy
```

## Cost Estimation

Monthly costs for typical usage (100 jobs/month):

- **DynamoDB**: ~$1 (on-demand pricing, minimal usage)
- **Lambda**: ~$0.20 (256MB, 30s per invocation)
- **API Gateway**: ~$1 (per million requests)
- **EC2 Workers**: Variable (charged per hour while running)
- **CloudWatch Logs**: ~$0.50 (7-day retention)

**Total fixed costs**: ~$3/month + EC2 worker costs

## Security

- Lambda has minimal IAM permissions (DynamoDB, EC2, IAM PassRole)
- Workers have minimal permissions (DynamoDB updates, CloudWatch Logs)
- API Gateway uses HTTPS only
- No hardcoded credentials (uses IAM roles)

## Troubleshooting

### Lambda deployment fails

Ensure `lambda.zip` exists in `aws/lambda/` directory:
```bash
cd ../lambda && zip -r lambda.zip handler.py && cd ../terraform
```

### Permission errors

Ensure your AWS credentials have permissions to create:
- DynamoDB tables
- Lambda functions
- API Gateway resources
- IAM roles and policies
- CloudWatch Log Groups

### Worker instances fail to start

Ensure `worker_ami_id` points to a valid AMI with:
- `seren-replicator` binary installed at `/opt/seren-replicator/`
- `worker.sh` script at `/opt/seren-replicator/worker.sh`
- PostgreSQL client tools installed
