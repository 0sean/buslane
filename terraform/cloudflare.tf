locals {
  # Parse database_url into components for Hyperdrive
  # Expected format: postgresql://user:password@host:port/dbname
  db = regex(
    "^(?P<scheme>[^:]+)://(?P<user>[^:]+):(?P<password>[^@]+)@(?P<host>[^:/]+)(?::(?P<port>[0-9]+))?/(?P<database>[^?]+)",
    var.database_url
  )
}

resource "cloudflare_hyperdrive_config" "timetables_db" {
  account_id = var.cloudflare_account_id
  name       = "buslane-timetables-db"

  origin = {
    scheme   = local.db.scheme
    host     = local.db.host
    port     = tonumber(coalesce(local.db.port, "5432"))
    database = local.db.database
    user     = local.db.user
    password = local.db.password
  }
}

resource "cloudflare_worker" "api" {
  account_id = var.cloudflare_account_id
  name       = "buslane-api"

  observability = {
    enabled = true
  }
}

resource "cloudflare_worker_version" "api" {
  account_id  = var.cloudflare_account_id
  worker_id   = cloudflare_worker.api.id

  # Build the API first: cd apps/api && bun run build
  modules = [
    {
      name         = "worker.mjs"
      content_file = "../apps/api/.output/server/index.mjs"
      content_type = "application/javascript+module"
    }
  ]
  main_module = "worker.mjs"

  compatibility_date  = "2026-04-17"
  compatibility_flags = ["nodejs_compat"]

  bindings = [
    {
      type = "hyperdrive"
      name = "HYPERDRIVE"
      id   = cloudflare_hyperdrive_config.timetables_db.id
    },
    {
      type = "plain_text"
      name = "AUTH_KEY"
      text = var.api_key
    },
    {
      type = "plain_text"
      name = "SENTRY_DSN"
      text = var.sentry_dsn
    }
  ]
}

resource "cloudflare_workers_deployment" "api" {
  account_id  = var.cloudflare_account_id
  script_name = cloudflare_worker.api.name
  strategy    = "percentage"

  versions = [
    {
      version_id = cloudflare_worker_version.api.id
      percentage = 100
    }
  ]
}

resource "cloudflare_workers_custom_domain" "api" {
  account_id = var.cloudflare_account_id
  hostname   = "lane2.transit.fyi"
  service    = cloudflare_worker.api.name
  zone_id    = var.cloudflare_zone_id

  depends_on = [cloudflare_workers_deployment.api]
}
