# omni-me: Architecture Research

**Date:** 2026-03-02
**Purpose:** Slow, thorough research session before making architectural decisions.
**Context:** Android phone (primary device), budget ~$30 CAD/month, fine with big tech APIs, Rust preference, modular design required.
**Workflow preference:** CI/CD pipeline (git push → auto-build → auto-deploy) is a high priority. Same workflow as personal website. No manual deploy steps.

---

## How to Use This Document

Each section covers one architectural area. For each decision:
- Read the options
- Mark your choice with `[CHOSEN]`
- Notes are space for your reasoning

We'll go through this together section by section. Nothing is decided yet.

---

## Section 1: Deployment Architecture

> **The question:** How does this app run on your Android phone?

This is the most fundamental decision — it shapes everything else.

### Option A: PWA (Progressive Web App)

**What it is:** Deploy a web app to your VPS. Access it via Chrome on Android. Install it to your home screen as a PWA — it behaves like an app (fullscreen, offline capable, home screen icon).

**Rust usage:** High. Build with Dioxus or Leptos (compiled to WASM). Rust runs in the browser and on the VPS server.

**Offline support:**
- Chrome on Android supports service workers and IndexedDB
- Can cache the UI and queue writes when offline — syncs when back online
- Limitation: background sync is limited on Android (can't sync while app is closed in most cases, needs user to open app)

**Android experience:** Good but not native. No APK — it's just a website installed as an app. Chrome handles everything. Can't access some low-level Android APIs (camera is fine, GPS is fine, contacts need special permissions).

**Time to MVP:** Fastest. No build pipeline for APK. Deploy to VPS, open browser, done.

**Pros:** Zero app store friction, deploy updates instantly, simpler architecture
**Cons:** Background sync limited, slightly "webby" feel, dependent on browser behaviour

---

### Option B: Tauri v2 (Recommended by research)

**What it is:** Rust backend + web frontend (HTML/CSS — you can still use Dioxus/Leptos for the UI) compiled into an actual Android APK. Tauri v2 added mobile support and you sideload the APK to your phone directly (no Play Store needed).

**Rust usage:** Very high. Backend logic in Rust, UI in Rust via WASM or any web framework.

**Offline support:** Excellent — the app runs locally on the device. Works offline by default. Sync to VPS is explicit when connected.

**Android experience:** Near-native. Uses Android's system WebView for rendering, but with Rust handling device APIs via plugins.

**Time to MVP:** Slightly longer than PWA — need to set up Android build toolchain (Android SDK), sign the APK.

**Pros:** Real offline-first, native feel, APK on your home screen, Rust everywhere
**Cons:** More complex build setup, APK updates require reinstalling (or a self-update mechanism), less battle-tested on Android than iOS

---

### Option C: Dioxus 0.7 Mobile

**What it is:** Dioxus (Rust UI framework) can compile directly to Android. All Rust, RSX syntax.

**Rust usage:** 100%.

**Offline support:** Excellent — local app on device.

**Android experience:** Good — uses WebView for rendering. Very similar to Tauri v2 in practice.

**Time to MVP:** Short to medium. Slightly simpler if you're already learning Dioxus.

**Pros:** 100% Rust, consistent with web Dioxus if you also want a desktop version
**Cons:** Mobile support in Dioxus is less mature than Tauri v2's mobile story, community is smaller

---

### Option D: Flutter

**What it is:** Google's cross-platform framework. Dart language. Compiles to native Android code.

**Rust usage:** None in the app. Rust only on VPS backend if at all.

**Android experience:** Excellent — compiles to actual native ARM code. Best mobile UX quality.

**Offline support:** Excellent built-in.

**Time to MVP:** Short, with hot-reload development. But Dart is a new language to learn.

**Pros:** Best mobile UX, huge ecosystem, excellent docs
**Cons:** No Rust on mobile side, Dart is another language to learn

---

### Option E: React Native + Rust backend

**What it is:** React Native (JavaScript/TypeScript) for mobile UI. Rust API on VPS. Phone talks to VPS.

**Rust usage:** Server only.

**Android experience:** Excellent — native components.

**Time to MVP:** Medium-Long. Two separate codebases (mobile JS + server Rust), complex sync.

**Pros:** Best native UI quality
**Cons:** Highest complexity, most moving parts, JavaScript everywhere on mobile

---

### Research Verdict

| Option | Mobile Quality | Offline | Rust Usage | MVP Speed | Complexity |
|--------|---------------|---------|------------|-----------|------------|
| PWA | Good | Good | High | ⭐ Fastest | Low |
| **Tauri v2** | **Near-native** | **Excellent** | **High** | **Fast** | **Medium** |
| Dioxus Mobile | Good | Excellent | 100% | Fast | Medium |
| Flutter | Excellent | Excellent | None | Fast | Medium |
| React Native | Excellent | Excellent | Low | Slow | High |

Research strongly recommends **Tauri v2** for this use case. PWA is the fastest MVP path but has real offline limitations. Tauri v2 hits all requirements with manageable complexity for a Rust developer.

**My choice:** `[ ] PWA`  `[x] Tauri v2`  `[ ] Dioxus Mobile`  `[ ] Flutter`  `[ ] React Native`

**Notes:** _Tauri v2 selected as the recommended starting point_

---

## Section 2: VPS Provider

> **The question:** Where does the server-side code and data live?

Budget: ~$22 USD/month total for everything. VPS should be well under $15/month ideally.

### Easiest to Set Up

**Render** — Connect GitHub repo, it builds and deploys automatically. Very beginner-friendly.
- Free tier: yes (slow cold starts)
- Paid: $7/month (512MB RAM) — may be tight for Rust + hledger
- No Linux admin needed

**Railway** — Similar to Render, pay-as-you-go
- $5 credit/month free
- Then ~$5-10/month for a small Rust app
- Very easy UI

**DigitalOcean** — Traditional VPS but excellent docs and UI
- $6/month for 1GB RAM, 25GB storage
- $200 free credit for new users (60 days)
- You manage the server (some Linux knowledge needed)

---

### Best Free Tier

**Oracle Cloud Always Free** — Most generous by far:
- 4 vCPU, 24GB RAM, 200GB storage — for free, indefinitely (on ARM)
- Rust compiles for ARM (aarch64) fine
- **Caveat:** Oracle has terminated free accounts without warning. Risk is real.

**Render** — Free web service, but sleeps after inactivity

---

### Best Value (Specs per Dollar)

**Hetzner Cloud** — European cloud, excellent reputation
- €3.79/month (~$4.20 USD) for 2 vCPU / 4GB RAM / 40GB storage
- US data centers available (Ashburn VA, Hillsboro OR)
- Very reliable, good performance

**Contabo** — Cheapest raw specs
- ~$5/month for 4 vCPU / 8GB RAM / 150GB — incredible specs for price
- Reputation: mixed (more downtime reports than Hetzner)
- Unmanaged — you do everything yourself

**Vultr** — Good balance
- $2.50/month for 512MB (probably too small), $6/month for 1GB
- Toronto data center available

---

### Recommendation Summary

| Provider | Price/mo (USD) | RAM | Notes |
|----------|---------------|-----|-------|
| Oracle Free | $0 | 24GB | Real risk of termination |
| Render | $0-7 | 512MB | Easy, but may be slow |
| **Hetzner** | **$4.20** | **4GB** | **Best value + reliable** |
| DigitalOcean | $6 | 1GB | Best docs, $200 credit |
| Contabo | $5 | 8GB | Cheapest specs, mixed reliability |

**Recommended:** Hetzner CAX11 (ARM) at ~$4.20/month. Excellent reliability, generous specs, Rust compiles to ARM fine.

**My choice:** `[ ] Oracle Free`  `[ ] Render`  `[ ] Hetzner`  `[x] DigitalOcean`  `[ ] Contabo`  `[ ] Other`

**Notes:** 2GB Droplet (~$12 USD/month). $200 credit lasts ~16 months. Planned migration to Hetzner before credit expires. No cross-compilation friction since both local Linux and DO are x86_64. CI/CD pipeline will handle deploys — git push triggers GitHub Actions build + deploy to Droplet.

---

## Section 3: LLM Integration

> **The question:** What AI powers the intelligent features?

Use case: ~50-100 calls/day. Tasks: categorize transactions, summarize journal entries, answer questions about your data, extract info from notes.

Estimated monthly usage: 1.5M input tokens + 600K output tokens per month at 100 calls/day.

### Easiest to Integrate

**OpenAI (gpt-4o-mini)** — Most mature ecosystem
- `async-openai` Rust crate: robust, widely used
- Most tutorials and examples exist for OpenAI
- $0.59/month at 100 calls/day
- $5 free credits for new users (lasts ~8 months at your usage)

**Anthropic (claude-haiku-4-5)** — Clean API, good Rust community crate
- `anthropic-rs` community crate
- $1.13/month at 100 calls/day
- No free tier (sometimes $5 new user credits)

---

### Best Free Tier

**Google Gemini API (gemini-2.0-flash)** — Winner by far
- **1,500 requests/minute free tier, no expiration**
- At 100 calls/day you will never pay a cent
- 1 million token context window
- No official Rust SDK — use HTTP REST API with `reqwest`

---

### Best Value

**Google Gemini Flash** — Free = infinite value

**OpenAI gpt-4o-mini** — $0.59/month is negligible

**Claude haiku-4-5** — $1.13/month, possibly best quality for reasoning tasks

---

### Local Ollama (Self-Hosted)

- Phi-3 Mini needs 4GB RAM minimum. **Won't run on a 2GB VPS.**
- Any model worth running needs 4-8GB RAM, meaning a more expensive VPS
- Quality is noticeably lower than API models for summarization/Q&A tasks
- No per-call cost but VPS cost goes up significantly
- **Verdict:** Not recommended for MVP. Revisit if privacy becomes paramount.

---

### Monthly Cost Estimate at 100 calls/day

| LLM | $/month | Notes |
|-----|---------|-------|
| **Gemini Flash** | **$0** | Free forever at your usage level |
| GPT-4o mini | $0.59 | Near-free, excellent quality |
| Claude Haiku | $1.13 | Slightly pricier, excellent reasoning |
| Mistral Small | $3.30 | No free tier |
| Local Ollama | $0 API + higher VPS cost | Not feasible on small VPS |

**Modular design note:** In Rust, define a `trait LlmClient { async fn complete(...) }` and implement it for each provider. Switch providers by changing config, not code.

**My choice:** `[x] Gemini Flash (free)`  `[ ] GPT-4o mini`  `[ ] Claude Haiku`  `[ ] Mistral`  `[ ] Ollama`

**Notes:** _______________________________

---

## Section 4: Database & Offline Strategy

> **The question:** Where does data live locally and how does it sync?

### Database: SurrealDB

**CHOSEN: SurrealDB (embedded) — not SQLite.**

Reasoning: 13 features with unknown final data shapes, graph relationships needed (People Tracker, Knowledge Compounder), schema flexibility critical for an evolving personal app. SurrealDB is multi-model (relational + document + graph), Rust-native, and supports embedded single-file mode identical to SQLite's usage pattern.

- `surrealdb` crate with `kv-surrealkv` storage engine for embedded (Tauri device + VPS)
- Same query language (SurrealQL) works in both embedded and server mode
- Schemaless by default, opt-in constraints per table as patterns stabilize
- hledger plaintext journals remain separate for financial data (unchanged)

**Data lives in two places:** on-device (Tauri embedded SurrealDB) and on VPS (SurrealDB server).

---

### Offline Sync Strategy

**CHOSEN: Event Sourcing**

Events are append-only immutable facts. Sync is simply exchanging events the other side hasn't seen — no conflict resolution needed because events describe *what happened*, not *current state*.

```
event: { id, type, aggregate_id, timestamp, payload: {flexible JSON} }
```

Read models (queryable projections) are built from events and can be rebuilt anytime. Schema evolution = change how you interpret events, not the events themselves.

**Sync flow:**
1. App opens → query VPS for all events since `last_sync_timestamp`
2. Send local events VPS hasn't seen
3. Both sides replay new events → read models update
4. No LWW, no conflict resolution, no migrations

**MVP scope implication:** Event sourcing + SurrealDB requires ~1 week of infrastructure before first visible feature. MVP must target 1-2 features done properly rather than 3 features done quickly.

**My choice:** `[ ] Simple polling sync`  `[ ] CRDTs`  `[x] Event sourcing + SurrealDB`  `[ ] CR-SQLite`

**Notes:** Accepted complexity tradeoff for long-term flexibility. March MVP scoped to journal + tasks only.

**Multi-device note:** Phone + laptop + desktop are all active devices. Concurrent edits ARE possible (e.g. Obsidian-style cross-device workflows). Event sourcing preserves all edits as events — no silent data loss. Read model reconciles by latest timestamp for MVP; more sophisticated merge can come later. Near-real-time sync (polling every 10-30s while app is open, or WebSocket push) is a cycle 2 goal — not MVP.

---

## Section 5: Feature Service Map

> **The question:** For each feature, which external service (if any) will power it?

Features marked with 🔧 need external services. Features marked with 📱 are pure app logic — no external services needed.

---

### 📱 Decisions
**External services needed:** None
**Implementation:** Local SQLite. UI for pros/cons entry, decision log, outcome tracking.

---

### 📱 Note Taking / Daily Journal
**External services needed:** None
**Editor component:** CodeMirror 6 (JavaScript, MIT license — same editor Obsidian uses). Excellent Android touch support. Bundled locally, works offline.
**UI stack:** Dioxus (Rust) for all app chrome. CodeMirror 6 embedded as a JS component via Tauri IPC bridge. Used everywhere text editing happens — not just journal, but decisions, people notes, knowledge entries.
**Data model:** Single raw note type. LLM derives all structured data (tags, insights, tasks, categories) as a separate `note_llm_processed` event. Re-process any note with a better model/prompt at any time.
**Event shape:**
- `note_created` → `{raw_text: "...", date}`
- `note_updated` → `{note_id, raw_text: "..."}`
- `note_llm_processed` → `{note_id, derived: {tags, insights, tasks_found, mood, ...}}`

---

### 📱 Focus
**External services needed:** None
**Design decision:** Not a standalone feature. Focus is a workflow and UI mode baked into the Task Manager. The underlying problem (shiny object syndrome, tasks outpacing completions) is solved by Task Manager's event model — abandoning and deferring tasks are first-class actions, not afterthoughts. Pomodoro timer and focused view are UI layers on top of `task_focused` event, not separate features.

---

### 📱 Goal Setter / Project Manager
**External services needed:** None
**Design decisions:**
- Goal and Project are the same thing — variable-depth tree. A sub-goal is a goal with a parent. No fixed hierarchy levels.
- SurrealDB graph relations: `RELATE goal->contains->goal` and `RELATE goal->contains->task`
- **Status model:** `backlog` → `active` → `paused` / `completed` / `abandoned`
- Backlog: fuzzy ideas, no criteria required. Active: SMART criteria enforced before activation.
- **Active goal limit:** starts at 5, user-editable but with friction — requires written justification, stored as an event, history visible. New limit has 24-hour cooling-off period.
- Goals connect to tasks — tasks surface "why am I doing this?" context.
- **Event shape:** `goal_created`, `goal_activated` (+ SMART criteria), `goal_shelved`, `goal_completed` (+ outcome), `goal_abandoned` (+ reason), `goal_limit_changed` (+ justification, old/new limit)

---

### 📱 Task Manager
**External services needed:** None
**Design decisions:**
- Tasks are leaf nodes in the goal tree. Orphan tasks (no goal) are valid.
- **No task-level hard deadlines.** Urgency is derived from the parent goal's deadline. App computes: "goal due in 3 weeks, 8 tasks remaining → need ~3/week."
- **Scheduled date** (intent to do today) is separate from goal deadline. Rolls forward only by explicit deferral, never silently.
- **Deferral is a two-part justification:** why today failed + why the new date will succeed. Stored as event, enables LLM pattern analysis over time.
- **Opportunistic completion:** tasks can be pulled forward and marked done ahead of schedule (`task_advanced` event).
- **Event shape:**
  - `task_created` → `{title, goal_id?, scheduled_date?}`
  - `task_scheduled` → `{task_id, date}`
  - `task_focused` → `{task_id}` (Pomodoro/focus mode hooks here)
  - `task_deferred` → `{task_id, from_date, to_date, blocked_by, confidence}`
  - `task_advanced` → `{task_id, original_date, completed_at}`
  - `task_completed` → `{task_id}`
  - `task_abandoned` → `{task_id, reason}`

---

### 📱 Routine Manager
**External services needed:** None
**Core concept:** A time budget — finite minutes in a day, intentionally allocated to repeated actions.
**Design:**
- **Routine Groups** (templates): named sets of related items with frequency (weekday, weekend, weekly) and time of day. e.g. "Morning Weekday", "Weekend Meal Prep".
- **Routine Items** within groups: individual habits with estimated duration and order.
- **Daily checklist view:** expected routine groups for today shown as expandable checkboxes. Tapping an item auto-logs the completion timestamp — replaces manual journal entries entirely.
- **Calendar/time map view:** week view showing how groups fill each day's time budget. Surfaces free windows where new habits could slot in without disrupting existing ones.
- **Goal linkage:** when an active goal requires a new habit, it gets added to a routine group with a `goal_id` so progress is visible in both places.
- **Event shape:**
  - `routine_group_created` → `{name, frequency, time_of_day}`
  - `routine_item_added` → `{group_id, name, estimated_duration_min, order}`
  - `routine_item_completed` → `{item_id, group_id, date, completed_at}` ← timestamp auto-logged on checkbox tap
  - `routine_item_skipped` → `{item_id, group_id, date, reason?}`
  - `routine_group_modified` → `{group_id, changes, justification}`

---

### 📱 People Tracker (Personal CRM)
**External services needed:** None
**Design:** Low-friction note-based system. Same note → LLM derivation pattern as journal. Write freely about a person or interaction; LLM extracts structured data (name, employer, birthday, interests, connections). Natural language search is the primary retrieval interface.
**Graph model:** SurrealDB relationships capture connections between people (`knows`, `introduced_by`, `works_at`). LLM extracts these edges from raw text automatically.
**Event shape:**
- `person_noted` → `{raw_text: "Met Marcus at design meetup..."}`
- `person_llm_enriched` → `{person_id, derived: {name, employer, birthday, interests, connections}}`
- `interaction_logged` → `{person_id, raw_text: "Had coffee, mentioned..."}`
- `interaction_llm_enriched` → `{interaction_id, derived: {...}}`

---

### 🔧 Budget / Financial Tracking

**Core data:** hledger plaintext journal files (already decided). Rust calls `hledger` CLI via subprocess.

**For receipt/invoice OCR (photos → data):**

| Service | $/page | Free tier | Accuracy | Notes |
|---------|--------|-----------|----------|-------|
| **Mindee** | $0.10 | 250 pages/mo | ⭐ Very high | Structured JSON output for receipts, easiest to use |
| AWS Textract | $0.015 | 100 pages/mo | High | Analyze Expense model is receipt-specific |
| Azure Doc Intel | $0.01 | 500 pages/mo | High | Prebuilt receipt model |
| Google Vision | $0.0015 | 1000 units/mo | Good | General OCR, less structured for receipts |
| Tesseract | Free | N/A | Moderate | Self-hosted, requires significant pre-processing work |

**Easiest:** Mindee — structured JSON response, purpose-built for receipts, 250 free/month
**Best free tier:** Google Cloud Vision (1000 general OCR/month free)
**Best value:** Azure Document Intelligence (500 pages/month free on receipt model)

**For tax filing:** No API integration planned. Receipts + journals provide raw data; filing itself is manual.

**My choice for OCR:** `[x] Mindee`  `[ ] AWS Textract`  `[ ] Azure`  `[ ] Google Vision`  `[ ] Tesseract (self-hosted)`  `[ ] Skip for MVP`

**Notes:** 250 pages/month free — well above personal usage (~20-40/month). Watch: per-product quotas (Receipt API and Invoice API may be separate buckets), page counting is literal (3-page PDF = 3 pages). No "accidentally leave running" risk — pay-per-call only.

**Visualization:** Paisa (open source, runs on VPS, reads same hledger journal files). Displayed inside Tauri app as an embedded WebView panel. Protected via Tailscale (private network VPN — free for personal use, devices connect as if on same LAN, Paisa never exposed to public internet).

**Bank statement import:** Primary — Paisa's Handlebars templating (one-time setup per bank). Fallback — LLM (Gemini) extraction when template fails or confidence is low. LLMs are format-agnostic and resilient to bank format changes. Always show mandatory user review/confirmation step before committing to hledger — format changes surface as bad preview data, not silent corruption.

**Paystub import:** Mindee payslip API or LLM (Gemini) extraction from PDF text — consistent format from same employer makes LLM reliable here.

**Architecture:** Our app = capture friction removal. Paisa = statement import + visualization. hledger journal files = shared data layer between both.

---

### 🔧 Locations

**Design:** Location is cross-cutting metadata on ALL events — not a standalone feature. Every event fired while app is open captures `{lat, lon}` automatically. Reverse geocoded lazily when online. "Locations" feature = a query/view layer over existing event data.

**Reverse geocoding service:** HERE Maps (250K requests/month free, no billing setup, better privacy than Google).
- Raw coordinates captured instantly from Tauri GPS plugin (no network needed)
- HERE API called in background when online → returns human-readable address
- Both stored: `{lat, lon, place_name: "Tim Hortons, 277 King St W, Toronto"}`
- Coordinates are ground truth; place name is derived convenience (can re-geocode if wrong)

**Map display:** Leaflet.js (open source, MIT) with OpenStreetMap tiles (free). Plots event dots on map. Tap dot → see all events from that location. Supports chronological and spatial views.

**Views:**
- Map view: dots on Leaflet map, clustered by proximity
- Chronological: "where were you on March 3rd?"
- Pattern view: "places you visit most frequently"

**My choice:** `[ ] Google Maps`  `[x] HERE Maps`  `[ ] OpenStreetMap/Nominatim`  `[ ] Mapbox`

**Notes:** Leaflet.js confirmed for map rendering. Location as event metadata means location history is automatic from normal app use — no separate logging action needed.

---

### 🔧 Scheduler / Calendar

**Service:** Google Calendar API (free, ~1M queries/day, full CRUD, native to Android).
**OAuth setup:** One-time 30-60 min setup via Google Cloud Console. Required before any calendar access.

**Mode:** LLM-assisted scheduling (Mode B):
- User makes natural language request ("find me 2 hours for deep work this week")
- App fetches current Google Calendar events + loads Routine Manager time blocks
- LLM receives full context → suggests available slots that respect both commitments
- User confirms → app writes event via Google Calendar API
- Routine Manager and Scheduler share context — time budget informs scheduling suggestions

**My choice:** `[x] Google Calendar API`  `[ ] CalDAV`  `[ ] Microsoft Graph`  `[ ] Skip for MVP`

**Notes:** Cycle 2 feature — Routine Manager must exist first for Mode B to have routing context. Architecture supports it from day one, sequencing requires it second.

---

### 🔧 Meal Tracker

**Design:** Maximum frictionless capture. Raw input (photo, barcode, text) → LLM derives structure. Never ask for portion sizes — assume standard servings. Rough log of everything beats precise log of some things.

**Capture modes:**
- Food photo → Gemini vision (free, already in stack) → food identification → USDA lookup
- Barcode scan → Open Food Facts lookup (exact product, full nutrition label)
- Natural language → LLM extracts foods → USDA lookup → estimated nutrition

**Nutrition output:** Qualitative balance checks, not precise macro tracking.
- "No vegetables logged today" / "This week is heavy on carbs"
- Food-mood correlation via journal cross-reference over weeks (acknowledging digestion lag)

**Services:**
- **USDA FoodData Central** — whole foods, free, no key required
- **Open Food Facts** — 2M+ packaged foods, barcode lookup, free, no key required
- **Gemini vision** — food photo recognition, already in LLM stack, no extra cost

**My choice:** `[x] USDA + Open Food Facts + Gemini vision`

**Notes:** No paid nutrition service needed. Gemini handles image recognition as part of existing LLM integration.

---

### 🔧 Knowledge Compounder

**Design:** Not a separate capture system — a processing pipeline on top of the single note type. Every note gets embedded; related ideas surface via semantic search and proactive LLM connections.

**Search:** Semantic (vector) search via:
- **Gemini text-embedding-004** (free tier) — generates meaning vectors for notes and queries
- **SurrealDB native vector search** (built-in since v1.3) — stores and queries vectors alongside documents, no separate vector DB needed

**Proactive surfacing:** LLM periodically connects new notes to older ones:
- "This connects to something you wrote 6 weeks ago — want to revisit it?"
- Spaced repetition of valuable insights without manual flashcards

**Event shape:**
- `note_embedding_generated` → `{note_id, vector: [...768 floats], model: "text-embedding-004"}`
- `knowledge_connection_surfaced` → `{note_id_a, note_id_b, connection_summary, surfaced_at}`

**My choice:** `[x] Gemini embeddings + SurrealDB native vector search`

**Notes:** No additional service or database needed. Same infrastructure as everything else. Embedding runs as a background task after note creation.

**Media capture (Knowledge Compounder input sources):**
- Voice note (Android mic via Tauri) OR direct text entry — both first-class, context-dependent
- YouTube URL → YouTube Data API fetches transcript → LLM reads structured transcript + user thoughts
- Podcast URL → Podcast Index API (free) + transcript if available → same pipeline
- Book/media mentioned in note → Open Library API (free) lookup → structured metadata attached
- Principle: prefer structured sources (transcripts, metadata APIs) over asking LLM to infer from descriptions
- Derived media library builds automatically from referenced works in notes

**LLM consistency architecture (applies to ALL derivation pipelines):**
1. **Structured output mode** — Gemini JSON schema enforcement, LLM constrained to defined output shape
2. **Tool use / function calling** — LLM chooses from finite typed actions (`create_task`, `save_insight`, `log_person`) rather than free-form extraction. Validated schemas per tool.
3. **Deterministic pre-processing first** — extract URLs, parse dates, detect barcodes with code before LLM sees raw text
4. **Prompt versioning** — every `note_llm_processed` event stores `{prompt_version, model}`. Improved prompts can re-process old notes without losing raw input.
5. **Confidence + review gate** — low-confidence derivations (especially financial) surface a review card before committing

---

### 🔧 Archive

**Design:** Upload document → LLM processes it → flat storage with rich metadata → retrieve by natural language.

**Organization:** Flat filesystem on VPS (no folder hierarchy). LLM assigns tags and writes auto-description on upload. Folders are a manual browsing system — when search is the primary interface, tags + semantic search replace them entirely.

**Upload flow:**
1. User uploads file (photo, PDF, scan)
2. Gemini vision / Mindee extracts text
3. LLM writes auto-description + assigns tags + suggests category
4. User description optional (low friction)
5. Text + embedding stored in SurrealDB alongside file path

**Retrieval:** Natural language query → semantic search → document displayed in-app. Tag filtering also available.

**Sharing:** Android native share sheet — Tauri hands the file to the OS, user picks any installed app (WhatsApp, Gmail, Drive, etc.). No direct integration with any messaging service needed.

**Event shape:**
- `document_archived` → `{filename, mime_type, storage_path, user_note?}`
- `document_llm_processed` → `{doc_id, extracted_text, auto_description, tags}`
- `document_shared` → `{doc_id, shared_at}` (method tracked by OS, not app)

**My choice:** `[x] Gemini/Mindee OCR + SurrealDB semantic search + Android share sheet`

---

### 💡 Future Idea: Communication Manager
**Status:** Not assigned to any cycle — idea to revisit.
**Concept:** Read messages (email, SMS, WhatsApp), draft responses in user's voice, user reviews/edits/approves before sending. Strictly opt-in per conversation.
**Technical constraints:**
- Email: Gmail API (OAuth, same pattern as Calendar) — fully feasible
- SMS: Android SMS permissions — feasible
- WhatsApp: No official personal API. Meta blocks third-party access. No clean solution without ToS violation.
**Privacy note:** Feeds other people's messages to an LLM API. Tension with data sovereignty value. Local LLM (Ollama) becomes compelling specifically for this feature. Resolve before implementing.

---

## Section 6: MVP Scope

> **The question:** What gets built first?

**Revised after full feature review.** Original proposal (financial capture) superseded.

**Week 1 — Infrastructure (nothing visible, all foundational):**
- Tauri v2 Android APK build + sideload validation
- SurrealDB embedded (on-device) + server (VPS)
- Event store foundation
- Basic sync (polling on app open)
- CI/CD pipeline: GitHub Actions → DigitalOcean Droplet
- Gemini LLM pipeline (structured output + tool calling architecture)
- CodeMirror 6 editor component (reusable across all text features)

**Weeks 2-3 — Two features:**
1. **Journal / Notes** — proves full capture → LLM derivation pipeline. Replaces Obsidian for daily journaling.
2. **Routine Manager** — checkboxes + auto-timestamps replace manual "woke at X, done exercise at Y" journal entries. Validates multi-device sync with real daily usage.

**MVP validates:**
- Tauri APK sideloading on Android actually works
- SurrealDB + Event Sourcing sync across phone + laptop + desktop
- CI/CD deploy pipeline end-to-end
- LLM derivation pipeline (tool calling, structured output, prompt versioning)

**Cycle 2:** Task Manager, Goal Setter, Calendar (LLM scheduling), Budget (hledger + Mindee + Paisa), Locations
**Cycle 3+:** Meal Tracker, People Tracker, Knowledge Compounder (semantic search), Archive

---

## Section 7: Tech Stack Summary

Based on research, here's the proposed full stack:

| Layer | Choice | Reasoning |
|-------|--------|-----------|
| App framework | Tauri v2 | Rust + Android APK + offline-first |
| UI language | Dioxus (WASM in Tauri) or HTML/CSS/JS | Dioxus keeps full Rust; HTML/CSS may be faster for MVP |
| Backend language | Rust (Axum) | Personal preference, performance |
| Database (device) | SQLite via Tauri plugin | On-device local storage |
| Database (VPS) | SQLite | Single-user, no need for Postgres |
| Financial data | hledger (plaintext journal files) | Existing ecosystem, powerful reporting |
| Sync strategy | Simple polling, last-write-wins | Appropriate for single user |
| LLM | Gemini Flash API | Free tier covers all usage |
| VPS | Hetzner CAX11 | ~$4/month, excellent reliability |
| Receipt OCR | Mindee (250/month free) | Purpose-built for receipts |
| Maps/Geocoding | HERE Maps or OpenStreetMap | 250K/month free or self-hosted |
| Calendar | Google Calendar API | Android-native, free |
| Nutrition | USDA + Open Food Facts | Free, no key for Open Food Facts |
| Search | SQLite FTS5 (MVP), Qdrant + Gemini later | Start simple |

---

## Open Questions

1. **Tauri UI layer:** ✅ ANSWERED — Dioxus (Rust) for all app chrome. CodeMirror 6 (JS) for editor component via Tauri IPC bridge.

2. **Financial MVP:** Deferred to Cycle 2. Existing `.ledger` files will be importable — hledger reads them natively.

3. **Sync timing:** ✅ ANSWERED — Sync on app open (MVP). Near-real-time polling (every 10-30s while open) is Cycle 2. Multi-device concurrent edits handled by Event Sourcing (no silent data loss).

4. **Calendar auth:** ✅ ANSWERED — Google Calendar API with OAuth. One-time 30-60 min setup. Cycle 2 feature.

5. **OPEN — YouTube Data API quota:** Free tier is 10,000 units/day. One transcript fetch costs ~50-100 units. Should verify this covers expected usage before implementing Knowledge Compounder media capture.

6. **OPEN — SurrealDB embedded maturity:** Monitor before Cycle 1 implementation. Newer than SQLite — may hit rough edges in the `kv-surrealkv` embedded Rust crate.

---

*This document is a research artifact — decisions marked above will feed into `architecture.md` once choices are confirmed.*
