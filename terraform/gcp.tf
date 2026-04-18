resource "google_project_service" "run" {
  service            = "run.googleapis.com"
  disable_on_destroy = false
}

resource "google_project_service" "cloudscheduler" {
  service            = "cloudscheduler.googleapis.com"
  disable_on_destroy = false
}

resource "google_project_service" "artifactregistry" {
  service            = "artifactregistry.googleapis.com"
  disable_on_destroy = false
}

resource "google_project_service" "storage" {
  service            = "storage.googleapis.com"
  disable_on_destroy = false
}

resource "google_artifact_registry_repository" "buslane" {
  location      = var.gcp_region
  repository_id = "buslane"
  description   = "Buslane Docker images"
  format        = "DOCKER"

  depends_on = [google_project_service.artifactregistry]
}

resource "google_storage_bucket" "timetables_tmp" {
  name          = "${var.gcp_project_id}-timetables-tmp"
  location      = var.gcp_region
  storage_class = "STANDARD"
  force_destroy = true

  lifecycle_rule {
    condition {
      age = 2
    }
    action {
      type = "Delete"
    }
  }

  depends_on = [google_project_service.storage]
}

resource "google_service_account" "timetables" {
  account_id   = "buslane-timetables"
  display_name = "Buslane Timetables Job"
}

resource "google_storage_bucket_iam_member" "timetables_tmp" {
  bucket = google_storage_bucket.timetables_tmp.name
  role   = "roles/storage.objectAdmin"
  member = "serviceAccount:${google_service_account.timetables.email}"
}

locals {
  timetables_image = "${var.gcp_region}-docker.pkg.dev/${var.gcp_project_id}/${google_artifact_registry_repository.buslane.repository_id}/timetables:${var.timetables_image_tag}"
}

resource "google_cloud_run_v2_job" "timetables" {
  name     = "buslane-timetables"
  location = var.gcp_region

  template {
    template {
      service_account = google_service_account.timetables.email

      containers {
        image = local.timetables_image

        env {
          name  = "DATABASE_URL"
          value = var.database_url
        }

        resources {
          limits = {
            cpu    = "1"
            memory = "1Gi"
          }
        }

        volume_mounts {
          name       = "tmp"
          mount_path = "/tmp"
        }
      }

      volumes {
        name = "tmp"
        gcs {
          bucket    = google_storage_bucket.timetables_tmp.name
          read_only = false
        }
      }

      # Allow up to 1 hour for a full GTFS import
      timeout     = "3600s"
      max_retries = 0
    }
  }

  depends_on = [google_project_service.run]
}

resource "google_service_account" "scheduler" {
  account_id   = "buslane-scheduler"
  display_name = "Buslane Cloud Scheduler"
}

resource "google_cloud_run_v2_job_iam_member" "scheduler_invoker" {
  project  = var.gcp_project_id
  location = var.gcp_region
  name     = google_cloud_run_v2_job.timetables.name
  role     = "roles/run.invoker"
  member   = "serviceAccount:${google_service_account.scheduler.email}"
}

resource "google_cloud_scheduler_job" "timetables_daily" {
  name             = "buslane-timetables-daily"
  description      = "Run timetables GTFS import daily at 2am UTC"
  schedule         = "0 2 * * *"
  time_zone        = "UTC"
  attempt_deadline = "1800s"
  region           = var.gcp_region

  http_target {
    http_method = "POST"
    uri         = "https://run.googleapis.com/v2/projects/${var.gcp_project_id}/locations/${var.gcp_region}/jobs/${google_cloud_run_v2_job.timetables.name}:run"

    oauth_token {
      service_account_email = google_service_account.scheduler.email
    }
  }

  depends_on = [google_project_service.cloudscheduler]
}
