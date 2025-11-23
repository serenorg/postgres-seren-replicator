# Deployment Ready Status

## Current Status

✅ **ALL CODE AND INFRASTRUCTURE COMPLETE**

All development work for remote replication is finished and ready to deploy.

## What's Ready

### 1. Release Binary
- ✅ Built: `target/release/seren-replicator` (v2.4.2)
- ✅ Tested: All commands functional
- ✅ Size: ~11MB optimized binary

### 2. Infrastructure Code
- ✅ Lambda function: `aws/lambda/handler.py`
- ✅ Terraform configuration: `aws/terraform/*.tf`
- ✅ EC2 worker script: `aws/ec2/worker.sh`
- ✅ AMI build automation: `aws/ec2/build-ami.sh` + `setup-worker.sh`

### 3. Automation Scripts
- ✅ Deployment automation: `aws/deploy.sh`
- ✅ Integration tests: `aws/test-integration.sh`
- ✅ All scripts syntax-validated
- ✅ All scripts executable

### 4. Documentation
- ✅ Main README: `aws/README.md` (architecture, usage, costs)
- ✅ Lambda guide: `aws/lambda/README.md`
- ✅ Terraform guide: `aws/terraform/README.md`
- ✅ EC2 guide: `aws/ec2/README.md`
- ✅ Testing guide: `aws/TESTING.md`

## Deployment Command

Once AWS credentials are configured, deployment is a single command:

```bash
./aws/deploy.sh
```

This will automatically:
1. ✅ Check prerequisites (already satisfied)
2. ⏳ Build worker AMI with Packer (~10 minutes)
3. ⏳ Package Lambda function
4. ⏳ Deploy infrastructure with Terraform (~5 minutes)
5. ⏳ Test API endpoint
6. ✅ Provide API URL and monitoring links

## AWS Credentials Setup

### Required Permissions

The AWS user/role needs permissions to create:
- **EC2**: Instances, AMIs, instance profiles
- **Lambda**: Functions, update code
- **API Gateway**: HTTP APIs, routes, integrations
- **DynamoDB**: Tables, TTL configuration
- **IAM**: Roles, policies, instance profiles
- **CloudWatch**: Log groups

### Configuration Options

**Option 1: AWS Configure (Recommended)**
```bash
aws configure
# AWS Access Key ID: [your-key]
# AWS Secret Access Key: [your-secret]
# Default region name: us-east-1
# Default output format: json
```

**Option 2: Environment Variables**
```bash
export AWS_ACCESS_KEY_ID="your-access-key"
export AWS_SECRET_ACCESS_KEY="your-secret-key"
export AWS_REGION="us-east-1"
```

**Option 3: AWS SSO**
```bash
aws sso login --profile your-profile
export AWS_PROFILE=your-profile
```

### Verify Credentials

```bash
aws sts get-caller-identity
# Should show: UserId, Account, Arn
```

## Expected Costs

### One-Time Costs
- **AMI Build**: ~$0.10 (t3.medium for ~10 minutes)

### Monthly Fixed Costs
- **DynamoDB**: ~$1-2 (on-demand, minimal usage)
- **API Gateway**: ~$1 (first million requests free)
- **Lambda**: ~$0.20-1 (256MB, 30s invocations)
- **CloudWatch Logs**: ~$0.50 (7-day retention)
- **Total Fixed**: ~$3-5/month

### Variable Costs (Per Job)
- **EC2 Worker** (c5.2xlarge): $0.34/hour = $0.0057/minute
  - 30-minute job: ~$0.17
  - 2-hour job: ~$0.68
  - 8-hour job: ~$2.72

### Example Monthly Totals
- **Light** (10 jobs, 30 min avg): ~$7/month
- **Medium** (50 jobs, 1 hour avg): ~$22/month
- **Heavy** (200 jobs, 2 hours avg): ~$141/month

## Deployment Steps

### Step 1: Configure AWS (Required)

```bash
# Check if already configured
aws sts get-caller-identity

# If not, configure now
aws configure
```

### Step 2: Deploy Infrastructure

```bash
# Single command deployment
cd /Users/taariqlewis/Projects/Seren_Projects/neon-seren-replicator
./aws/deploy.sh
```

Expected output:
```
[TIMESTAMP] Starting deployment of remote replication infrastructure
[TIMESTAMP] Region: us-east-1

[TIMESTAMP] Checking prerequisites...
[TIMESTAMP] ✓ All prerequisites satisfied

[TIMESTAMP] Building release binary...
[TIMESTAMP] ✓ Built binary version: 2.4.2

[TIMESTAMP] Building worker AMI (takes ~10 minutes)...
... (Packer output) ...
[TIMESTAMP] ✓ AMI created: ami-0123456789abcdef0

[TIMESTAMP] Packaging Lambda function...
[TIMESTAMP] ✓ Lambda packaged: 2.3K

[TIMESTAMP] Deploying infrastructure with Terraform...
... (Terraform output) ...
[TIMESTAMP] ✓ Infrastructure deployed successfully

Outputs:
  API Endpoint: https://abcdef1234.execute-api.us-east-1.amazonaws.com
  DynamoDB Table: replication-jobs
  Lambda Function: seren-replication-coordinator

[TIMESTAMP] Testing API endpoint...
[TIMESTAMP] ✓ API is responding correctly

==========================================
Deployment Complete!
==========================================
```

### Step 3: Export API Endpoint

```bash
# Set environment variable for CLI usage
export SEREN_REMOTE_API=$(cat aws/.api_endpoint)

# Or get from Terraform
export SEREN_REMOTE_API=$(cd aws/terraform && terraform output -raw api_endpoint)
```

### Step 4: Test Remote Replication

```bash
# Option A: Run automated integration tests
./aws/test-integration.sh

# Option B: Manual test with real databases
seren-replicator init --remote \
  --source "postgresql://user:pass@source:5432/db" \
  --target "postgresql://user:pass@target:5432/db" \
  --yes
```

## Monitoring

### AWS Console Links (After Deployment)

**EC2 Instances:**
```
https://console.aws.amazon.com/ec2/home?region=us-east-1#Instances:tag:ManagedBy=seren-replication-system
```

**DynamoDB Table:**
```
https://console.aws.amazon.com/dynamodbv2/home?region=us-east-1#table?name=replication-jobs
```

**Lambda Function:**
```
https://console.aws.amazon.com/lambda/home?region=us-east-1#/functions/seren-replication-coordinator
```

**CloudWatch Logs:**
```
https://console.aws.amazon.com/cloudwatch/home?region=us-east-1#logsV2:log-groups/log-group/$252Faws$252Flambda$252Fseren-replication-coordinator
```

### CLI Monitoring

```bash
# Watch EC2 workers
watch -n 5 'aws ec2 describe-instances \
  --filters "Name=tag:ManagedBy,Values=seren-replication-system" \
  --query "Reservations[].Instances[].[InstanceId,State.Name,Tags[?Key==\`JobId\`].Value|[0]]" \
  --output table'

# Query job status
aws dynamodb scan --table-name replication-jobs \
  --query 'Items[].{JobId:job_id.S,Status:status.S,Created:created_at.S}' \
  --output table

# Follow Lambda logs
aws logs tail /aws/lambda/seren-replication-coordinator --follow
```

## Cleanup (When Done Testing)

To remove all infrastructure and stop charges:

```bash
cd aws/terraform
terraform destroy
# Type 'yes' to confirm

# Optionally delete AMIs
aws ec2 describe-images --owners self \
  --filters "Name=name,Values=seren-replicator-worker-*" \
  --query 'Images[].ImageId' --output text | \
  xargs -n1 aws ec2 deregister-image --image-id
```

## Troubleshooting

### "Unable to locate credentials"
```bash
aws configure
# Enter your AWS access key and secret
```

### "InsufficientPermissions"
Ensure your AWS user has all required permissions (see above).

### "AMI build fails"
Check Packer logs. Common issues:
- Wrong region
- VPC/subnet restrictions
- SSH key issues

### "Terraform apply fails"
Check:
- Lambda zip file exists: `aws/lambda/lambda.zip`
- AMI ID is valid
- No conflicting resources

### "Integration tests fail"
Ensure:
- Docker is running
- API endpoint is set: `echo $SEREN_REMOTE_API`
- Infrastructure is deployed

## Success Criteria

After successful deployment:

- ✅ AMI created and tagged
- ✅ Lambda function deployed and invocable
- ✅ API Gateway responding to requests
- ✅ DynamoDB table created with TTL
- ✅ IAM roles and instance profiles created
- ✅ API endpoint URL available
- ✅ Integration tests pass

## Next Steps After Deployment

1. **Test with real databases**: Use `--remote` flag for actual replication
2. **Monitor costs**: Check AWS Cost Explorer daily
3. **Set up alerts**: CloudWatch alarms for failed jobs
4. **Scale testing**: Try with larger databases
5. **Documentation**: Update with production API URL
6. **CI/CD**: Integrate deployment into pipeline

## Support

- **Issues**: https://github.com/serenorg/seren-replicator/issues
- **Documentation**: See README files in each aws/ subdirectory
- **AWS Docs**: https://docs.aws.amazon.com/

---

**Status**: All code complete. Infrastructure ready to deploy with a single command once AWS credentials are configured.
