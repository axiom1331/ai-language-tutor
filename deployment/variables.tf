variable "aws_region" {
  description = "AWS region"
  type        = string
  default     = "us-east-1"
}

variable "instance_type" {
  description = "EC2 instance type"
  type        = string
  default     = "t3.medium"
}

variable "ami_id" {
  description = "AMI ID for Ubuntu 22.04 LTS"
  type        = string
  default     = "ami-0866a3c8686eaeeba" # Ubuntu 22.04 LTS in us-east-1
}

variable "key_name" {
  description = "SSH key pair name"
  type        = string
}
