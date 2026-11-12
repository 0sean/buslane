# Passenger catalogue verification — 4 November 2026

This report accompanies [the API and bus.rip migration guide](../fares-catalogue.md). Raw measurements, executed SQL and query plans are saved in `fares-catalogue-2026-11-04.json`; input hashes and fixture provenance are in [the input manifest](fares-catalogue-inputs-2026-11-04.json). Large feeds, databases and build scratch remain on ephemeral SSD storage, outside the repository.

## Inputs and scope

The BODS national bulk archive was retrieved on 4 November 2026 from `https://data.bus-data.dft.gov.uk/fares/download/bulk_archive`. The complete ANEA dataset was selected from `Arriva UK Bus_63/9936_129314_2026-11-03_16-02-31_current.zip`: 83,839,787 compressed bytes, 1,276,468,929 XML bytes, 5,761 documents and 1,441,080 source entities. Its SHA-256 is `545080a022db98c618dbb204d910cf1d262e45c83be757ec0c9c4bb308fbe24c`.

This revalidation differs from the earlier generation-6 investigation: 5,761 products, 138 distinct names, 949 singleTrip, 4,637 periodPass and 175 other products. AD SGL occurs 236 times, AD SGL REF 224 and U22 SGL 160. A name count is still not a semantic ticket count. The three Durham fixture publications share 40 published stops; adult day and week publications contain GBP 6.00 and 29.00 respectively. Their direct access rules produce one browsing area while retaining distinct ticket variants. The two route-6 fixture documents contain 3,136 matrix elements each; only cells with usable monetary references are projected.

Untouched XML fixtures from ANEA, Nu-Venture and Stagecoach are retained in `tests/fixtures/netex/catalogue-*.zip`. Nu-Venture exercises nested price tables and an inferred standard single-ticket type (496 supported directed cells). Stagecoach exercises a return product whose PriceGroup matrix pattern is explicitly unsupported: no monetary fare is invented. These are representative structural samples, not complete imports of those two operators.

The full national archive was **not** projected. National throughput, Raspberry Pi memory/latency, a deployed bus.rip browser flow, sustained production concurrency and OS-cache-cold disk performance remain unverified.

## Method

Host: Linux ARM64, four Neoverse-V2 cores, local NVMe `/mnt/ssd`. Local tools: Rust/Cargo 1.99.1, Node 24.11.0, Bun 1.4.4, Python 3.13.5; native API SQLite 3.53.2. Container integration independently builds with the repository's pinned Docker toolchain (Rust 1.94, Node 24, Bun 1.3.6).

The release importer is measured with `wait4` elapsed/CPU/max-RSS statistics and sampled WAL sizes. The schedule is initial import, identical archive, one controlled amount change (1.8 → 1.81), removal of one XML publication, then restoration of the original archive. Each is one sample, not a confidence interval. Tests/container compilation can overlap the import benchmark, so elapsed numbers are host observations, not isolated throughput guarantees. Projection time is importer instrumentation for deriving, storing and publishing catalogue facts; it excludes the remaining raw import work.

The HTTP harness starts one built, authenticated API process with its default workers (two timetable workers and one fares worker), reads and parses every JSON response, then measures 100 warm requests per scenario across five clients. It also measures ten sequential requests after explicit cache invalidation. “Cold” means the first request to each scenario in that process: later scenarios reuse initialized workers, and OS caches are not cleared. Warm cache hits can bypass the catalogue repository. Peak API RSS covers the whole process and all workers.

## Reproduce

Use new output directories for each run. Download the bulk archive separately; the manifest identifies the measured snapshot. The preparation script deliberately fails if dataset 9936 cannot be selected uniquely from a future archive.

```sh
cargo build --release --locked --manifest-path apps/fares/Cargo.toml
python3 scripts/prepare-fares-catalogue-benchmark.py \
  /mnt/ssd/buslane-catalogue/bulk.zip /mnt/ssd/catalogue-inputs-new
python3 scripts/benchmark-fares.py \
  --feed /mnt/ssd/catalogue-inputs-new/anea.zip \
  --feed /mnt/ssd/catalogue-inputs-new/anea-price-update.zip \
  --feed /mnt/ssd/catalogue-inputs-new/anea-removal.zip \
  --feed /mnt/ssd/catalogue-inputs-new/anea.zip \
  --output /mnt/ssd/catalogue-import-new
cargo build --locked --manifest-path apps/timetables/Cargo.toml
bun run --cwd apps/api build
node --import ./apps/api/node_modules/tsx/dist/loader.mjs \
  scripts/benchmark-fares-catalogue.mjs \
  --database /mnt/ssd/catalogue-import-new/fares.sqlite \
  --output /mnt/ssd/catalogue-api-new --samples 100
apps/fares/target/release/fares \
  --db /mnt/ssd/catalogue-import-new/fares.sqlite --check
bun run test
bun run check-types
bash scripts/test-compose.sh --build
```

The preparation script was executed and reproduced the original, changed and removal ZIP hashes in the raw report. Do not use these controlled modification archives as production feeds. Do not import an operator-only archive into a national production database: missing documents are intentionally removed.

## Measured import and storage

| Operation | Wall seconds | Peak RSS MiB | Projection ms | SQLite changes | Generation |
| --- | ---: | ---: | ---: | ---: | ---: |
| Initial complete ANEA | 142.674 | 184.6 | 35,064 | 6,106,194 | 1 |
| Identical archive | 0.065 | 11.3 | 0 | 0 | 1 |
| One amount update | 1.860 | 57.2 | 131 | 313 | 2 |
| One document removed | 1.484 | 36.2 | 30 | 1,659 | 3 |
| Original archive restored | 1.761 | 66.0 | 169 | 1,970 | 4 |

The initial database occupied 3,669,655,552 bytes (3.42 GiB), with a peak pre-checkpoint WAL of 3,691,116,272 bytes (3.44 GiB). After update/removal/restoration it occupied 3,669,839,872 bytes. Budget for the WAL as well as the main database during publication. The repeat run left generation 1 unchanged and created no WAL writes; incremental runs published generations 2–4.

## Measured authenticated HTTP

The final catalogue contains **242 areas, 138 navigation families, 5,078 ticket variants, 96 route contexts, 6,210 journey points and 544,246 directed priced cells**. Counts are canonical projection outputs, not an assertion that every variant is a distinct commercial ticket. There are 54,455 source contributions.

| Scenario (limit 50 where applicable) | First ms | Warm p50 ms | Warm p95 ms | Invalidated p95 ms | Body bytes | Repository SQL |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Operator areas | 16.30 | 2.27 | 7.18 | 1.81 | 11,907 | 2 |
| Operator tickets | 2.00 | 2.51 | 3.80 | 2.02 | 46,547 | 2 |
| Ticket prefix search | 2.06 | 2.03 | 4.01 | 1.98 | 48,206 | 2 |
| Routes | 1.37 | 1.55 | 3.33 | 1.86 | 10,298 | 2 |
| Area families | 1.71 | 1.43 | 4.80 | 1.23 | 3,547 | 3 |
| Area tickets | 1.30 | 1.07 | 1.92 | 1.23 | 12,944 | 3 |
| Area stops | 0.70 | 0.89 | 2.29 | 1.19 | 1,705 | 3 |
| Ticket detail | 0.94 | 0.91 | 2.56 | 1.06 | 8,032 | 2 |
| Route origins | 1.38 | 0.98 | 3.97 | 1.06 | 10,316 | 3 |
| Directed destinations | 0.93 | 0.91 | 2.62 | 1.05 | 4,483 | 4 |
| Exact journey pair | 1.06 | 0.95 | 1.75 | 0.95 | 1,116 | 5 |

Peak API process RSS was **323.1 MiB**, including all three workers. Derived table/index allocation was **1,105,809,408 bytes (1.03 GiB)** versus 2,563,846,144 source/schema bytes (2.39 GiB): **43.1% additional allocated storage** relative to the source category. `dbstat` includes table indexes and SQLite autoindexes; total database size also includes free pages/headers. This storage tradeoff keeps raw evidence, source contributions for incremental reconciliation and indexed canonical journey cells together.

HTTP measurements cover the selected first page/detail/pair, not every area or every journey. Bodies are uncompressed JSON. The lower sequential invalidated timings reflect different concurrency and warmed runtime/OS state; they are not evidence that cache misses are intrinsically faster. Full uncached p50/p95 values are in the raw JSON.

## Query work and optimisations

Catalogue HTTP reads operate on materialised canonical rows and indexed applicability, membership, route-pair and journey tables. Instrumentation rejects any catalogue SQL reading raw `entities`, `documents` or `links`. Actual SQL, parameters and `EXPLAIN QUERY PLAN` output are retained for every measured scenario. Repository statement counts exclude the worker's shared schema/generation/snapshot checks; they are not total connection statement counts.

Measurement identified and corrected four material costs:

* An unscoped materialised-table DELETE scanned the whole journey table despite a scoped subquery. Moving the affected-object predicate to the outer DELETE removed that scan; a plan regression test protects it.
* Refreshing all tickets sharing a family/point on a source-count change unnecessarily republished siblings. Refresh now propagates only when source-to-public-ID mappings change.
* After SQLite statistics were collected, endpoint joins could start with unrelated source anchors and form a large intermediate result. Explicit source-cell-first joins prevent that ordering; a regression test verifies it.
* A redundant search tuple range encouraged an operator skip-scan and temporary sort on SQLite 3.53.2. Prefix pagination now uses two bounded index seeks (same name after ID, then subsequent names).

Development measurements on the same ANEA input exposed a 387.1-second initial import / 91.6-second removal before the scoped-DELETE fix, then a 145.2-second initial import / 63.0-second removal with unnecessary sibling refresh still present. Removing sibling propagation and correcting join ordering brought one-document removal to approximately 1.5 seconds in subsequent runs. These are diagnostic runs across evolving implementations, not a controlled comparison of identical final schemas. Selected raw diagnostic measurements are preserved alongside the final results.

The final measured page plans use indexed searches, without full scans or temporary sorts. A page does not parse XML, enumerate all operator source documents, download a matrix or deduplicate raw pages. Membership, provenance and geometry availability are separate requests. B-tree seeks and at most the requested canonical page determine work, although an individual bounded detail can carry more condition evidence than a list row.

First usable areas require one request; direct areas → tickets → detail requires three; grouped areas → families → tickets → detail requires four. Each first stop/provenance page adds one request. These are API-flow counts, not measurements of bus.rip's existing browser implementation.

## Checks and remaining costs

The final repository suite passes 67 tests (one existing opt-in test ignored), including source import → publication → authenticated built HTTP responses. It covers canonical pagination, direct access/deduplication, eligibility/channel/date/duration separation, namespace/selector conditions, ambiguous typed/versioned references, incomplete zones, direction, multiple sales contexts, stable identities, split/merge/removal/restoration, rollback, schema migration/backfill, zero-write repeat imports, ETags and generation conflicts. Type checks, Rust formatting, changed JavaScript/TypeScript formatting and Docker Compose integration pass. The full measured ANEA database passes integrity and foreign-key checks.

The derived tables intentionally duplicate some indexed source journey facts to support incremental reconciliation and bounded reads. Their additional storage is material and reported above. No claim of storage minimality or full national optimisation is made. Complicated selectors and unsupported matrices remain explicit, document-local unresolved references remain conservative, geometry polygons are unavailable, and published candidates still require passenger/channel/time/rule interpretation. This is not a universal payable-fare or capping engine.
