output "api_url" {
  description = "API base URL"
  value       = "https://lane.transit.fyi"
}

output "worker_name" {
  description = "Cloudflare Worker name"
  value       = cloudflare_worker.api.name
}

output "hyperdrive_id" {
  description = "Cloudflare Hyperdrive config ID"
  value       = cloudflare_hyperdrive_config.timetables_db.id
}

output "worker_version_id" {
  description = "Deployed Cloudflare Worker version ID"
  value       = cloudflare_worker_version.api.id
}

output "timetables_job_name" {
  description = "Cloud Run job name"
  value       = google_cloud_run_v2_job.timetables.name
}

output "fares_job_name" {
  description = "Cloud Run fares job name"
  value       = google_cloud_run_v2_job.fares.name
}

output "artifact_registry_repo" {
  description = "Artifact Registry repository URL for pushing images"
  value       = "${var.gcp_region}-docker.pkg.dev/${var.gcp_project_id}/${google_artifact_registry_repository.buslane.repository_id}"
}

output "timetables_image" {
  description = "Full image path for the timetables job"
  value       = local.timetables_image
}

output "fares_image" {
  description = "Full image path for the fares job"
  value       = local.fares_image
}
