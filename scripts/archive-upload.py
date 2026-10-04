#!/usr/bin/env python3
"""Upload a folder of documents into an omni-me server's archive.

    scripts/archive-upload.py <folder> --server http://host:3001 --instance dev [--limit N] [--dry-run]

Each file goes through `POST /documents/archive?source=bulk`, which runs no model,
so uploading costs nothing; reading happens later in the archive's own passes. A
bulk upload of bytes already archived files nothing new, so an interrupted run is
safe to repeat.

Refuses unless `--instance` matches the name the server declares on `/health`,
the same gate the wipe route uses, so a backfill meant for dev cannot reach live.
A bearer token, when the server needs one, is read from OMNI_AUTH_TOKEN.

Walks in sorted order and uploads only document types (`--only`, PDFs, CSVs and
images by default): the finance corpus keeps generated `.ledger` files and its
importer's own scripts beside the documents, and none of those are documents.
One unreadable file never ends the run; the report counts every outcome.
"""
import argparse
import json
import mimetypes
import os
import sys
import urllib.error
import urllib.request
from collections import Counter
from pathlib import Path


def request(method, url, body=None, headers=None):
    headers = dict(headers or {})
    token = os.environ.get("OMNI_AUTH_TOKEN")
    if token:
        headers["Authorization"] = f"Bearer {token}"
    req = urllib.request.Request(url, data=body, method=method, headers=headers)
    with urllib.request.urlopen(req, timeout=120) as resp:
        return json.loads(resp.read() or b"null")


def walk(root, only):
    files, unreadable = [], []
    stack = [root]
    while stack:
        folder = stack.pop()
        try:
            entries = sorted(folder.iterdir())
        except OSError as e:
            unreadable.append((str(folder), str(e)))
            continue
        for entry in entries:
            if entry.is_dir():
                stack.append(entry)
            elif entry.suffix.lower() in only:
                files.append(entry)
    return sorted(files), unreadable


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("folder", type=Path)
    ap.add_argument("--server", required=True)
    ap.add_argument("--instance", required=True, help="must equal what the server's /health declares")
    ap.add_argument("--only", default=".pdf,.csv,.jpg,.jpeg,.png,.heic,.webp",
                    help="comma-separated extensions to upload")
    ap.add_argument("--limit", type=int, help="upload only the first N files, for a trial")
    ap.add_argument("--dry-run", action="store_true", help="list what would go, upload nothing")
    args = ap.parse_args()

    server = args.server.rstrip("/")
    declared = request("GET", f"{server}/health").get("instance")
    if declared != args.instance:
        sys.exit(f"refused: --instance {args.instance!r} but {server} declares {declared!r}")

    only = {s.strip().lower() for s in args.only.split(",") if s.strip()}
    files, unreadable = walk(args.folder, only)
    for folder, why in unreadable:
        print(f"unreadable folder {folder}: {why}", file=sys.stderr)
    if args.limit is not None:
        files = files[: args.limit]
    print(f"{len(files)} files under {args.folder} for {declared}", file=sys.stderr)
    if args.dry_run:
        for f in files:
            print(f)
        return 0

    outcome, text = Counter(), Counter()
    failed = []
    for i, path in enumerate(files, 1):
        mime = mimetypes.guess_type(path.name)[0] or "application/octet-stream"
        try:
            body = path.read_bytes()
            resp = request(
                "POST",
                f"{server}/documents/archive?source=bulk",
                body,
                # Raw, as the app sends it; a header cannot carry non-ASCII.
                {"Content-Type": mime,
                 "X-Filename": path.name.encode("ascii", "replace").decode().replace("?", "_")},
            )
        except (OSError, urllib.error.URLError, ValueError) as e:
            failed.append((str(path), str(e)))
            outcome["failed"] += 1
            continue
        if resp.get("already_archived"):
            outcome["already archived"] += 1
        else:
            outcome["archived"] += 1
            text[resp.get("text_source", "?")] += 1
        if i % 25 == 0:
            print(f"  {i}/{len(files)}", file=sys.stderr)

    print(json.dumps({"seen": len(files), **outcome, "text_source": text,
                      "unreadable_folders": len(unreadable)}, indent=2))
    for path, why in failed:
        print(f"failed {path}: {why}", file=sys.stderr)
    return 1 if failed or unreadable else 0


if __name__ == "__main__":
    sys.exit(main())
