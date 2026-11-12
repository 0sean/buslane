# Production upgrade to the passenger fares catalogue

This is the upgrade path for the existing `/opt/buslane` Docker Compose installation on the ARM64 Pi. It upgrades the fares database from schema 1 to schema 2 and publishes catalogue interpretation version 1. Keep the existing timetable database, authentication key, data-directory mounts and public hostname. No timetable reimport is required.

Deploy Buslane first, verify the catalogue endpoint, then deploy bus.rip's `v3-fares` changes. The old detailed fares endpoints remain available to the previous consumer. The new consumer requires catalogue version 1.

## 1. Pause scheduled fares imports and make an old-format backup

Do this **before replacing the old fares importer image**. Even `--backup` and `--check` in the new importer can migrate a schema-1 database when opening it.

```sh
cd /opt/buslane
# If the fares timer is installed:
sudo systemctl stop buslane-fares-import.timer
systemctl is-active buslane-fares-import.service
```

Wait for any running import to finish, including manually launched containers. Lock contention logs `another fares importer is running; skipped` and exits successfully, so check that the following commands actually report a created backup and successful database checks. Use a new backup filename if this one exists.

Using the **currently deployed schema-1 image** and existing Compose configuration:

```sh
docker compose --profile jobs run --rm --no-deps fares-importer \
  --backup /data/fares-before-catalogue.sqlite
docker compose --profile jobs run --rm --no-deps fares-importer \
  --db /data/fares-before-catalogue.sqlite --check
```

The backup is in the host directory configured by `FARES_DATA_DIR`, not the timetable data directory. Keep it and the previous API/importer images or image archives for rollback. Do not copy the live main SQLite file as a substitute for an online backup.

Check free space on the fares data and scratch SSDs. The existing raw national database was measured at 69.8 GiB on an older snapshot. The new catalogue's national storage and Pi backfill duration have **not** been measured. The complete ANEA test added 1.03 GiB, but that is not a national sizing estimate. Allow space for the backup, catalogue growth, transient WAL, downloaded archive, scratch and reserve. The 512 MiB `MIN_FREE_BYTES` setting is only a floor, not a capacity estimate.

## 2. Update the checkout and build/load matching images

```sh
cd /opt/buslane
git pull --ff-only origin main
```

Prefer building on your ARM64-capable builder and loading the resulting archives on the Pi. Run these in the updated checkout on the builder:

```sh
docker buildx build --platform linux/arm64 -f apps/api/Dockerfile \
  -t buslane-api:local --output type=docker,dest=/tmp/buslane-api-arm64.tar .
docker buildx build --platform linux/arm64 -f apps/fares/Dockerfile \
  -t buslane-fares:local --output type=docker,dest=/tmp/buslane-fares-arm64.tar .
```

Transfer both archives to the Pi using your usual method, then load them:

```sh
docker load -i /path/to/buslane-api-arm64.tar
docker load -i /path/to/buslane-fares-arm64.tar
```

Those tags match `.env.example`. If your `.env` uses custom `BUSLANE_API_IMAGE` / `BUSLANE_FARES_IMAGE` values, build/tag/load the images under those configured names instead. The timetable importer image does not need replacing for this upgrade. An alternative on a suitably provisioned ARM64 host is `docker compose --profile jobs build api fares-importer`.

## 3. Backfill before starting the new API

The straightforward in-place upgrade uses an API maintenance window. Build/load the images first, then:

```sh
cd /opt/buslane
docker compose stop api
docker compose --profile jobs run --rm --no-deps fares-importer \
  --source https://data.bus-data.dft.gov.uk/fares/download/bulk_archive
docker compose --profile jobs run --rm --no-deps fares-importer --check
```

Wait for both commands to finish successfully. The import automatically migrates the schema and projects the catalogue even if the original XML is unchanged. It preserves unchanged raw rows, original XML and the identity registry. This first projection processes the full input and can take a substantial time; the ANEA-only elapsed time is not a national/Pi estimate. Detailed integrity checks also scan the full database.

Use the **complete original/current feed**, as above. Do not use the small fixtures, ANEA-only benchmark archive, or controlled price/removal archives against production: absent publications are intentionally deleted. Do not replace the production database with a fresh benchmark database; keeping the existing registry preserves historical public-ID reconciliation.

Schema-1 readers are not supported against schema 2. Do not restart the old API against the upgraded database. If the job fails, investigate its output or restore the checked old backup and matching old images; do not treat a migrated-but-unpublished projection as ready.

## 4. Recreate the API and verify fares explicitly

```sh
docker compose up -d --no-build --force-recreate api
docker compose logs --tail=100 api
curl --fail http://127.0.0.1:3000/up
```

`/up` retains timetable readiness semantics, so also test the catalogue. With the existing configured token available as `AUTH_KEY` in your shell (and adjusting the port if `API_PORT` differs):

```sh
curl --fail-with-body -i -H "Authorization: Bearer $AUTH_KEY" \
  'http://127.0.0.1:3000/api/fares/operators/ANEA/areas?limit=1'
```

Expect HTTP 200, a compact JSON array, `X-Catalogue-Version: 1` and matching `X-Catalogue-Generation` / `X-Fares-Generation` headers. Repeat against your public Buslane hostname to verify the tunnel/origin path. No DNS/tunnel change or new token is required by this upgrade.

Then deploy the pushed bus.rip `v3-fares` branch through your normal pipeline. Its existing `BUSLANE_BASE_URL` and `BUSLANE_API_KEY` stay server-only and unchanged. There is no bus.rip database migration. Check operator areas, Durham day/week tickets, and a directional journey lookup. Numeric legacy fare links intentionally offer fresh catalogue navigation.

## 5. Resume daily imports

If already installed:

```sh
sudo systemctl enable --now buslane-fares-import.timer
systemctl list-timers buslane-fares-import.timer
journalctl -u buslane-fares-import.service -n 100 --no-pager
```

If the fares units have not been installed yet:

```sh
sudo install -m 644 deploy/systemd/buslane-fares-import.service \
  deploy/systemd/buslane-fares-import.timer /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now buslane-fares-import.timer
```

The existing fares schedule is 04:00 UTC. Daily imports now maintain the catalogue in the same transaction and generation as the source records. An identical input remains zero-write; no separate catalogue service, scheduled job, migration command or cache-flush step is needed.

## Rollback

Stop the API and fares timer and wait for any importer to finish. Preserve the upgraded database and its WAL/SHM sidecars together. Restore the checked schema-1 backup as the configured `fares.sqlite`, ensuring the stopped upgraded WAL/SHM files are not left attached to the restored file. Restore the previous API and fares-importer images, check the restored database with the **old** importer to initialise compatible sidecars, then restart the old API and timer. Restore/deploy the old bus.rip consumer too if the new consumer was released. Do not attempt an in-place schema downgrade.

See [the API/migration guide](fares-catalogue.md) and [measured operator-scale results](benchmarks/fares-catalogue-2026-11-04.md) for interpretation limits and verification evidence.
