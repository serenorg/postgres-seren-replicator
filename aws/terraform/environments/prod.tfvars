# Production environment configuration
# Usage: terraform apply -var-file=environments/prod.tfvars

aws_region           = "us-east-1"
project_name         = "seren-replication"
dynamodb_table_name  = "replication-jobs"
worker_instance_type = "c5.2xlarge"  # Default production instance
max_concurrent_jobs  = 10            # Production limit

# Worker AMI ID - update after building AMI
# worker_ami_id = "ami-xxxxx"
