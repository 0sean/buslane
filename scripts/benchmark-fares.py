#!/usr/bin/env python3
"""Repeatable Linux import benchmark; put --output on local SSD storage."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import time
import threading


def measure(command, directory, name):
    started = time.monotonic()
    pid = os.fork()
    if pid == 0:
        with (directory / f"{name}.stdout.json").open("wb") as out, (directory / f"{name}.stderr.log").open("wb") as err:
            os.dup2(out.fileno(), 1)
            os.dup2(err.fileno(), 2)
            os.environ["TMPDIR"] = str(directory)
            os.environ["SQLITE_TMPDIR"] = str(directory)
            os.execv(command[0], command)
    wal = Path(command[command.index("--db") + 1] + "-wal")
    peak_wal = 0
    stopped = threading.Event()
    def sample_wal():
        nonlocal peak_wal
        while not stopped.is_set():
            try:
                peak_wal = max(peak_wal, wal.stat().st_size)
            except FileNotFoundError:
                pass
            stopped.wait(0.1)
    monitor = threading.Thread(target=sample_wal, daemon=True)
    monitor.start()
    _, status, usage = os.wait4(pid, 0)
    elapsed = time.monotonic() - started
    stopped.set()
    monitor.join()
    result = {
        "command": command,
        "exit_code": os.waitstatus_to_exitcode(status),
        "elapsed_seconds": elapsed,
        "user_seconds": usage.ru_utime,
        "system_seconds": usage.ru_stime,
        "peak_rss_kib": usage.ru_maxrss,
        "block_inputs": usage.ru_inblock,
        "block_outputs": usage.ru_oublock,
        "peak_wal_bytes": peak_wal,
    }
    if result["exit_code"] == 0:
        result["import"] = json.loads((directory / f"{name}.stdout.json").read_text())
        result["peak_wal_bytes"] = max(peak_wal, result["import"].get("wal_bytes_before_checkpoint", 0))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--feed", action="append", required=True, help="Local ZIP/XML/directory; repeat for subsequent daily feeds")
    parser.add_argument("--output", required=True, type=Path, help="New results/database/scratch directory, preferably /mnt/ssd/...")
    parser.add_argument("--binary", type=Path, default=Path(__file__).resolve().parents[1] / "apps/fares/target/release/fares")
    args = parser.parse_args()
    directory = args.output.resolve()
    directory.mkdir(parents=True, exist_ok=False)
    scratch = directory / "scratch"
    scratch.mkdir()
    binary = str(args.binary.resolve(strict=True))
    feeds = [str(Path(feed).resolve(strict=True)) for feed in args.feed]
    results = {"machine": platform.platform(), "processor": platform.machine(), "feeds": [], "runs": []}
    for feed in feeds:
        path = Path(feed)
        results["feeds"].append({"path": feed, "bytes": path.stat().st_size if path.is_file() else None,
                                  "sha256": hashlib.file_digest(path.open("rb"), "sha256").hexdigest() if path.is_file() else None})
    database = directory / "fares.sqlite"
    schedule = [("initial", feeds[0]), ("identical", feeds[0])]
    schedule.extend((f"update-{i}", feed) for i, feed in enumerate(feeds[1:], 1))
    for name, feed in schedule:
        command = [binary, "--db", str(database), "--source", feed, "--scratch", str(scratch)]
        result = measure(command, directory, name)
        result["name"] = name
        result["database_bytes"] = database.stat().st_size
        wal = Path(str(database) + "-wal")
        result["wal_bytes"] = wal.stat().st_size if wal.exists() else 0
        results["runs"].append(result)
        (directory / "results.json").write_text(json.dumps(results, indent=2) + "\n")
        if result["exit_code"]:
            raise SystemExit(f"{name} failed; inspect {directory / (name + '.stderr.log')}")
        report = result["import"]["report"]
        print(f"{name}: {result['elapsed_seconds']:.3f}s, {result['peak_rss_kib']/1024:.1f} MiB RSS, {report['sqlite_changes']} SQLite changes", flush=True)
        if name == "identical" and (report["sqlite_changes"] != 0 or report["generation"] != results["runs"][0]["import"]["report"]["generation"]):
            raise SystemExit("FAIL: identical feed changed the database")
        if result["peak_rss_kib"] >= 512 * 1024:
            raise SystemExit("FAIL: importer exceeded the 512 MiB memory target")
    print(directory / "results.json")


if __name__ == "__main__":
    main()
