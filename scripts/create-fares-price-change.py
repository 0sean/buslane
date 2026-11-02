#!/usr/bin/env python3
"""Create a synthetic benchmark ZIP with one direct XML amount increased by 1p."""
import argparse
import copy
import decimal
import json
from pathlib import Path
import re
import shutil
import zipfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("source", type=Path)
parser.add_argument("output", type=Path, help="New file; never use as production fares data")
args = parser.parse_args()
changed = None
with args.source.open("rb") as source_file, args.output.open("xb") as output_file:
    with zipfile.ZipFile(source_file) as source, zipfile.ZipFile(output_file, "w", allowZip64=True) as output:
        for info in source.infolist():
            if changed is None and info.filename.lower().endswith(".xml") and info.file_size <= 64 * 1024 * 1024:
                raw = source.read(info)
                match = re.search(rb"<Amount>(\d+(?:\.\d+)?)</Amount>", raw)
                if match:
                    before = match.group(1).decode("ascii")
                    with decimal.localcontext() as context:
                        context.prec = max(28, len(before) + 4)
                        after = str(decimal.Decimal(before) + decimal.Decimal("0.01"))
                    raw = raw[:match.start(1)] + after.encode("ascii") + raw[match.end(1):]
                    output.writestr(copy.copy(info), raw)
                    changed = {"path": info.filename, "before": before, "after": after}
                    continue
            with source.open(info) as reader, output.open(copy.copy(info), "w", force_zip64=True) as writer:
                shutil.copyfileobj(reader, writer, 1024 * 1024)
if changed is None:
    raise SystemExit("No direct XML Amount found; output has no mutation and must not be used for the update benchmark")
print(json.dumps(changed))
