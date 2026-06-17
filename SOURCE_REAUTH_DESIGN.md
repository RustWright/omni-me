# Design: Interactive source re-authentication (app-entered OTP)

> Status: **COMPLETE 2026-06-17 (task 3.5a fully shipped + live-verified, full stack).** The engine
> `AuthState` model, the `POST /auto_import/reauth` route, and the `ws-helper` `reauth` handler (server
> half, 2026-06-15) plus the **Dioxus client** — inline "Reconnect {source}" callout + OTP field in the
> Auto-Import Sources settings row — are all built. Route shape is `/auto_import/*` (not the `/sources/*`
> this doc originally sketched — it follows the live route prefix). **Real-OTP happy path proven against
> the real WS account:** a live TOTP code flipped `auth_state → active` and the next manual pull came back
> `last_outcome: success` / `health: healthy` (proving the session refreshed, not just the flag cleared).
> This **unblocks task 2.5** (WS auto-import may deploy to the VPS once Phase 2 lands). Motivating
> consumer: the WealthSimple auto-import source (private overlay).

## Problem

Auto-import runs **server-side** (the LLM key must live server-side; the WS subprocess + its
session file live wherever the server runs). The WealthSimple source authenticates with a
**session token that lasts ~weeks**, minted by a **one-time OTP login**. The OTP code itself is
short-lived (≈30 s TOTP / a couple minutes SMS).

Today the server runs **locally**, so re-priming the session is a local terminal command. The moment
the server moves to a **VPS**, that same command would require SSH-ing into the box to type a code —
unacceptable UX for a roughly-monthly chore. The user must be able to enter the OTP **in the app**,
never touching the server host.

## Goal

A source that has lost its credential can request **interactive re-auth**, and the user supplies the
one missing factor (an OTP) **from the Tauri client**. Email/password and the minted session **never
leave the server**; only the single-use code travels (over the existing client↔server channel).

## This capability is open-core

| Layer | Public engine (`omni-me`) | Private overlay (`omni-me-private`) |
|---|---|---|
| Source auth-state model (`Active` / `NeedsReauth` / `AwaitingOtp`) | ✅ generic | — |
| `GET /sources/status`, `POST /sources/{name}/reauth/{start,submit}` | ✅ generic | — |
| Client "Reconnect *{source}*" UI + OTP field | ✅ generic (label is data) | — |
| The actual login/OTP protocol (driver invocation, exit-5 semantics) | — | ✅ WS-specific |

The public engine learns one new generic idea: **a source may be re-auth-capable and report an
auth state**. It still names no banks. WS is merely the first source that implements the private
half.

## Server side

### Source auth state

Extend the `AutoImportSource` trait (or a sibling `ReauthableSource`) so a source can report and
drive re-auth. Sketch:

```rust
enum AuthState { Active, NeedsReauth { reason: String }, AwaitingOtp { expires_at: Instant } }

trait ReauthableSource {
    fn auth_state(&self) -> AuthState;
    async fn reauth_start(&self) -> ReauthStart;          // triggers the provider to send a code
    async fn reauth_submit(&self, otp: &str) -> Result<(), ReauthError>; // completes + persists
}
```

A normal 30-min tick that fails on an expired/invalid credential transitions the source to
`NeedsReauth` and **stops auto-retrying the login** (no lockout from repeated attempts). The other
sources are unaffected — this is the graceful-degradation behaviour we already have, just made
*visible* instead of buried in a log line.

### Endpoints (behind the existing Tailscale boundary; auth deferred per `project_auth_deferred`)

- `GET  /sources/status` → `[{ name, state, last_pull, message }]` — the client polls this (it
  already polls sync/status) to decide when to surface a "Reconnect" affordance.
- `POST /sources/{name}/reauth/start` → `{ status: "awaiting_otp" | "no_otp_needed" | "error", … }`
  — asks the source to begin a login, which causes the provider to **send** the code.
- `POST /sources/{name}/reauth/submit { otp }` → `{ status: "active" | "invalid_otp" | "error" }`
  — completes the login with the code, persists the session server-side, returns the source to
  `Active`.

## The driver-protocol question (the one real fork)

WS sends the code **in response to a login attempt**, so re-auth is inherently two-step
(trigger → receive → submit). Two shapes:

- **(a) Stateless two-call.** `start` runs a driver login(otp=null) purely to make WS send the
  code, then exits. `submit` runs a *fresh* driver login(email, password, otp=code). Fits the Step-2
  **helper model** cleanly (each call is a short-lived request/response subprocess — no held state).
  **Risk:** if WS sends a *new* SMS on the `submit` attempt and rejects the `start` code, this breaks
  for SMS 2FA (works fine for TOTP, where the code is authenticator-derived and login-attempt-independent).
- **(b) Long-lived interactive subprocess.** `start` spawns the driver, which logs in, catches
  "OTP required", prints `OTP_SENT`, then **blocks reading a second stdin line**; the server holds
  that child (with a TTL) and writes the code to it on `submit`. Robust to **both** TOTP and SMS
  (same login context completes), but the server must babysit a stateful child — friction against the
  clean stateless helper contract Step 2 is trying to freeze.

**Resolved (2026-06-14): this account uses TOTP (authenticator app).** That collapses the fork in
our favour. A TOTP code is generated client-side by the authenticator, continuously and
**independent of any login attempt** — WS sends nothing. So there is nothing to "trigger," and the
two-step flow reduces to a **single `reauth/submit { otp }`** that runs a fresh
`login(email, password, otp=code)`. Shape **(a)** in its simplest form; the long-lived subprocess
**(b)** and the SMS-resend risk are **moot for this account** (keep (b) noted only as the fallback if
a future source uses SMS/email 2FA).

Consequences:
- `POST /sources/{name}/reauth/start` is **optional** — only needed by a future provider-sent-code
  source (SMS/email). For the TOTP path the client goes straight to `submit`.
- The client UI is just: `NeedsReauth` → "Reconnect WealthSimple" → an OTP field ("enter your
  authenticator code") → `submit`. No "we sent you a code" interstitial.
- The driver's existing exit codes already separate `5` (OTP required → `NeedsReauth`) from `6`
  (transient error → retry), so the state-transition logic is unambiguous.

## Client side

**Built 2026-06-17 (inline-in-row).** The Auto-Import Sources section of Settings already lists each
source with a health badge and a `Fetch now` button (`AutoImportRow`, `frontend/src/pages/settings.rs`).
Re-auth is folded into that same row rather than a modal or banner: when a source reports
`auth_state.kind == "needs_reauth"` **and** `reauth_capable`, the row grows an amber "Reconnect needed"
callout carrying the `reason`. A `Reconnect` button expands an **inline** 6-digit OTP field (digit-
filtered on input); `Submit` calls the `reauth_source` Tauri command → `POST /auto_import/reauth` (OTP
in the JSON body, never the URL — it must not land in access logs). The `ReauthOutcome` JSON drives the
row: `active` → success toast, field collapses, parent re-pulls `GET /auto_import/status` so the row
returns to healthy; `invalid_otp` → "Authenticator code rejected — try again", field stays, input
clears; `not_supported`/`error` → the message inline. No `reauth/start` step (TOTP collapsed the fork).

The key seam to get right was **serde survival**: the server emits `auth_state`/`reauth_capable`, but
each hop's proxy struct (`AutoImportSourceView` in both the Tauri command layer and the frontend types)
deserializes lossily — an undeclared field is silently dropped. Every layer's struct had to declare the
new fields (`#[serde(default)]` so old mocks/servers stay parseable) for the signal to reach the screen.

This is **orthogonal to the health badge**: `health` is passive ("is data flowing — wait out a transient
blip"), `auth_state` is imperative ("the user must act"). A degraded-but-active source (e.g. an SC PDF
decrypt failure) shows *no* Reconnect callout; only `needs_reauth` does.

Re-auth runs on **two clocks**: the OTP code is short-lived (~30 s TOTP), the session it mints lasts
~weeks. `registry.reauth` only flips `auth_state` (it does *not* pull) — so `auth_state → active` proves
the OTP clock, and a separate successful pull (`last_outcome: success`) proves the session clock. The
real-OTP test exercised both: a live code, then a manual `Fetch now` that came back healthy.

## Security

- Email/password + the minted session stay **server-side** (`credentials.toml` + `session_path`).
- The client transmits only the **OTP** — single-use, short-lived, low value if intercepted.
- Endpoints inherit the Tailscale reachability boundary today; when real sync auth lands they sit
  behind it.

## Sequencing

- **Step 2 (subprocess-helper conversion)** is the natural home: the helper's JSON contract gains the
  `reauth start/submit` verbs at the same time it's being frozen.
- **Phase 2 (VPS deploy)** was **blocked on this** — going headless-on-VPS without it reintroduces the
  SSH-to-OTP problem. **Unblocked 2026-06-17:** app-entered re-auth has shipped, so WS auto-import may
  deploy to the VPS once the deploy pipeline (Phase 2) lands.
- The **local terminal prime** is no longer needed once deployed — the app reconnects WS itself.

## Open questions

1. ~~Is this WS account's 2FA TOTP or SMS?~~ **Resolved 2026-06-14: TOTP** → single-step `submit`,
   no `start`/trigger needed (see the driver-protocol section).
2. ~~Where does the source-health surface live in the client — Settings, or a persistent status chip?~~
   **Resolved 2026-06-17: inline in the existing Auto-Import Sources settings row** (no modal/banner).
3. Should `reauth` be rate-limited to avoid repeated bad-code login attempts (lockout risk)? **Still
   open** — low risk behind Tailscale; deferred (noted, not built). Revisit if the VPS ever opens up.
