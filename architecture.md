# Architecture

**Last Updated:** 2026-03-07
**Rationale & Alternatives:** See `research.md` for full exploration of each decision.

---

## Deployment

| Component | Choice | Details |
|-----------|--------|---------|
| App framework | Tauri v2 | Android APK, sideloaded (no Play Store) |
| UI framework | Dioxus 0.7 (Rust, WASM) | App chrome and all non-editor UI |
| Text editor | CodeMirror 6 (JS) | Embedded via Tauri IPC bridge, shared across all text features |
| Fallback | PWA | Browser access on any device via VPS-hosted web app |

**Offline:** Tauri app runs locally on device. Works offline by default. Syncs to VPS when connected.

> Research: `research.md` Section 1

---

## Infrastructure

| Component | Choice | Details |
|-----------|--------|---------|
| VPS | Hetzner CX22 (deferred) | ~€4.50/month. DigitalOcean rejected payment — skipping DO, going straight to Hetzner when features are stable |
| Local dev | Desktop server + Tailscale | Phone↔desktop sync via Tailscale mesh VPN during development. Same networking model as production |
| CI/CD | GitHub Actions | Build + test + artifacts. Deploy step added when VPS provisioned |
| Backend | Rust (Axum) | Runs locally during dev, VPS in production |

> Research: `research.md` Section 2

---

## Database

| Component | Choice | Details |
|-----------|--------|---------|
| Primary DB | SurrealDB | Multi-model (relational + document + graph), Rust-native, schemaless by default |
| Storage engine | `kv-surrealkv` (embedded) | Single-file embed on both device and VPS |
| Financial data | hledger plaintext journals | Separate from SurrealDB. Rust calls `hledger` CLI via subprocess |
| Vector search | SurrealDB native (v1.3+) | Embeddings stored alongside documents, no separate vector DB |

**Why not SQLite:** 13 features with unknown final data shapes. Graph relationships needed (People Tracker, Knowledge Compounder). Schema flexibility critical for an evolving app. SurrealDB handles all three data models in one engine.

> Research: `research.md` Section 4

---

## Sync Strategy: Event Sourcing

All state changes stored as **append-only immutable events**. Current state derived by replaying events into read models (projections).

```
event: { id, type, aggregate_id, timestamp, device_id, payload: {flexible JSON} }
```

**Sync flow:**
1. App opens → query VPS for events since `last_sync_timestamp`
2. Push local events VPS hasn't seen
3. Both sides replay new events → read models update

**Why event sourcing:** Events are facts — no conflicts, no last-write-wins, no migrations. Schema evolution = change how you interpret events, not the events themselves. Full history for free.

**Trade-off:** ~1 week infrastructure before first visible feature.

> Research: `research.md` Section 4

---

## LLM Integration

| Component | Choice | Details |
|-----------|--------|---------|
| Primary LLM | Gemini Flash (free API) | 1,500 req/min free, no expiration. REST API via `reqwest` |
| Abstraction | `trait LlmClient` | Swap/add providers via config. Gemini now, Claude API later |
| Output format | Structured output + tool calling | LLM constrained to defined JSON schemas and typed actions |
| Prompt management | Versioned | Every `*_llm_processed` event stores `{prompt_version, model}` |
| Confidence gate | Review before commit | Low-confidence derivations (especially financial) require user confirmation |

**Deterministic pre-processing first:** Extract URLs, parse dates, detect barcodes with code before LLM sees raw text.

> Research: `research.md` Section 3

---

## Data Model

**Single raw note type.** LLM derives all structure as separate events.

```
note_created       → {raw_text, date}
note_updated       → {note_id, raw_text}
note_llm_processed → {note_id, prompt_version, model, derived: {tags, insights, tasks, ...}}
```

Re-process any note with a better model/prompt at any time. Raw input is never modified.

---

## Feature Map

### Pure App Logic (no external services)

| Feature | Key Design Notes |
|---------|-----------------|
| **Note Taking / Journal** | CodeMirror editor. Single note type → LLM derivation |
| **Decisions** | Pros/cons entry, decision log, outcome tracking |
| **Focus** | Not standalone — UI mode within Task Manager (Pomodoro, focused view) |
| **Goal Setter** | Goals and projects are the same thing — variable-depth tree. Active limit (default 5) with friction to change |
| **Task Manager** | Leaf nodes in goal tree. Orphan tasks valid. Deferral requires two-part justification. No hard deadlines — urgency derived from parent goal |
| **Routine Manager** | Time budget model. Routine groups (templates) with frequency. Daily checklist auto-logs timestamps on tap |
| **People Tracker** | Note-based, LLM-enriched. SurrealDB graph relations for connections between people |

### External Service Dependencies

| Feature | Services | Free Tier |
|---------|----------|-----------|
| **Budget / Financial** | hledger CLI, Mindee OCR, Paisa (visualization) | Mindee: 250 pages/month |
| **Locations** | HERE Maps (reverse geocoding), Leaflet.js (display) | HERE: 250K req/month |
| **Scheduler** | Google Calendar API (OAuth) | Free (~1M queries/day) |
| **Meal Tracker** | USDA FoodData Central, Open Food Facts, Gemini vision | All free |
| **Knowledge Compounder** | Gemini text-embedding-004, SurrealDB vector search | Free |
| **Archive** | Gemini/Mindee OCR, SurrealDB semantic search, Android share sheet | See above |

> Research: `research.md` Section 5 for full event shapes and design details per feature

---

## Security

### VPS Hardening
- SSH key-only auth, password login disabled
- UFW firewall — expose only 443 (HTTPS) and SSH
- `unattended-upgrades` for automatic security patches
- SurrealDB and Paisa bound to localhost only
- Tailscale (free mesh VPN) for accessing services — nothing on public internet

### Data in Transit
- All phone ↔ VPS sync over HTTPS (TLS)
- All LLM API calls over HTTPS
- Optional: route sync through Tailscale so API endpoint isn't publicly reachable

### Data at Rest
- Android full-disk encryption (default on modern devices)
- No additional app-level encryption for MVP (device encryption sufficient for single user)

### LLM Data Exposure
- Gemini API terms: no training on API data
- Accepted risk for MVP — raw notes and financial data sent to Google
- Avoid sending most sensitive documents (passport scans, etc.) through LLM — OCR locally
- `LlmClient` trait enables future swap to self-hosted LLM

### Backups
- DigitalOcean weekly snapshots (~$2/month) OR scheduled `surrealdb export` + hledger files to separate encrypted location

---

## MVP Scope (Cycle 1)

**Target:** End of March 2026

**Week 1 — Infrastructure:**
- Tauri v2 Android APK build + sideload validation
- SurrealDB embedded (device) + server (VPS)
- Event store foundation + read model projections
- Basic sync (polling on app open)
- CI/CD: GitHub Actions → DigitalOcean
- Gemini LLM pipeline (structured output + tool calling)
- CodeMirror 6 editor component (Tauri IPC bridge)

**Weeks 2-3 — Features:**
1. Journal / Notes — proves capture → LLM derivation pipeline
2. Routine Manager — validates multi-device sync with daily use

**MVP validates:** APK sideloading, SurrealDB + event sourcing sync, CI/CD pipeline, LLM derivation

### Future Cycles
- **Cycle 2:** Task Manager, Goal Setter, Calendar, Budget, Locations
- **Cycle 3+:** Meal Tracker, People Tracker, Knowledge Compounder, Archive

---

## Known Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| Tauri v2 Android sideloading untested | High | POC first — validate before building on it. Fallback: PWA-only |
| SurrealDB embedded maturity | Medium | Early POC in week 1. Fallback: SQLite with JSON columns |
| Dioxus inside Tauri integration friction | Medium | POC with simple UI before committing to complex views |
| Event sourcing first-time implementation | Medium | Keep event shapes simple for MVP. Read model complexity grows incrementally |
| YouTube Data API quota (Knowledge Compounder) | Low | Cycle 3+ feature. Verify quota before implementing |
