# Passenger fares catalogue and bus.rip migration

Buslane now derives a passenger catalogue in the fares importer. The raw documents, original XML, source entity IDs and existing endpoints remain available. The catalogue is an evidence-backed browsing interface with bounded journey price discovery; it does not evaluate every NeTEx fare rule or select a best fare.

## Model and publication

Schema 2 adds derived tables to the existing fares database. Schema 3 adds [precomputed operator fare maps](operator-fare-geometry.md), with a rebuild from existing catalogue rows and timetable coordinates. This keeps source changes, canonical groups, applicability, membership, matrices and the dataset generation in one WAL transaction. Existing readers see the previous committed snapshot until commit; any failed projection rolls back source and catalogue changes together. A second database would require a second publication protocol and could expose mismatched generations.

* **Areas** are operator-scoped navigation groups created from positive explicit access assignments. Matrix endpoints become journey points, not browsing areas. Unclassified/non-travel products remain discoverable in the operator ticket list with the explicit opt-in; they do not create travel browsing areas. Non-empty, explicitly published stop sets can group across different source zone names only when their access structures and line/direction restrictions agree. Empty/incomplete membership stays separate and is labelled incomplete. Published membership is not a geographical boundary.
* **Families** group normalized product names and product type for navigation. They explicitly declare `equivalence: false`. Area-family lists count the applicable canonical variants and paginate families before loading variants.
* **Tickets** are condition variants, separated by product, sales/price-table context, eligibility, duration, coverage, direction, channels, effective validity, prices and other reachable source conditions. Full content signatures retain source condition structure; equal names or amounts alone never merge variants. Conservative signatures may keep commercially equivalent records separate.
* **Applicability** is stored as indexed ticket → area/family/route relationships. Routes use source line references; their IDs are catalogue IDs, not timetable route IDs. Points retain directional matrix zone membership. No timetable/route applicability is inferred from shared document membership. Shared tariffs and multi-product sales packages do not grant one component another component’s areas; unsupported bundles retain explicit unresolved diagnostics.
* **Provenance** maps canonical items back to document IDs, typed source anchors, NeTEx refs and versions. Source lists are independently paginated and retain the selected sales/price-table context and its version. Differently named area sources remain visible there. Original XML is the complete evidence for rules the projection cannot interpret.

References resolve by element type, requested `version`/`versionRef` and frame scope within a document. A unique cross-frame definition can resolve; multiple candidates remain ambiguous. Explicit `frameRef` is honoured. No unqualified cross-document join or first-version selection is performed. Known public stop and locality references can be retained as external identifiers without inventing local definitions. Missing/ambiguous conditions are reported; unresolved ticket signatures are document-scoped to avoid asserting equivalence.

An item's first public ID is a deterministic content-derived opaque ID, independent of SQLite row/insertion order. Persisted source identities allow the ID to survive price, membership, effective-version and other updates. On a split, a surviving semantic partition retains its existing ID; remaining partitions receive distinct IDs. Merges choose a deterministic surviving ID. `/changes` records explicit `split`, `merge`, `removed` and `restored` events; inactive IDs return 410. Revisions describe changes to a surviving item. If upstream changes both source identity and semantics, continuity cannot safely be inferred: an old item can be removed and a new one created. Duplicate definitions of the same type/ref/version/frame have content-qualified anchors: changed ambiguous definitions receive new identities rather than inheriting the first source position. Preserve the database/identity registry in backups; a fresh rebuild cannot reproduce all historical reconciliation decisions.

Unchanged archives/XML retain the existing fast paths. Changed document facts, source relationships, membership and journey cells are reconciled by set difference. Only affected equivalence classes and navigation groups are republished; unchanged derived rows are preserved. Catalogue-only upgrades skip rewriting unchanged source documents/entities. Initial/backfill projection still processes the complete input; it is not free.

## HTTP API

All routes below start with `/api/fares/operators/:noc` and require the existing bearer token. `:noc` is case sensitive and path-scoped; a query parameter cannot replace it. Lists are JSON arrays unless noted. Default `limit=50`, maximum 100. All pagination selects canonical rows, never a page of raw records subsequently deduplicated.

| Path | Content |
| --- | --- |
| `/areas` | Area IDs, display names, membership status, stop and ticket counts |
| `/areas/:areaId/families` | Navigation families and applicable `variant_count` |
| `/areas/:areaId/tickets` | Applicable condition variants; optional `family_id` |
| `/families`, `/families/:familyId/tickets` | Operator families and their variants, including tickets without areas |
| `/tickets` | Operator condition variants, including tickets without areas |
| `/tickets/:ticketId` | Summary, conditions, interpretation limits, source count |
| `/tickets/:ticketId/areas`, `/routes`, `/families` | Separately paginated relationships (relative to the ticket path) |
| `/areas/:id`, `/families/:id`, `/routes/:id`, `/points/:id` | Individual catalogue item |
| `/areas/:areaId/stops`, `/points/:pointId/stops` | Separately paginated published stop codes/names |
| `/areas/:areaId/geometry`, `/points/:pointId/geometry` | Explicit geometry availability; v1 returns `geometry: null`, `status: unavailable` |
| `/tickets/:id/sources`, `/areas/:id/sources`, `/families/:id/sources`, `/points/:id/sources` | Paginated source mappings; use document IDs with the existing detailed/XML API |
| `/routes` | Catalogue route discovery, with source public line codes |
| `/routes/:routeId/origins` | Available origin points |
| `/routes/:routeId/destinations?origin=:pointId` | Available destinations in that direction |
| `/journey-fares?route_id=:routeId&origin=:pointId&destination=:pointId&date=YYYY-MM-DD` | Object containing a paginated `candidates` array and explicit interpretation status |
| `/geometry` | Complete precomputed operator area coverage, points, labels and missing-coordinate diagnostics; see [map guide](operator-fare-geometry.md) |
| `/changes` | Paginated lifecycle events for this operator |

Operator/area/family ticket lists and route lists support case-insensitive **name-prefix** search with `q` (maximum 120 characters). Search is an indexed prefix range, not fuzzy/substring search. `include_unclassified=true` also exposes `other`/unknown product types; these are excluded from travel navigation by default, not permanently classified as donations. A missing ProductType can be classified from one of the explicitly supported standard TypeOfFareProductRef values; `product_type_source` reports that evidence. The `travel` flag controls recognised travel navigation; false also includes unclassified products and does not prove that a product has no travel right. Product types outside the recognised travel set require review.

Ticket summaries include exact decimal `prices`, currency, `price_basis`, eligibility, channels, duration rules, effective date evidence, line references, `family_id`, `journey_support` and `conditions_evaluated: false`. Journey price bands are not advertised as a universal flat fare or converted to a minimum. Missing currency remains unresolved. Summary price/condition arrays are capped; `prices_truncated`, detail `conditions_truncated`, unresolved counts and provenance disclose omitted evidence. List bodies never contain XML trees, complete source lists or matrices.

`X-Next-Cursor` supplies a keyset cursor when more rows exist. Cursors bind the endpoint, operator, filters, limit, projection version (via dataset generation) and snapshot. `X-Fares-Generation` and `X-Catalogue-Generation` are the same atomic dataset generation; `X-Catalogue-Version: 1` identifies interpretation rules. Pass `generation=N` on subsequent independent detail/navigation requests to require a consistent generation. A change returns **409**; restart pagination/navigation. Cursors are not archival snapshot handles. Malformed input returns 400, unknown IDs 404, inactive IDs 410, unavailable projection 503.

Responses include private revalidation ETags; authenticated `If-None-Match` requests can return 304. Worker cache keys include the generation and every filter. A publication invalidates cached results by generation even if a particular item's content stayed unchanged. An ETag never bypasses authentication or generation checks.

### Journey support and date meaning

V1 supports explicit `DistanceMatrixElementPrice` cells whose product/price-table context, matrix, origin/end zones, monetary price and currency resolve uniquely, with one unambiguous line per matrix context. Multiple price/matrix/endpoint references and multi-line matrix contexts are reported as unresolved rather than selecting a first binding. Amounts can be directly present or referenced by a typed monetary-price reference. The matrix remains server-side. Forward and reverse pairs are separate; the lookup never swaps endpoints or fills absent fares by symmetry. Multiple published prices are retained as candidates, not arbitrarily reduced to one. Stop membership for points loads separately.

`journey_support` counts resolved cells and marks partial/unsupported patterns. Cells without usable monetary references are not silently assigned a band's price. Nested row/column profile or sales selectors that cannot be separated safely remain conditional catalogue offerings; v1 does not attribute every amount to every passenger/channel or quote their matrix cells. PriceGroup-only matrices without explicit supported price cells, ambiguous references, unresolved line ownership and arbitrary fare/capping rules are outside this implementation. Empty journey results have status `no_supported_published_fare` and `unsupported_patterns_possible: true`; they do not prove no ticket exists.

Journey candidates are classified against published FromDate/ToDate evidence for the requested day. `outside_published_validity` indicates date evidence requiring exclusion/review; `published_candidate` is still conditional. Dates are never derived from publication timestamps. Time-of-day, timezone/boundary details, Boolean validity alternatives, eligibility, concessions, sales channels, transfer/capping and other conditions are not fully evaluated. Consumers must not present these candidates as an unconditional payable fare. Full condition structure remains in source XML; compact condition summaries are evidence, not an executable rule AST.

### Examples

```sh
curl -i -H "Authorization: Bearer $AUTH_KEY" \
 'http://127.0.0.1:3000/api/fares/operators/ANEA/areas?limit=50'
curl -i -H "Authorization: Bearer $AUTH_KEY" \
 "http://127.0.0.1:3000/api/fares/operators/ANEA/areas/$AREA_ID/families?limit=50&generation=$GENERATION"
curl -i -H "Authorization: Bearer $AUTH_KEY" \
 "http://127.0.0.1:3000/api/fares/operators/ANEA/areas/$AREA_ID/tickets?family_id=$FAMILY_ID&generation=$GENERATION"
curl -H "Authorization: Bearer $AUTH_KEY" \
 "http://127.0.0.1:3000/api/fares/operators/ANEA/journey-fares?route_id=$ROUTE_ID&origin=$ORIGIN_ID&destination=$DESTINATION_ID&date=2026-11-04"
```

A compact ticket has this shape (IDs abbreviated here):

```json
{
  "id": "ticket_…", "family_id": "family_…", "name": "AD Durham Day Pass",
  "revision": 1, "product_type": "periodPass", "travel": true,
  "price_basis": "published", "prices": [{"amount": "6.00", "currency": "GBP"}],
  "eligibility": ["adult"], "sales_channels": ["onBoard"],
  "conditions_evaluated": false, "resolution": "published_conditions"
}
```

These are published prices and conditions from inspected data, not a claim of unrestricted current sale or travel validity.

## Migration for bus.rip

1. Before opening the database with the new binary, back it up using the currently deployed schema-1 importer: `fares --backup NEW_PATH`. New importer maintenance commands can migrate on open, so use the old binary for a rollback-compatible backup. Deploy the matching schema-2 importer and API together. Run the importer against the original/current complete feed to build the catalogue; do not import a partial operator archive into a national production database because absent publications are intentionally removed. An existing schema-1 database migrates automatically. Catalogue-only backfill preserves unchanged raw rows. Restart API workers with the matching build after migration; old readers intentionally reject schema 2.
2. Replace `getFares` operator-wide entity downloads and per-document metadata requests with one `/areas?limit=50` request. Display the first page immediately; use `X-Next-Cursor` for “show more”. Retain `X-Catalogue-Generation`. Offer `/families` or `/tickets` as a fallback for operators without confidently derived areas, and a separate route/journey entry point.
3. On selecting an area, load `/areas/:id/families` for grouped navigation, then `/areas/:id/tickets?family_id=...` for explicit variants. Alternatively show the area-ticket page directly. Families are navigation groups; never collapse their variants into one price. Ticket IDs are public URLs; do not use source row IDs, product names, publication timestamps or client-generated deduplication keys.
4. Load `/tickets/:id` for a selected variant. Load its provenance or related areas/routes only when requested. Remove the catalogue-wide membership checks, raw price scans and document-publication batching from `getFareDetail`.
5. Load `/areas/:id/stops` only when its map is opened. Membership is paginated and can be matched to bus.rip's existing stop/coordinate data by ATCO/NaPTAN code. V1 does not publish a generated polygon or fetch the operator's entire stop set for a catalogue page. Geometry unavailability must leave the ticket list usable.
6. For singles/returns, load `/routes`, its `/origins`, then `/destinations?origin=...`, then the exact `/journey-fares` pair/date. Do not download matrices. Show unresolved/unsupported and conditional results explicitly; do not label them guaranteed cheapest fares.
7. Keep the bearer token on bus.rip's server. Key caches by operator, catalogue generation/version, endpoint, all filters and cursor. Carry `generation=N` between area, family, variant and detail loads. On 409 restart navigation and clear dependent cursors. On 410 use `/changes` to reconcile known URLs; a split can offer multiple replacements and must not automatically redirect to the first. Revalidate ETags with authentication.

First usable area content needs one HTTP request. Direct area → variants → detail needs three; grouped area → families → variants → detail needs four. Stop/provenance pages are optional additional requests. These counts are interface bounds, not a measured bus.rip browser migration; no bus.rip code is changed here.

For rollback, restore the checked pre-migration backup and matching old API/importer binaries. Do not downgrade a live schema-2 database in place. Keep SQLite main/WAL/SHM together during normal operation and use online backups, not copies of a live main file.

## Verification

See [the reproducible benchmark report](benchmarks/fares-catalogue-2026-11-04.md) for real snapshot provenance, full-operator measurements, SQL plans, response bytes, request counts and limits. ANEA, Nu-Venture and Stagecoach fixtures exercise different publisher structures. Full national projection and Raspberry Pi performance remain unverified.
