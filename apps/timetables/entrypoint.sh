#!/usr/bin/env bash

set -euo pipefail

rm -rf /tmp/timetables

mkdir /tmp/timetables
cd /tmp/timetables

wget -O data.zip https://data.bus-data.dft.gov.uk/timetable/download/gtfs-file/all/
mkdir data
unzip -d data data.zip

/usr/local/bin/timetables /tmp/timetables/data

rm -rf /tmp/timetables

echo "Invalidating KV cache"
wget -q -O - --method=POST --header="Authorization: Bearer $AUTH_KEY" https://lane.transit.fyi/api/invalidate