# Development environment configuration
# Usage: terraform apply -var-file=environments/dev.tfvars

aws_region           = "us-east-1"
project_name         = "seren-replication-dev"
dynamodb_table_name  = "replication-jobs-dev"
worker_instance_type = "t3.medium"  # Smaller instance for dev
max_concurrent_jobs  = 3           # Lower limit for dev

# Worker AMI ID - update after building AMI
# worker_ami_id = "ami-xxxxx"
