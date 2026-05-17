#!/usr/bin/env python3
"""
WealthSimple driver script for omni-me's path-B auto-import (task 2.9).

omni-me invokes this script as a subprocess. The contract:

  Stdin (one line of JSON):
    {
      "email": "...",
      "password": "...",
      "otp": "123456" or null,        # 2FA code; omit on subsequent runs that use a saved session
      "session_path": "/abs/path/to/ws-session.json"  # persistent session file
    }

  Stdout (one line of JSON, on success):
    {
      "accounts": [
        {"id": "<ws-account-id>", "name": "<display>", "currency": "CAD"}
      ],
      "transactions": [
        {
          "external_id": "<stable WS-side id>",
          "account_id": "<ws-account-id>",
          "date": "YYYY-MM-DD",
          "description": "<merchant or description>",
          "amount": "<signed decimal STRING — negative = outflow>",
          "commodity": "CAD"
        }
      ]
    }

  Exit codes:
    0  success — accounts + transactions emitted on stdout
    2  malformed stdin JSON
    3  ws-api library missing (run: pip install ws-api)
    4  login failed (wrong password, network error, etc.)
    5  OTP REQUIRED — re-run with the 6-digit code in `otp`
    6  WS API call failed mid-run (transient — retry with backoff)

# Setup

    pip install --user ws-api

# Session persistence

The driver saves the WS session to `session_path` after each successful
login. Subsequent runs load that file via `from_token`, avoiding the OTP
prompt as long as the session is still valid (~weeks). When the session
expires, the driver falls back to `login()` and re-raises OTPRequired so
the user can supply a fresh code.

# Field-name notes (vs. the omni-me wire contract)

ws-api 0.33 returns activities with these field names; the driver maps them:
  - `canonical_id`     → external_id
  - `account_id`       → account_id (as-is)
  - `occurred_at`      → date (slice to YYYY-MM-DD)
  - `description`      → description
  - `amount.amount`    → amount (already a string in ws-api)
  - `amount.currency`  → commodity
"""

import json
import sys
from pathlib import Path


def main() -> int:
    # 1. Read stdin.
    creds_line = sys.stdin.readline()
    try:
        creds = json.loads(creds_line)
    except json.JSONDecodeError as e:
        print(f"failed to parse stdin JSON: {e}", file=sys.stderr)
        return 2

    email = creds.get("email")
    password = creds.get("password")
    otp = creds.get("otp")
    session_path = creds.get("session_path")
    if not email or not password or not session_path:
        print("stdin JSON missing email, password, or session_path", file=sys.stderr)
        return 2

    # 2. Import ws-api.
    try:
        from ws_api import (
            WealthsimpleAPI,
            OTPRequiredException,
            ManualLoginRequired,
            LoginFailedException,
            WSAPISession,
        )
    except ImportError as e:
        print(f"ws-api not installed (pip install ws-api): {e}", file=sys.stderr)
        return 3

    session_file = Path(session_path)

    def save_session(sess_json) -> None:
        # ws-api calls this with `self.session.to_json()` already applied —
        # we just write what we receive (str or dict, depending on version).
        session_file.parent.mkdir(parents=True, exist_ok=True)
        if isinstance(sess_json, str):
            session_file.write_text(sess_json)
        else:
            session_file.write_text(json.dumps(sess_json))
        try:
            session_file.chmod(0o600)
        except OSError:
            pass

    # 3. Build a client, preferring a saved session.
    client = None
    if session_file.exists():
        try:
            # WSAPISession.from_json expects the raw JSON string — don't pre-parse.
            sess = WSAPISession.from_json(session_file.read_text())
            client = WealthsimpleAPI.from_token(sess, persist_session_fct=save_session, username=email)
        except (ManualLoginRequired, Exception) as e:
            # Stale or unparseable session — fall through to fresh login.
            print(f"saved session unusable, re-logging in: {e}", file=sys.stderr)
            client = None

    if client is None:
        try:
            sess = WealthsimpleAPI.login(email, password, otp_answer=otp, persist_session_fct=save_session)
            client = WealthsimpleAPI.from_token(sess, persist_session_fct=save_session, username=email)
        except OTPRequiredException:
            print("OTP required — re-run with `otp` populated", file=sys.stderr)
            return 5
        except LoginFailedException as e:
            print(f"login failed: {e}", file=sys.stderr)
            return 4

    # 4. Pull accounts + activities.
    try:
        accounts_raw = client.get_accounts(open_only=True)
        activities_raw = []
        for acct in accounts_raw:
            for act in client.get_activities(acct["id"]):
                activities_raw.append((acct["id"], act))
    except Exception as e:
        print(f"WS API call failed: {e}", file=sys.stderr)
        return 6

    # 5. Map to omni-me wire shape.
    accounts = []
    for a in accounts_raw:
        accounts.append(
            {
                "id": a.get("id", ""),
                "name": a.get("description") or a.get("nickname") or a.get("type") or a.get("id", ""),
                "currency": a.get("base_currency") or a.get("currency") or "CAD",
            }
        )

    transactions = []
    for account_id, t in activities_raw:
        # ws-api 0.33 returns activities as flat camelCase dicts. Verified
        # 2026-05-16 against a real account with 7 sub-accounts.
        external_id = t.get("canonicalId") or t.get("externalCanonicalId") or ""
        date_str = (t.get("occurredAt") or "")[:10]
        description = (
            t.get("description")
            or t.get("spendMerchant")
            or t.get("counterPartyName")
            or t.get("type")
            or ""
        )
        raw_amount = t.get("amount")
        commodity = t.get("currency") or "CAD"
        # `amount` is an unsigned string; `amountSign` is "credit" / "debit".
        # Outflows ("debit") get a negative sign in our hledger model.
        sign = t.get("amountSign", "").lower()
        if not external_id or not date_str or raw_amount is None:
            print(f"skipping activity with missing fields: keys={list(t.keys())}", file=sys.stderr)
            continue
        signed = str(raw_amount)
        if sign == "debit" and not signed.startswith("-"):
            signed = "-" + signed

        transactions.append(
            {
                "external_id": str(external_id),
                "account_id": account_id,
                "date": date_str,
                "description": description,
                "amount": signed,
                "commodity": commodity,
            }
        )

    json.dump({"accounts": accounts, "transactions": transactions}, sys.stdout)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
