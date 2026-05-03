terraform {
  required_version = ">= 1.5"

  required_providers {
    cloudflare = {
      source  = "cloudflare/cloudflare"
      version = "~> 5.0"
    }
    google = {
      source  = "hashicorp/google"
      version = "~> 6.0"
    }
  }

  cloud {
    
    organization = "Transit"

    workspaces {
      name = "Buslane"
    }
  }
}

provider "cloudflare" {
  api_token = var.cloudflare_api_token
}

provider "google" {
  project = var.gcp_project_id
  region  = var.gcp_region
}
