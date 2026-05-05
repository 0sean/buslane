#!/usr/bin/env bash

set -euo pipefail

FARES_INPUT_URL="${FARES_INPUT_URL:-https://data.bus-data.dft.gov.uk/fares/download/bulk_archive}"
WORKDIR="${FARES_WORKDIR:-/tmp/fares}"

rm -rf "$WORKDIR"
mkdir -p "$WORKDIR/data"
cd "$WORKDIR"

echo "Downloading BODS fares archive from $FARES_INPUT_URL"
wget -O fares.zip "$FARES_INPUT_URL"

echo "Unpacking fares archive"
unzip -q -d data fares.zip

/usr/local/bin/fares "$WORKDIR/data"

rm -rf "$WORKDIR"

echo "Invalidating KV cache"
wget -q -O - --method=POST --header="Authorization: Bearer $AUTH_KEY" https://lane.transit.fyi/api/invalidate
