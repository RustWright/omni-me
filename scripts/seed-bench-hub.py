#!/usr/bin/env python3
"""Seed a throwaway hub with the corpus `omni-me-agent --bench` scores against.

The bench asks about rent, a dentist appointment, a journal entry on 2026-03-14,
a grocery list and "what routines do I have?". Against an empty log every one of
those is unanswerable, so a run on a bare hub measures nothing — the model
searches, finds nothing, and burns its turn budget looking.

⚠️ **This file and `agent/src/bench.rs` are edited together.** The cases name the
things seeded here; changing one without the other silently turns a case into a
test of what the model does when the answer does not exist.

⚠️ **Fictional content only**, per the standing rule that no real identity or
financial data reaches the public repo. That is not a formality here: the corpus
gets sent verbatim to whatever endpoint is being benchmarked.

Why `/sync/push` rather than the app: it validates payloads but applies **no
feature guard**, so a seeded event stands in for one arriving from another
device, and no client has to be running. The hub itself needs no credentials —
`server`'s DB_PATH is relative, so the working directory is the isolation
boundary:

    mkdir hub && cd hub && cargo run -p omni-me-server
    python3 scripts/seed-bench-hub.py

Teardown is `rm -rf hub`.
"""

import argparse
import json
import sys
import urllib.error
import urllib.request

# One entry per calendar day; `aggregate_id`, `journal_id` and `date` are all the
# date, which is what lets `read{type:"journal", id:"2026-03-14"}` work without a
# search first — journal identity is `IdentityKind::Date`.
JOURNALS = [
    ("2026-03-14", "Landlord posted the rent notice today. Going up 40 a month from June. "
                   "Spent the evening reading about tenancy boards. Slept badly."),
    ("2026-03-15", "Long walk by the canal. Started the sourdough starter again, third attempt. "
                   "Called Mum."),
    ("2026-03-16", "Dentist appointment at 10. No cavities. Worked on the migration script "
                   "most of the afternoon, finally got the batch import to stop timing out."),
    ("2026-03-17", "Slow day. Read most of the afternoon. Cycling club in the evening, "
                   "did about 30km."),
    ("2026-03-18", "Back to the sourdough. It rose properly this time. Wrote up notes on the "
                   "import work so I stop rediscovering the same edge cases."),
]

NOTES = [
    ("Grocery list", "Milk, eggs, flour for the starter, coffee beans, olive oil."),
    ("Tenancy board notes", "Rent increases need 90 days notice in writing. Above guideline "
                            "increases need an application. Keep the posted notice."),
    ("Sourdough log", "Attempt 3: 100g flour 100g water, fed twice daily at 24C. Doubled "
                      "after 6 days. Attempts 1 and 2 died, probably too cold."),
    ("Bike maintenance", "Chain needs degreasing every 300km or so. Rear brake pads are "
                         "getting thin, replace before the spring rides."),
]

# Deliberately named "Morning" and "Evening winddown" — neither contains the word
# "routine". That is the whole point of the `list` verb and of the bench case
# that expects it: a record is not guaranteed to carry the name of its own type,
# so text search cannot answer "what routines do I have?".
ROUTINES = [
    ("Morning", "daily", ["stretch", "coffee", "journal"]),
    ("Evening winddown", "daily", ["tidy kitchen", "read", "lights out by 11"]),
]


def ulid(n: int, prefix: str) -> str:
    """A 26-char ULID-shaped id. Not a real ULID — stable and readable beats
    sortable here, because a reseed must land on the same ids or a second push
    duplicates the corpus instead of converging on it (the projections UPSERT)."""
    return f"01JK{prefix}{n:0{26 - 4 - len(prefix)}d}"


def build_events(device_id: str) -> list[dict]:
    events: list[dict] = []

    for date, text in JOURNALS:
        events.append({
            "event_type": "journal_entry_created",
            "aggregate_id": date,
            "timestamp": f"{date}T09:00:00Z",
            "device_id": device_id,
            "payload": {"journal_id": date, "date": date, "raw_text": text},
        })

    for i, (title, body) in enumerate(NOTES):
        nid = ulid(i, "NOTE")
        events.append({
            "event_type": "generic_note_created",
            "aggregate_id": nid,
            "timestamp": f"2026-03-1{i}T12:00:00Z",
            "device_id": device_id,
            "payload": {"note_id": nid, "title": title, "raw_text": body},
        })

    for i, (name, freq, items) in enumerate(ROUTINES):
        gid = ulid(i, "GRP")
        # The group's identity is the aggregate_id, NOT a payload field — the
        # projection reads `event.aggregate_id` for it and `order` (not
        # `order_num`) from the payload. Getting either wrong pushes cleanly and
        # projects into an empty row.
        events.append({
            "event_type": "routine_group_created",
            "aggregate_id": gid,
            "timestamp": f"2026-03-0{i + 1}T08:00:00Z",
            "device_id": device_id,
            "payload": {"name": name, "frequency": freq, "order": i},
        })
        for j, item in enumerate(items):
            iid = ulid(i * 10 + j, "ITM")
            events.append({
                "event_type": "routine_item_added",
                "aggregate_id": iid,
                "timestamp": f"2026-03-0{i + 1}T08:0{j + 1}:00Z",
                "device_id": device_id,
                "payload": {
                    "group_id": gid,
                    "name": item,
                    "estimated_duration_min": 5,
                    "order": j,
                },
            })

    return events


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--url", default="http://localhost:3000",
                    help="hub base URL (default: %(default)s)")
    ap.add_argument("--device-id", default="seed-device",
                    help="author device id; must NOT be the agent's own, or the "
                         "agent's pull will filter these out (default: %(default)s)")
    args = ap.parse_args()

    events = build_events(args.device_id)
    body = json.dumps({"device_id": args.device_id, "events": events}).encode()
    req = urllib.request.Request(
        f"{args.url.rstrip('/')}/sync/push",
        data=body,
        headers={"Content-Type": "application/json"},
    )
    try:
        reply = urllib.request.urlopen(req, timeout=30).read().decode()
    except urllib.error.HTTPError as e:
        print(f"HTTP {e.code}: {e.read().decode()[:500]}", file=sys.stderr)
        return 1
    except urllib.error.URLError as e:
        print(f"no hub at {args.url}: {e.reason}", file=sys.stderr)
        return 1

    print(f"pushed {len(events)} events: {reply}")
    print(f"  {len(JOURNALS)} journal entries, {len(NOTES)} notes, "
          f"{len(ROUTINES)} routine groups")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
