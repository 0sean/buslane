#!/usr/bin/env bash
set -euo pipefail
repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
task_dir="$(mktemp -d)"
export TIMETABLES_DATA_DIR="$task_dir/data"
export TIMETABLES_SCRATCH_DIR="$task_dir/scratch"
export FARES_DATA_DIR="$task_dir/fares-data"
export FARES_SCRATCH_DIR="$task_dir/fares-scratch"
export BUSLANE_UID="$(id -u)" BUSLANE_GID="$(id -g)"
export AUTH_KEY=compose-fixture-token SENTRY_DSN= API_PORT=0 MIN_FREE_BYTES=0
export BUSLANE_API_IMAGE=buslane-api:local BUSLANE_IMPORTER_IMAGE=buslane-importer:local
export BUSLANE_FARES_IMAGE=buslane-fares:local
mkdir -p "$TIMETABLES_DATA_DIR" "$TIMETABLES_SCRATCH_DIR" "$FARES_DATA_DIR" "$FARES_SCRATCH_DIR"
cp -R "$repo_dir/tests/fixtures/gtfs" "$task_dir/feed"
cp -R "$repo_dir/tests/fixtures/netex" "$task_dir/fares-feed"
cat > "$task_dir/override.yaml" <<EOF
services:
  importer:
    volumes:
      - $task_dir/feed:/feed:ro
  fares-importer:
    volumes:
      - $task_dir/fares-feed:/feed:ro
EOF
compose=(docker compose --project-directory "$repo_dir" --project-name "buslane-test-$$" -f "$repo_dir/compose.yaml" -f "$task_dir/override.yaml")
cleanup() {
  result=$?
  if ((result != 0)); then "${compose[@]}" logs api || true; fi
  "${compose[@]}" down --remove-orphans >/dev/null 2>&1 || true
  rm -rf "$task_dir"
  exit "$result"
}
trap cleanup EXIT
if [[ "${1:-}" == "--build" ]]; then "${compose[@]}" --profile jobs build; fi
"${compose[@]}" --profile jobs run --rm --no-deps importer --source /feed
"${compose[@]}" --profile jobs run --rm --no-deps fares-importer --source /feed
"${compose[@]}" up -d --no-build api
for attempt in {1..60}; do
  if "${compose[@]}" exec -T api node -e "fetch('http://127.0.0.1:3000/up').then(r=>process.exit(r.ok?0:1)).catch(()=>process.exit(1))" >/dev/null 2>&1; then break; fi
  sleep 1
done
"${compose[@]}" exec -T api node --input-type=module - <<'JS'
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { createRequire } from 'node:module';
const require=createRequire('/app/apps/api/package.json');
const Database=require('better-sqlite3');
const db=new Database('/data/timetables.sqlite',{readonly:true,fileMustExist:true});
assert.throws(()=>db.exec('DELETE FROM stops'),/readonly/i);db.close();
assert.throws(()=>fs.writeFileSync('/data/write-attempt','forbidden'),/EROFS|EACCES/);
const base='http://127.0.0.1:3000';
assert.equal((await fetch(base+'/api/timetables/stops')).status,400);
const response=await fetch(base+'/api/timetables/trips/T1',{headers:{Authorization:'Bearer compose-fixture-token'}});
assert.equal(response.status,200);assert.equal(response.headers.get('x-timetable-generation'),'1');
const trip=await response.json();assert.equal(trip.stop_times[0].arrival_time,'25:00:00');assert.equal(trip.route_id,101);
const fares=new Database('/fares/fares.sqlite',{readonly:true,fileMustExist:true});
assert.throws(()=>fares.exec('DELETE FROM documents'),/readonly/i);fares.close();
assert.throws(()=>fs.writeFileSync('/fares/write-attempt','forbidden'),/EROFS|EACCES/);
const products=await fetch(base+'/api/fares/products?noc=ONE&line=1',{headers:{Authorization:'Bearer compose-fixture-token'}});
assert.equal(products.status,200);assert.equal(products.headers.get('x-fares-generation'),'1');assert.equal((await products.json()).length,2);
for(const path of ['/api/timetables/agencies/A1/fares','/api/timetables/routes/101/fares','/api/timetables/stops/S1/fares']) {
  const linked=await fetch(base+path,{headers:{Authorization:'Bearer compose-fixture-token'}});
  assert.equal(linked.status,200);assert.equal((await linked.json()).length,2);
}
const price=await fetch(base+'/api/fares/prices?ref=price%3Aadult',{headers:{Authorization:'Bearer compose-fixture-token'}});
assert.equal((await price.json())[0].amount,'2.00');
JS
sed -i 's/First Stop/Fresh Stop/' "$task_dir/feed/stops.txt"
"${compose[@]}" --profile jobs run --rm --no-deps importer --source /feed
sed -i 's/<Amount>2.00<\//<Amount>2.50<\//' "$task_dir/fares-feed/fares.xml"
"${compose[@]}" --profile jobs run --rm --no-deps fares-importer --source /feed
"${compose[@]}" restart api
"${compose[@]}" exec -T api node --input-type=module - <<'JS'
import assert from 'node:assert/strict';
let response;
for(let i=0;i<60;i++) {
  try{response=await fetch('http://127.0.0.1:3000/api/timetables/stops',{headers:{Authorization:'Bearer compose-fixture-token'}});if(response.ok)break;}catch{}
  await new Promise(r=>setTimeout(r,100));
}
assert.equal(response.status,200);assert.equal(response.headers.get('x-timetable-generation'),'2');
assert.equal((await response.json())[0].name,'Fresh Stop');
const price=await fetch('http://127.0.0.1:3000/api/fares/prices?ref=price%3Aadult',{headers:{Authorization:'Bearer compose-fixture-token'}});
assert.equal(price.status,200);assert.equal(price.headers.get('x-fares-generation'),'2');assert.equal((await price.json())[0].amount,'2.50');
JS
repeat="$("${compose[@]}" --profile jobs run --rm --no-deps importer --source /feed)"
[[ "$repeat" == *'"changed_files":0'* ]]
"${compose[@]}" --profile jobs run --rm --no-deps importer --backup /data/backup.sqlite
"${compose[@]}" --profile jobs run --rm --no-deps importer --db /data/backup.sqlite --check
fares_repeat="$("${compose[@]}" --profile jobs run --rm --no-deps fares-importer --source /feed)"
[[ "$fares_repeat" == *'"sqlite_changes":0'* ]]
"${compose[@]}" --profile jobs run --rm --no-deps fares-importer --backup /data/backup.sqlite
"${compose[@]}" --profile jobs run --rm --no-deps fares-importer --db /data/backup.sqlite --check
printf 'Compose integration checks passed: timetable and fares import, API contract, read-only mounts, update, restart, no-op and backup restoration.\n'
