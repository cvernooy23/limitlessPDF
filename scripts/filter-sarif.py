#!/usr/bin/env python3
"""Remove ASH-suppressed findings from SARIF before uploading to GitHub.

ASH applies the suppressions in .ash.yaml to its aggregated report
(ash.sarif) by attaching a SARIF `suppressions` entry to each suppressed
result. The per-scanner SARIF files it also writes under .ash/ carry no
suppression markers, and GitHub code scanning opens alerts for suppressed
results either way. This script collects every result ASH suppressed
(keyed on rule id, file path and start line), removes those results from
every SARIF file, and writes the filtered copies to a separate directory
with the same relative layout. The original ASH output is left untouched.

Usage: filter-sarif.py <ash-dir> <out-dir>
"""

import json
import os
import sys
from pathlib import Path

# SARIF suppression.status values that do NOT suppress a result.
# An absent status means "accepted" per the SARIF 2.1.0 spec.
INACTIVE_STATUSES = {"rejected", "underreview"}


def is_suppressed(result):
    for suppression in result.get("suppressions") or []:
        status = str(suppression.get("status") or "accepted").lower()
        if status not in INACTIVE_STATUSES:
            return True
    return False


def normalize_uri(uri, workspace):
    uri = uri or ""
    if uri.startswith("file://"):
        uri = uri[len("file://") :]
    if workspace:
        prefix = workspace.rstrip("/") + "/"
        if uri.startswith(prefix):
            uri = uri[len(prefix) :]
    while uri.startswith("./"):
        uri = uri[2:]
    return uri.lstrip("/")


def finding_key(result, workspace):
    locations = result.get("locations") or [{}]
    physical = locations[0].get("physicalLocation") or {}
    uri = (physical.get("artifactLocation") or {}).get("uri")
    line = (physical.get("region") or {}).get("startLine")
    rule = result.get("ruleId") or (result.get("rule") or {}).get("id")
    return (rule, normalize_uri(uri, workspace), line)


def main():
    if len(sys.argv) != 3:
        sys.exit("usage: filter-sarif.py <ash-dir> <out-dir>")
    src, dst = Path(sys.argv[1]), Path(sys.argv[2])
    workspace = os.environ.get("GITHUB_WORKSPACE", str(Path.cwd()))

    docs = {}
    for path in sorted(src.rglob("*.sarif")):
        with path.open(encoding="utf-8") as fh:
            docs[path] = json.load(fh)
    if not docs:
        sys.exit(f"no SARIF files found under {src}")

    suppressed = set()
    for doc in docs.values():
        for run in doc.get("runs") or []:
            for result in run.get("results") or []:
                if is_suppressed(result):
                    suppressed.add(finding_key(result, workspace))
    print(f"{len(suppressed)} finding(s) suppressed by ASH:")
    for rule, uri, line in sorted(suppressed, key=str):
        print(f"  {rule} {uri}:{line}")

    for path, doc in docs.items():
        removed = 0
        for run in doc.get("runs") or []:
            results = run.get("results")
            if not results:
                continue
            kept = [
                r
                for r in results
                if not is_suppressed(r)
                and finding_key(r, workspace) not in suppressed
            ]
            removed += len(results) - len(kept)
            run["results"] = kept
        out = dst / path.relative_to(src)
        out.parent.mkdir(parents=True, exist_ok=True)
        with out.open("w", encoding="utf-8") as fh:
            json.dump(doc, fh)
        print(f"{path.relative_to(src)}: removed {removed} suppressed result(s)")


if __name__ == "__main__":
    main()
