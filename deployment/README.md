# AWS EC2 Deployment

## Prerequisites

1. AWS CLI configured with credentials
2. Terraform installed
3. SSH key pair created in AWS

## Setup

1. Copy the example variables file:
   ```bash
   cp terraform.tfvars.example terraform.tfvars
   ```

2. Edit `terraform.tfvars` and set your SSH key name

3. Initialize Terraform:
   ```bash
   terraform init
   ```

4. Review the deployment plan:
   ```bash
   terraform plan
   ```

5. Deploy:
   ```bash
   terraform apply
   ```

6. Get the instance IP:
   ```bash
   terraform output instance_public_ip
   ```

## Cleanup

To destroy resources:
```bash
terraform destroy
```
