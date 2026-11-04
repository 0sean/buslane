# Catalogue fixtures

`catalogue.xml` is synthetic: differently named zones have the same published
membership, day/week products have direct typed access rules and distinct prices,
and an unclassified donation is linked to coverage but excluded from travel
navigation.

`catalogue-anea.zip`, `catalogue-nuventure.zip` and `catalogue-stagecoach.zip` retain
unmodified XML bytes from the public BODS fares bulk archive downloaded on
4 November 2026. They contain five, one and one documents respectively. Input
archive names, individual XML SHA-256 hashes, sizes, products, prices and matrix
cell counts are recorded in
`docs/benchmarks/fares-catalogue-inputs-2026-11-04.json`.

The ANEA sample includes the matching 40-stop Durham day/week coverage and two
inbound route-6 matrices. Nu-Venture exercises nested fare tables, an explicit
standard product-type reference without ProductType, and 496 directional price
cells. Stagecoach exercises day-return conditions and a matrix pattern that v1
reports as unsupported instead of inventing prices. These are published source
samples for tests, not assertions of current sale or travel validity.

The Compose contract fixture intentionally imports only `fares.xml`.
