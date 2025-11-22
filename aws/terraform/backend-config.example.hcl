# Terraform backend configuration example
#
# This configures remote state storage in S3 with DynamoDB locking.
#
# Usage:
#   1. Copy this file: cp backend-config.example.hcl backend-config.hcl
#   2. Update the values below with your actual S3 bucket and DynamoDB table
#   3. Initialize: terraform init -backend-config=backend-config.hcl
#   4. IMPORTANT: Add backend-config.hcl to .gitignore (contains sensitive info)
#
# Prerequisites:
#   - S3 bucket for state storage (with versioning enabled)
#   - DynamoDB table for state locking (with LockID as partition key)
#
# Create resources:
#   aws s3 mb s3://your-terraform-state-bucket --region us-east-1
#   aws s3api put-bucket-versioning \
#     --bucket your-terraform-state-bucket \
#     --versioning-configuration Status=Enabled
#
#   aws dynamodb create-table \
#     --table-name terraform-state-lock \
#     --attribute-definitions AttributeName=LockID,AttributeType=S \
#     --key-schema AttributeName=LockID,KeyType=HASH \
#     --billing-mode PAY_PER_REQUEST \
#     --region us-east-1

bucket         = "your-terraform-state-bucket"
key            = "seren-replication/terraform.tfstate"
region         = "us-east-1"
encrypt        = true
dynamodb_table = "terraform-state-lock"

# Optional: Enable state file versioning for recovery
# versioning = true
