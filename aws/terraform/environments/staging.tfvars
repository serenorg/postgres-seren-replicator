# Staging environment configuration
# Usage: terraform apply -var-file=environments/staging.tfvars

aws_region           = "us-east-1"
project_name         = "seren-replication-staging"
dynamodb_table_name  = "replication-jobs-staging"
worker_instance_type = "c5.large"  # Cost-effective for staging
max_concurrent_jobs  = 5           # Moderate limit for staging

# Worker AMI ID - update after building AMI
# worker_ami_id = "ami-xxxxx"
