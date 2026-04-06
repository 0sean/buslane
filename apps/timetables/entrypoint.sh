#!/usr/bin/env bash

set -euo pipefail

mkdir /tmp/timetables
cd /tmp/timetables

wget -O data.zip https://data.bus-data.dft.gov.uk/timetable/download/gtfs-file/all/
unzip data.zip

exec /usr/local/bin/timetables /tmp/timetables/data
