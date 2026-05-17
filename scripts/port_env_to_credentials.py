#!/usr/bin/env python3
"""
One-shot port of `.env` keys → `~/.config/omni-me/credentials.toml`.

Reads .env in the omni-me repo root (literally, no shell interpolation), maps
known keys to their credential homes, and writes a TOML file with chmod 600.

Idempotent: re-running overwrites the previous file. Safe to run after rotating
any secret in .env.
"""

import os
import stat
import sys
from pathlib import Path


def parse_env(path: Path) -> dict[str, str]:
    out: dict[str, str] = {}
    with path.open() as f:
        for line in f:
            line = line.rstrip("\n")
            if not line or line.startswith("#") or "=" not in line:
                continue
            k, v = line.split("=", 1)
            if (v.startswith('"') and v.endswith('"')) or (v.startswith("'") and v.endswith("'")):
                v = v[1:-1]
            out[k.strip()] = v
    return out


def toml_quote(s: str) -> str:
    # TOML basic string — escape backslash + double quote.
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def main() -> int:
    here = Path(__file__).resolve().parent.parent
    env_path = here / ".env"
    if not env_path.exists():
        print(f".env not found at {env_path}", file=sys.stderr)
        return 1
    env = parse_env(env_path)

    xdg = os.environ.get("XDG_CONFIG_HOME") or str(Path.home() / ".config")
    out_path = Path(xdg) / "omni-me" / "credentials.toml"
    out_path.parent.mkdir(parents=True, exist_ok=True)

    sections: list[str] = []

    if env.get("WISE_API_KEY"):
        sections.append("[wise]")
        sections.append(f"api_token = {toml_quote(env['WISE_API_KEY'])}")
        sections.append("")

    if env.get("GEMINI_API_KEY"):
        sections.append("[gemini]")
        sections.append(f"api_key = {toml_quote(env['GEMINI_API_KEY'])}")
        sections.append("")

    ws_user = env.get("WS_EMAIL")
    ws_pass = env.get("WS_PASSWORD")
    if ws_user and ws_pass:
        # Driver script lives in the repo for now; user can copy to a stable
        # path (e.g. ~/.local/share/omni-me/drivers/) and update if desired.
        driver = str(here / "scripts" / "wealthsimple_driver_example.py")
        sections.append("[wealthsimple_python]")
        sections.append(f"email = {toml_quote(ws_user)}")
        sections.append(f"password = {toml_quote(ws_pass)}")
        sections.append('python_path = "python3"')
        sections.append(f"driver_script = {toml_quote(driver)}")
        sections.append("")

    # SC accounts — derive hledger account name from the env-var suffix.
    for currency in ("USD", "NGN"):
        key = f"SC_{currency}_ACCNT_NO"
        if env.get(key):
            sections.append("[[sc_accounts]]")
            sections.append(f"account_number = {toml_quote(env[key])}")
            sections.append(
                f'hledger_account = "Assets:StandardChartered:{currency}"'
            )
            sections.append(f'commodity = "{currency}"')
            sections.append("")

    # IMAP accounts — three known providers.
    for slot, host, user_key, pw_key in [
        ("gmail_personal", "imap.gmail.com", "GMAIL_PERSONAL_USER", "GMAIL_PERSONAL_PASSWORD"),
        ("gmail_work", "imap.gmail.com", "GMAIL_WORK_USER", "GMAIL_WORK_PASSWORD"),
        ("yahoo", "imap.mail.yahoo.com", "YAHOO_USER", "YAHOO_PASSWORD"),
    ]:
        if env.get(user_key) and env.get(pw_key):
            sections.append(f"[imap.{slot}]")
            sections.append(f"host = {toml_quote(host)}")
            sections.append("port = 993")
            sections.append(f"account = {toml_quote(env[user_key])}")
            sections.append(f"app_password = {toml_quote(env[pw_key])}")
            sections.append('watched_label = "INBOX"')
            sections.append("")

    contents = "\n".join(sections) + "\n" if sections else ""
    out_path.write_text(contents)
    out_path.chmod(stat.S_IRUSR | stat.S_IWUSR)  # 0600

    print(f"Wrote {len(sections)} TOML lines to {out_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
