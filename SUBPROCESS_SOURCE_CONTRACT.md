# Contract: subprocess auto-import sources (engine ↔ helper)

> Status: **frozen 2026-06-15.** Both verbs are implemented server-side (`pull` since the freeze;
> `reauth` server half added 2026-06-15 — see `SOURCE_REAUTH_DESIGN.md` for the auth-state model the
> client "Reconnect" UI consumes, which lands next). This is a freeze-once interface: the deploy image
> and any third-party data-source plugin key off it, so it changes only with a deliberate version bump.

## What this is

The public engine (`omni-me-server`) runs auto-import sources on a schedule. A **subprocess source** is
a source whose work is performed by a **separate executable** — a *helper* — that the engine spawns,
talks to over stdin/stdout with a one-line JSON request/response, and waits on. The engine knows nothing
about the helper's upstream (which bank, which API, which file format); it only knows this contract.

This is how the open-core split keeps bank-specific code out of the public engine at the **artifact**
level, and the mechanism by which anyone can add a data source without modifying the engine: write a
program that speaks this contract, point a `SubprocessSource` at it.

```
                       {"verb":"pull"}                  ┌────────────────┐
  ┌──────────────┐ ──────────────────────────────────▶ │   helper       │
  │ engine       │      (one JSON line on stdin)        │  (private /    │
  │ Subprocess-  │                                       │   third-party) │
  │ Source       │ ◀────────────────────────────────── │  owns its own  │
  └──────────────┘   {"status":"ok","drafts":[…]}        │  credentials   │
        │             (one JSON line on stdout)          └────────────────┘
        ▼
  wrap drafts → AutoImportBatchProposed event → append → project → review screen
```

## Boundary principle — the helper owns its credentials

The engine's request carries **no secrets**. The helper reads its own credentials itself (today: from
`credentials.toml`; a helper is free to use env vars, a keyring, whatever). Consequences:

- The public engine has **no code path** by which a bank credential could reach it — the open-core
  boundary is structural, not a convention to police.
- Interactive re-auth (the `reauth` verb) is a pure **pass-through**: the engine relays only the
  single-use OTP it received from the client; the email/password and the minted session never leave the
  helper's side.

## Engine → helper request

One line of JSON on the helper's **stdin**, followed by EOF (the engine closes the pipe). Tagged by
`verb`:

```json
{"verb":"pull"}
{"verb":"reauth","otp":"123456"}
```

- **`pull`** — fetch whatever is new and return drafts. The normal scheduled tick. *(implemented)*
- **`reauth`** — complete an interactive re-authentication using the supplied one-time `otp`, persist the
  refreshed credential, and report `reauth_ok` / `invalid_otp` / `error`. *(implemented)*

A helper that only ever does `pull` may treat any other verb as `error`.

## Helper → engine response

One line of JSON on the helper's **stdout**:

```json
{
  "status": "ok",
  "drafts": [ /* DraftTransaction objects — see below */ ],
  "dedup_key": "globepay-watermark-8841",   // optional
  "source_metadata": { "...": "..." },   // optional, opaque
  "disposition": { /* REQUIRED when status = "ok" — see below */ },
  "message": "human-readable detail"     // optional; required when status = "error"
}
```

### `disposition` — the row accounting (REQUIRED on `ok`)

A helper must account for **every row it fetched**. This is the one field the engine reads and
validates rather than passing through.

```json
"disposition": {
  "fetched": 295,                       // upstream rows, BEFORE any filtering
  "deduped": 0,                         // already present upstream-side
  "out_of_window": 145,                 // below the helper's import_since floor
  "unmapped_ids": ["acct-a", "acct-a"], // ONE ENTRY PER DROPPED ROW, repeats included
  "failures": [{ "id": "row-9", "reason": "no amount" }]
}
```

The engine checks this identity and **fails the tick** if it does not hold:

```
fetched == drafts.length + deduped + out_of_window + unmapped_ids.length + failures.length
```

Two rules follow from it, both deliberately unforgiving:

- **Omitting `disposition` on an `ok` response is an error.** It used to be legal, and
  `{"status":"ok"}` meant "nothing happened" — indistinguishable from "I discarded everything
  I fetched". A helper that will not say what it did with its rows is not reporting success.
- **`fetched > 0` with every row unmapped is an error** (`NothingMapped`), raised by the engine
  with the offending ids in the message. A *partial* mapping gap is not fatal — real rows still
  flow — but it forces the source's health to `dropping` rather than `healthy`.

`unmapped_ids` carries one entry per dropped **row**, not per distinct account: collapsing to a
distinct set reports 6 unmapped rows for 295 dropped ones and breaks the arithmetic. The engine
de-duplicates for display.

Put identifying keys here, not in `source_metadata` — the engine stores that blob but never reads
it, so anything a user needs to *act* on (the ids to add to an account map) is invisible there.

### `status` values

| status         | meaning                                                              | engine reaction |
|----------------|----------------------------------------------------------------------|-----------------|
| `ok`           | success; `drafts` may be empty (no new data is **not** a failure) — but `disposition` is **required** | validate the row accounting, then wrap + append + project the drafts; record a successful tick |
| `needs_reauth` | the stored credential is expired/invalid; the helper did **not** loop on login | degrade this source (surface as needing re-auth); do **not** hammer login. Other sources unaffected |
| `reauth_ok`    | `reauth` succeeded; credential refreshed and persisted               | return the source to `Active`; the client clears the Reconnect prompt |
| `invalid_otp`  | `reauth` ran but the code was wrong                                  | tell the client the code was rejected; the source stays `NeedsReauth` |
| `error`        | anything unexpected; `message` carries detail                        | treat as a transient failure → exponential backoff |

### Exit code

The helper **exits `0` whenever it produced a valid JSON response — including `needs_reauth`**, which is
a *handled* outcome, not a crash. A **non-zero** exit means the helper crashed or never emitted parseable
JSON; the engine treats that as a transient error and backs off. This keeps structured outcomes in the
`status` field rather than overloading numeric exit codes.

> A helper may wrap an inner tool with its own exit-code scheme (the Northwind helper wraps a Python
> driver whose codes `2`–`6` distinguish malformed-input / missing-library / login-failed / OTP-required
> / transient). Those inner codes are an implementation detail **below** this contract — the helper
> translates them into a `status` and exits `0`. The same inner code can read differently per verb: a
> `pull` reads exit `5` (no session) as `needs_reauth`, while a `reauth` reads exit `4` (login rejected)
> as `invalid_otp`.

## `drafts` — the helper builds them fully

Each element is a `DraftTransaction` (the same type the review screen already stores as JSON in the
`pending_auto_import_batches` projection):

```json
{
  "external_id": "northwind-txn-abc-123",
  "date": "2026-06-15",
  "description": "Loblaws",
  "postings": [
    {"account":"Assets:Northwind:Cash","commodity":"CAD","amount":"-87.42","fx_rate":null,"tags":[]},
    {"account":"Unmatched","commodity":"CAD","amount":"87.42","fx_rate":null,"tags":[]}
  ]
}
```

The **helper** owns all upstream-specific decisions: account mapping (upstream id / currency / CSV column
→ hledger account) and whether to add an `Unmatched` mirror posting. The engine never reasons about
banks or balancing — it wraps the drafts verbatim. (A polling source typically mirrors to `Unmatched`;
a source that emits already-balanced transfers between known accounts may not. That choice is the
helper's, and the engine preserves it.) `amount` is a decimal **string**; `external_id` must be stable
across runs so re-pulling the same upstream row doesn't double-record.

## `dedup_key` — idempotency token

The per-tick idempotency key for the whole batch. **Optional**: if the helper omits it, the engine
generates `"{source-name}-{unix_millis}"` (fine for a polling source that re-proposes a fresh batch each
tick; row-level dedup still happens via each draft's stable `external_id`). A watermark-style source
(e.g. "everything after transfer-id N") should supply its own key so an unchanged upstream produces an
identical key the engine can skip.

## `source_metadata` — opaque context

Free-form JSON the review screen can render for the user (statement window dates, sender/subject for an
emailed source, etc.). The engine stores it but never interprets it.

## What is NOT a subprocess source (scope boundary)

Email-handler sources (Meridian statements, receipts) attach to the engine's **generic IMAP
source** and depend on the **server-side document extractor** (the LLM key stays engine-side). They are a
*different* extension point — an already-fetched email handed to a handler — not a self-contained
"go fetch from upstream" pull. They are intentionally **out of scope** for this contract and remain
in-process for now; folding them in (likely: helper does only the bank-specific PDF decrypt, engine runs
the extraction) is a separate future design.

## Versioning

No version field today (single producer + consumer, pre-daily-use). The first **breaking** change adds a
`"v"` field to the request and a minimum-version check in `SubprocessSource`. Additive changes
(new optional response fields, new `status` values a helper opts into) do not bump the version — consumers
ignore unknown fields and treat unknown statuses as `error`.

**Required `disposition` is a breaking change**, taken deliberately without a version field because the
only producer is in-tree and the failure it prevents is silent data loss. A pre-existing helper now fails
its tick with a message naming the missing field — which is the intended outcome: a helper that cannot
account for its rows should stop, not keep reporting clean zeroes.

## See also

- `SOURCE_REAUTH_DESIGN.md` — the auth-state model + client "Reconnect {source}" UI that consume the
  `reauth` verb.
- `core/src/auto_import/subprocess.rs` — the engine side (`SubprocessSource` + the `HelperRequest` /
  `HelperResponse` / `HelperStatus` types that are this contract in code).
