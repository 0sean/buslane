# Legacy cloud deployment and retirement

These Terraform definitions describe the previous Cloudflare Worker/Hyperdrive/KV and GCP import deployment. New builds produce a Node server and local importer; the root build/deploy scripts no longer apply these definitions.

Keep the running legacy infrastructure during cutover. The current API build output is a Node server and must not be redeployed as a Cloudflare Worker. Preserve the legacy Worker artifact and its matching Git revision separately before making any legacy deployment changes.

## Retirement after cutover

1. Seed and validate SQLite, verify the self-hosted origin through the existing tunnel, and switch `lane.transit.fyi` to that origin.
2. Verify authenticated operator/trip endpoints, a successful scheduled incremental import, API restarts and a backup restoration. Retain PostgreSQL and the Worker until those checks pass.
3. Preserve Terraform state, legacy deployment artifacts and any database backup needed for rollback. The GCP temporary bucket uses `force_destroy=true`; its contents will be deleted during retirement.
4. In the existing Terraform working directory, run `terraform plan -destroy -out=retire.tfplan` and inspect it. The intended retired resources are the Worker/custom-domain assignment, Hyperdrive, timetable KV namespace, Cloud Run job, scheduler, temporary bucket, artifact repository and associated service accounts/IAM grants.
5. The legacy configuration also manages GCP project API enablement. Its resources use `disable_on_destroy=false`, preserving those APIs. The existing tunnel and its DNS configuration are not managed here; verify the destruction plan does not affect your replacement ingress.
6. Apply the reviewed retirement plan only after the new origin is verified. Check the tunnel hostname and API again. Retire PostgreSQL separately once its rollback retention period is over.

No script in this repository automatically destroys cloud resources or changes tunnel/DNS assignments.
