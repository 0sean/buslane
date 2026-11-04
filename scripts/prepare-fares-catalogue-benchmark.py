#!/usr/bin/env python3
"""Extract the complete ANEA operator archive from a downloaded national ZIP.

Usage: python3 scripts/prepare-fares-catalogue-benchmark.py BULK.zip NEW_DIRECTORY
Creates untouched anea.zip, a controlled one-price update and a one-file removal.
The resulting files are test inputs, never production feed replacements.
"""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import zipfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("bulk", type=Path)
parser.add_argument("output", type=Path)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=False)
with zipfile.ZipFile(args.bulk) as bulk:
    members = [n for n in bulk.namelist() if n.startswith("Arriva UK Bus_63/9936_") and n.endswith(".zip")]
    if len(members) != 1:
        raise SystemExit("Expected exactly one Arriva dataset 9936; inspect the new archive before selecting a replacement")
    original = args.output / "anea.zip"
    original.write_bytes(bulk.read(members[0]))
changed = args.output / "anea-price-update.zip"
change = json.loads(subprocess.check_output([sys.executable, str(Path(__file__).with_name("create-fares-price-change.py")), str(original), str(changed)]))
removed = args.output / "anea-removal.zip"
with zipfile.ZipFile(changed) as source, zipfile.ZipFile(removed, "w") as output:
    missing = source.namelist()[-1]
    for info in source.infolist():
        if info.filename != missing:
            output.writestr(copy.copy(info), source.read(info))
metadata = {"source_member": members[0], "change": change, "removed": missing, "files": {}}
for path in [original, changed, removed]:
    with path.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    metadata["files"][path.name] = {"bytes": path.stat().st_size, "sha256": digest}
(args.output / "inputs.json").write_text(json.dumps(metadata, indent=2) + "\n")
print(json.dumps(metadata, indent=2))
