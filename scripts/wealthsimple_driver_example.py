#!/usr/bin/env python3
"""
Example WealthSimple driver script for omni-me's path-B auto-import (task 2.9).

omni-me invokes this script as a subprocess. The contract:

  1. Read ONE line of JSON from stdin: `{"email": "...", "password": "..."}`
  2. Log into WealthSimple via the `ws-api` library
     (see https://github.com/gboudreau/ws-api-python)
  3. Pull accounts + transactions
  4. Emit ONE line of JSON to stdout matching the WsImportEnvelope shape:

     {
       "accounts": [
         { "id": "<ws-account-id>", "name": "<display>", "currency": "CAD" },
         ...
       ],
       "transactions": [
         {
           "external_id": "<stable WS-side txn id>",
           "account_id": "<ws-account-id matching the accounts list>",
           "date": "YYYY-MM-DD",
           "description": "<merchant or description>",
           "amount": "<signed decimal STRING — negative = outflow>",
           "commodity": "CAD"
         },
         ...
       ]
     }

  5. Exit 0 on success, non-zero on failure (stderr is captured into omni-me's logs).

The "amount" field MUST be a JSON string (not a number) — rust_decimal won't
deserialize from a JSON number on the Rust side. See `core/src/events/types.rs`.

# Setup

    pip install --user ws-api

(Or in a venv — point `credentials.toml -> wealthsimple_python.python_path` at
the venv's python executable.)

# Install location

Put this script wherever you like (e.g. `~/.local/share/omni-me/drivers/ws.py`)
and reference its absolute path in your omni-me configuration. omni-me passes
that path to `python_path` as the script argument.

# Updating

If ws-api breaks on a WealthSimple regex change, `pip install --upgrade ws-api`
usually picks up the community fix within hours. This driver script doesn't
need to change unless the library's Python API surface itself changes.
"""

import json
import sys


def main() -> int:
    # 1. Read credentials from stdin.
    creds_line = sys.stdin.readline()
    try:
        creds = json.loads(creds_line)
    except json.JSONDecodeError as e:
        print(f"failed to parse stdin JSON: {e}", file=sys.stderr)
        return 2

    email = creds.get("email")
    password = creds.get("password")
    if not email or not password:
        print("stdin JSON missing email or password", file=sys.stderr)
        return 2

    # 2. Log in + pull data. Adjust the imports + calls below to match
    #    whatever the current ws-api version exposes.
    try:
        from ws_api import WealthsimpleAPI  # type: ignore
    except ImportError as e:
        print(f"ws-api not installed (pip install ws-api): {e}", file=sys.stderr)
        return 3

    try:
        client = WealthsimpleAPI.login(email, password)
        accounts_raw = client.get_accounts()
        # ws-api's transaction API varies by account type — adapt as needed.
        transactions_raw = []
        for acct in accounts_raw:
            for txn in client.get_account_activities(acct["id"]):
                transactions_raw.append((acct["id"], txn))
    except Exception as e:
        print(f"WS login or fetch failed: {e}", file=sys.stderr)
        return 4

    # 3. Map ws-api shapes → omni-me's WsImportEnvelope JSON.
    accounts = [
        {
            "id": a["id"],
            "name": a.get("name") or a.get("description") or a["id"],
            "currency": a.get("currency", "CAD"),
        }
        for a in accounts_raw
    ]
    transactions = []
    for account_id, t in transactions_raw:
        # Adjust field names below to match ws-api's actual response shape.
        amount = t.get("amount") or t.get("net_amount") or "0"
        transactions.append(
            {
                "external_id": str(t["id"]),
                "account_id": account_id,
                "date": t["date"][:10],  # YYYY-MM-DD slice
                "description": t.get("description", ""),
                "amount": str(amount),  # MUST be a string
                "commodity": t.get("currency", "CAD"),
            }
        )

    # 4. Emit single-line JSON envelope.
    json.dump({"accounts": accounts, "transactions": transactions}, sys.stdout)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
