variable "database_url" {
  description = "PostgreSQL connection URL (postgresql://user:password@host:port/dbname)"
  type        = string
  sensitive   = true
}

variable "api_key" {
  description = "Bearer token for API authentication (AUTH_KEY)"
  type        = string
  sensitive   = true
}

variable "cloudflare_api_token" {
  description = "Cloudflare API token with Workers and Hyperdrive permissions"
  type        = string
  sensitive   = true
}

variable "cloudflare_account_id" {
  description = "Cloudflare account ID"
  type        = string
}

variable "cloudflare_zone_id" {
  description = "Cloudflare zone ID for transit.fyi"
  type        = string
}

variable "gcp_project_id" {
  description = "Google Cloud project ID"
  type        = string
}

variable "gcp_region" {
  description = "Google Cloud region for Cloud Run and Artifact Registry"
  type        = string
  default     = "europe-west1"
}

variable "sentry_dsn" {
  description = "Sentry DSN for the API app"
  type        = string
  sensitive   = true
}

variable "timetables_image_tag" {
  description = "Docker image tag for the timetables job"
  type        = string
  default     = "latest"
}

variable "fares_image_tag" {
  description = "Docker image tag for the fares job"
  type        = string
  default     = "latest"
}

variable "fares_input_url" {
  description = "BODS fares bulk archive URL"
  type        = string
  default     = "https://data.bus-data.dft.gov.uk/fares/download/bulk_archive"
}
