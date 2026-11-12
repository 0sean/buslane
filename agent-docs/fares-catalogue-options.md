# Fares browsing: options and recommendation

Investigation: 4 November 2026. Reviewed Buslane and `/home/sean/bus.rip`, and made bounded, authenticated, read-only requests to the configured Buslane API for ANEA (Arriva North East). Live samples reported fares generation **6**. No application code or production data was changed.

## Recommendation

Add a derived, passenger-facing catalogue to Buslane and make bus.rip consume it. Retain the existing source records and detailed API for provenance and further interpretation.

The primary browsing flow should be **operator → ticket area → tickets**. A separate **single/return fares → route → origin/destination** flow handles journey-dependent fares. The operator page can link to both. An area ticket may appear in multiple applicable areas, but should have one identity and one detail page.

Use “ticket areas” to distinguish these pages from the small fare stages also represented by NeTEx FareZone records. Operators without confidently identifiable ticket areas should still have a ticket list and journey lookup; generating attractive but unsupported areas would misrepresent the source.

## What ANEA actually contains

The complete product and document lists were retrieved; only the first page of zones and selected XML documents were inspected. Sizes below are JSON body bytes before any transport compression, not measured browser-page sizes.

| Observation | Evidence |
| --- | --- |
| 1,054 documents and 1,054 product records | Two pages each from `/fares/documents?noc=ANEA&limit=1000` and `/fares/entities?noc=ANEA&category=product&limit=1000` |
| 143 distinct product names | Complete product list; this is a name count, not a count of semantically distinct tickets |
| 862 single-trip records, 184 period-pass records, 8 records of type `other` | ProductType in each product's source tree |
| `AD SGL` occurs 246 times; `AD SGL REF` 232 times; `U22 SGL` 160 times | Complete product list; shared names do not establish identical rules or prices |
| Products alone total 3,375,613 bytes | Both product pages, before bus.rip compacts them |
| First 1,000 zones total 1,136,042 bytes | One zone page, with further pages available; **not** the complete zone count |
| Zone sample includes Framwellgate Moor, Aykley Heads and Claypath | These are stages in route journey matrices, not all independent pass coverage areas |
| One route-6 adult single file has 112 FareZones and 3,136 matrix elements | Document 93048, XML body 3,613,118 bytes; only three distinct GeographicalIntervalPrice amounts: 1.8, 2.4, 2.5 |
| Different file creation timestamps occur within one route | Route-6 inbound documents 93047/93048/93049 were published at 11:22:35, 11:22:54 and 11:22:45 on 27 October; outbound document 93050 at 11:23:03 |

### A real operator → area → tickets example

| Product | Source document | Zone name | Published stops | Published price |
| --- | --- | --- | ---: | ---: |
| `AD Durham Day Pass` | 93760 | `TNE Co Durham Zone` | 40 | GBP 6.00 |
| `AD Durham WK1 Pass` | 93761 | `Durham Zone` | 40 | GBP 29.00 |

The two zones have **identical sorted stop membership**, despite having different names and product-specific references. Both products explicitly reference their zone through an access FareStructureElement. Their prices are TimeIntervalPrice entries; they do not depend on a matrix's origin/destination zone references.

These are strong candidates for a shared Durham ticket-area page displaying separate day and week tickets. Confirm any further line restrictions and exclusions when resolving full access conditions. Identical published stops establish matching published membership, not a guarantee that every possible use condition is identical or that the stop list represents a complete geographic boundary.

The product list also contains `DONATE £1 Pass`, whose ProductType is `other`. A source product is not automatically a passenger travel ticket. Keep unclassified/other products outside the main travel-ticket list until classified; do not hide every `other` product solely because of this example.

Prices above describe inspected publications, not a claim that every passenger can currently buy them under every condition.

## Why the current implementation struggles

1. **It reconstructs a catalogue on each uncached load.** `src/functions/fares.ts:getFares` retrieves all operator zones and products, then groups them. For large operators, `fareCatalogueOptions` requests publication metadata in batches, but the server still makes one document-detail request per document. For this ANEA snapshot that means 1,054 metadata requests in addition to entity pagination. The catalogue query waits for those batches before returning its result.
2. **Compaction happens after transfer from Buslane.** Buslane includes the source JSON tree on every entity. bus.rip strips much of that tree only after fetching and parsing it.
3. **Show more only paginates rendering.** `Fares.tsx` slices an already loaded group array. It does not bound catalogue loading, and the map also requests the operator's stops.
4. **Details repeat operator-wide work.** `getFareDetail` scans the complete matching entity category to check membership and rediscover group members. It fetches the group's documents, then all prices and labels for the selected publication before the browser filters origin/destination.
5. **Source-document boundaries leak into navigation.** Detail variants are grouped by `[route lines, exact publication_timestamp]`. ANEA's creation timestamps vary seconds apart, so related files stay separate. Publication timestamp is neither effective validity nor a safe fare-edition identity.
6. **Current grouping cannot establish semantic identity.** Product grouping uses a normalized name and product type; zone grouping also requires the name to match. This both misses the Durham coverage match and can group ticket names whose conditions differ. The code correctly preserves source members, but the UI still has to expose and resolve them.
7. **Zone association currently means matrix endpoint.** `zonePrices` checks start/end zone references, directly or through matrices. It does not resolve product → access rule → coverage zone. Consequently the Durham pass prices cannot be discovered by this lookup on their area pages.
8. **Buslane filters are deliberately broad.** NOC, line and stop filters primarily select matching documents, then return their entities. A matching document is a source candidate, not proof that every product in it applies to that operator, line or stop.

Relevant code: [Buslane repository](../apps/api/db/fares/repository.ts), [import projections](../apps/fares/src/netex.rs), and bus.rip's `src/functions/fares.ts`, `src/lib/fares.ts`, `src/data/queries.ts`, `src/components/pages/Fares.tsx` and `src/components/pages/FareDetail.tsx`.

## Options

| Approach | Benefit | Limitation | Assessment |
| --- | --- | --- | --- |
| Improve bus.rip request-time loading | Lazy maps, server pagination, bounded details and caching reduce immediate load | Full grouping still requires discovering duplicates across raw pages; every cache miss reconstructs relationships | Useful short-term relief |
| Build a persistent catalogue in bus.rip | Precomputed grouping and compact pages without changing Buslane | Requires its own ingestion/update job and persistent store; fares interpretation becomes a responsibility of the website | Viable if Buslane must remain a source-only API |
| Add a precomputed catalogue to Buslane | Resolve references once, share canonical groups, serve small indexed responses, keep semantics near the importer | Requires a derived schema and incremental projection work | **Recommended** |
| Maintain curated operator areas and mappings | Can provide good names and known commercial groupings where source data is inconsistent | Manual maintenance and stale mapping risks; does not independently solve raw loading | Use as a small override layer on the catalogue |

Adding gzip, more concurrent fetches, a larger response limit or virtualized lists alone would leave the underlying work and relationship problems in place. Compression remains useful after the response model is bounded.

## Proposed catalogue model

Build a separate projection, rather than merging or deleting imported source entities:

- **Ticket area:** canonical operator-scoped browsing area with a display name, published stop membership, optional source geometry, and source-zone mappings. Grouping by identical membership needs compatible access restrictions and reliable, non-empty membership; empty or incomplete zones must not collapse together. Retain differing source names as aliases. Curated names and mappings can resolve commercial ambiguities.
- **Ticket family:** a navigational grouping such as adult single or a named pass. Similar names can suggest a family without proving that its members are interchangeable.
- **Ticket variant:** the purchasable conditions: passenger eligibility, duration, allowed coverage/lines, direction where relevant, sales channel, effective validity and price basis. Different prices can be channel, date or eligibility variants within one family.
- **Applicability links:** many-to-many relationships between variants, areas, routes and directional journey tables. Matching a document is not an applicability link.
- **Source mappings:** document/entity IDs and resolved reference paths behind every projected item, with unresolved or ambiguous links explicitly recorded.

Resolve relationships through the declared scopes, frames, reference types and versions. Existing local document-scoped price/matrix resolution is useful but is not a complete reference resolver. External or cross-frame ambiguity must remain explicit. Never globally join by an unqualified NeTEx reference or choose the first matching version.

Use stable catalogue IDs independent of whichever source row was first encountered. A stop-membership or price change should update a surviving area's or ticket's version where its identity is established, rather than automatically changing its public URL. Persist mappings and treat merges/splits explicitly.

Keep shared definitions once and attach applicability separately. Do not turn equal prices into equivalent tickets, or infer validity across routes from a shared name. The route-6 example can share three monetary bands internally while retaining the directional origin/destination mapping that selects each band.

### Illustrative API

These are proposed routes, not existing endpoints:

```text
GET /fares/operators/ANEA/areas?limit=50&cursor=...
GET /fares/operators/ANEA/areas/:areaId/tickets?limit=50&cursor=...
GET /fares/operators/ANEA/tickets/:ticketId
GET /fares/operators/ANEA/areas/:areaId/geometry
GET /fares/operators/ANEA/routes/:routeId/origins
GET /fares/operators/ANEA/routes/:routeId/destinations?origin=...
GET /fares/operators/ANEA/journey-fares?route_id=...&origin=...&destination=...&date=...
```

Area lists return identifiers, names and counts, without raw XML trees, all stop IDs, all source-document IDs or price matrices. Ticket lists return ready-to-display structured summaries with exact decimal prices/currency where resolved. Conditions and provenance belong on detail responses. Unknown price or applicability is explicit, not silently converted to a minimum price or universal validity.

Pagination and search must operate on canonical groups in the API. Deduplicating independently within raw pages would still repeat groups across pages. Geometry can load separately for the selected area; list content should remain usable while maps load. A map of all areas can use a dedicated bounded summary layer rather than every stop and source polygon.

Journey requests return only relevant published fares for the chosen direction and date. Initially support the verified ANEA matrix patterns; unsupported conditions return a clear unresolved result. A complete best-fare engine including arbitrary capping and complex products is a separate project.

### Publication and caching

Prefer derived tables inside the existing fares database if they can be maintained in the importer's publication transaction. Add a projection version and update affected documents/groups incrementally; unchanged input should retain the importer's zero-write behavior. Measure initial build and incremental costs before applying nationally.

If projection is deployed as a separate build first, publish a complete catalogue snapshot atomically, identify the raw generation it used, and continue serving the previous complete catalogue while building. Do not expose half-updated source/group mappings. This is also the deployment pattern needed by a bus.rip-owned catalogue.

Use effective tariff/frame validity to select dated views, preserving conflicts and future/expired offers. Do not use exact publication timestamps as edition identity. Cache keys and pagination need both catalogue generation and query conditions; conditional requests can avoid returning unchanged results. Authentication stays server-side in bus.rip.

## Suggested delivery order

1. Prototype the ANEA projection offline against an existing imported snapshot. Prove the Durham area example, journey-matrix classification and source provenance before choosing final grouping rules.
2. Add catalogue storage, stable IDs, bounded area/ticket endpoints and incremental publication. Keep the current detailed API available.
3. Change bus.rip to load area summaries first, tickets on area selection and geometry separately. Remove catalogue-wide reconstruction and per-document publication fetching from page requests.
4. Add bounded route/origin/destination lookup for singles and returns, preserving boarding/alighting direction. Do not fetch whole matrices into the browser.
5. Expand to other operators and add narrow curation only where evidence shows it is needed.

Acceptance checks should cover the two Durham source zones appearing under one candidate area; their day/week variants and distinct prices; repeated singles appearing as families with retained route/direction differences; generation changes, deleted publications and stable links; conflicting dates/channels remaining separate; and fixed page/request bounds independent of the number of source documents. Measure end-to-end bytes, request count and first usable content, rather than only warm latency for a single raw API page.

## Source and limits

The [BODS NeTEx Fares Profile v1.1](https://www.pti.org.uk/system/files/files/BODS_NeTEx_Fares_profile_v1_1.pdf), sections 5.2, 6.2 and 6.3.2, explicitly allows FareZones to represent fare stages or larger areas, describes tariff validity, and distinguishes zone/line access rules from journey matrices. This supports the distinction used above.

Live ANEA evidence was sampled from generation 6 on the investigation date. Complete product/document lists were inspected, alongside one 1,000-record zone page, four document details and four source XML files. This was not a full national analysis, a complete ANEA reference-resolution audit, a measured browser performance test or an implementation of the proposed catalogue. The national import benchmark documents a retained database on another host; that database was not available locally in this workspace.
