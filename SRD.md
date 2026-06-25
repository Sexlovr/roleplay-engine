# Software Requirements Document — Roleplay Engine

**Version:** 1.0 (draft)
**Status:** Working baseline shipped; this SRD defines the path to production-grade.
**Owner:** Sexlovr / nxnxb
**Repository:** `Sexlovr/roleplay-engine` (private)
**Live (current):** https://nxnxb-character-chat.hf.space

---

## 1. Purpose & Scope

### 1.1 Purpose
Roleplay Engine is a self-hostable, JanitorAI-style character-chat application. Users browse or create AI characters and hold persistent, in-character conversations through a **bring-your-own-endpoint** LLM connector. Unlike hosted competitors, no model is baked in and no third party sees the conversation: the user points the app at their own OpenAI-compatible / Anthropic / Gemini / custom endpoint, and all data lives in the user's own database.

### 1.2 Product goals
- **Feature parity with JanitorAI** (character gallery, personas, lorebooks, swipes, chat memory) and beyond.
- **Provider-agnostic**: any chat endpoint works via a templated request engine.
- **Privacy-first / self-host-first**: single binary + SQLite, deployable to a $5 VPS, a Docker host, or HF Spaces.
- **Polished across every screen size** (phones → ultrawide), which the responsive overhaul (v0.2) delivered.

### 1.3 In scope (this document)
The current architecture, the functional/non-functional requirements to call it "production-grade," and a prioritized roadmap from the present working baseline.

### 1.4 Out of scope (explicitly deferred)
- Multi-tenant SaaS / user accounts with auth (current model is single-tenant / single-user-per-deployment).
- A public character marketplace with moderation.
- Native mobile apps (the PWA path is in scope; native is not).
- Training or hosting models.

---

## 2. System Overview

### 2.1 Architecture
A three-crate Cargo workspace producing one backend binary that serves both the API and the compiled frontend:

```
shared/    serde DTOs + the proxy/template engine  (compiles native AND wasm32)
backend/   axum 0.8 + tokio + rusqlite/r2d2 (SQLite) + reqwest  → single binary
frontend/  Leptos 0.7 (CSR) + Trunk  → static wasm/JS/CSS bundle in /dist
```

- **No router**: navigation is a single `RwSignal<Page>` in Leptos context.
- **Server-side LLM calls**: the API key lives only in the DB; the browser never holds it. The backend renders the provider request from a template and proxies it.
- **Persistence**: SQLite at `${DATA_DIR}/roleplay.db` (WAL mode, foreign keys on). On HF Spaces, `DATA_DIR=/data` is a mounted persistent bucket.
- **Static serving**: `${STATIC_DIR}` (default `./dist`) with SPA fallback to `index.html`.

### 2.2 Deployment topology
| Target | How | Notes |
|---|---|---|
| HF Spaces | `Dockerfile.from-git` clones GitHub `main` at build (repo public) **or** `deploy-hf.sh` rsync (repo private) | Content-moderation risk for NSFW; can be re-flagged. |
| VPS / Docker host | 3-stage `Dockerfile` → slim runtime image on :7860 | Recommended long-term home for adult content. |
| Local dev | `trunk serve --release` (FE) + `cargo run -p backend` (BE) | |

### 2.3 Data model (current SQLite schema)
- **characters** — id, name, tagline, description, personality, scenario, first_message, avatar, tags(JSON), creator, messages, likes, nsfw, created_at, + V2/V3 cols (spec_version, creator_notes, system_prompt, post_history_instructions, mes_example, alternate_greetings(JSON), lorebook(JSON)).
- **chats** — id, character_id (FK CASCADE), title, memory, created_at, updated_at.
- **messages** — id, chat_id (FK CASCADE), from_user, text, created_at, variants(JSON), variant(idx).
- **settings** — key/value (stores the proxy store + persona store as JSON).

Migrations are additive & idempotent (`ALTER TABLE ADD COLUMN` guarded by `PRAGMA table_info`).

### 2.4 Prompt assembly ("the brains")
`build_system()` composes the system prompt in this order: character system prompt (or default framing) → summary → personality → about → scenario → example dialogue → persona ("your scene partner") → triggered lorebook (world info) → chat memory. `post_history_instructions` (jailbreak/UJB) are placed **after** the chat history for maximum recency. History is trimmed so the first turn is always a user turn (Anthropic-safe).

---

## 3. Functional Requirements

IDs are `FR-<area>-<n>`. **MUST** = required for production-grade; **SHOULD** = strongly desired; **MAY** = optional/roadmap.

### 3.1 Character gallery & discovery
- **FR-GAL-1 (MUST)** Display a responsive grid of character cards (avatar, name, tagline, tags, message/like counts).
- **FR-GAL-2 (MUST)** Filter by free-text search across name/tagline/tags.
- **FR-GAL-3 (MUST)** Filter by tag chips and by an NSFW visibility toggle (default off).
- **FR-GAL-4 (MUST)** Sort by Popular / New / Trending; paginate results.
- **FR-GAL-5 (SHOULD)** Server-side pagination & filtering once the library exceeds a few hundred characters (today filtering is client-side over the full list).
- **FR-GAL-6 (MAY)** "Favorites" / pinning.

### 3.2 Character authoring
- **FR-CHR-1 (MUST)** Create a character via a sectioned form (Identity / Persona / Greeting / Lorebook / Advanced) with a live avatar preview.
- **FR-CHR-2 (MUST)** Edit and delete existing characters.
- **FR-CHR-3 (MUST)** Support V1 / V2 / V3 card fields (system_prompt, post_history_instructions, mes_example, alternate_greetings, creator_notes, lorebook).
- **FR-CHR-4 (MUST)** Import SillyTavern cards: JSON and PNG (`ccv3`/`chara` tEXt chunks).
- **FR-CHR-5 (SHOULD)** Export a character back to V2/V3 JSON and to PNG (card round-trip).
- **FR-CHR-6 (MUST)** Avatar upload with client-side downscale; reject oversized payloads at the API boundary.
- **FR-CHR-7 (SHOULD)** Per-character lorebook entries with keys + content + enabled; constant (keyless) entries always injected.

### 3.3 Chat
- **FR-CHT-1 (MUST)** Start a new chat from a character (seeds the first/greeting message).
- **FR-CHT-2 (MUST)** Continue an existing chat or start additional parallel chats per character.
- **FR-CHT-3 (MUST)** Send a message and receive an in-character reply via the active proxy.
- **FR-CHT-4 (MUST)** Per-message regenerate, edit-in-place, and delete (with a floor so a chat can't be emptied below its greeting).
- **FR-CHT-5 (MUST)** Message **swipes**: alternate greetings + regenerated variants navigable with a counter.
- **FR-CHT-6 (MUST)** Markdown rendering in messages (`*italic* **bold** \`code\` > quote ![img]`) with an XSS-safe renderer and an image-src scheme allowlist.
- **FR-CHT-7 (MUST)** Auto-scroll to newest; `Enter` sends, `Shift+Enter` newline.
- **FR-CHT-8 (MUST)** Per-chat **memory** note (user-curated facts injected into the system prompt).
- **FR-CHT-9 (MUST)** Editable per-chat title (click-to-rename).
- **FR-CHT-10 (SHOULD)** **Streaming responses** (SSE/token streaming) — currently replies arrive whole. This is the single biggest UX upgrade remaining.
- **FR-CHT-11 (SHOULD)** Stop/cancel an in-flight generation.
- **FR-CHT-12 (MAY)** Token-budget management / context-window trimming with a visible token meter.

### 3.4 Personas
- **FR-PRS-1 (MUST)** Multiple saved personas (name, description, avatar); one active at a time.
- **FR-PRS-2 (MUST)** Active persona is injected into the system prompt as the user's identity.

### 3.5 Connectors / settings
- **FR-CON-1 (MUST)** Multiple proxy profiles, each with url, headers, model, api_key, body_template, response_path, temperature, max_tokens, and its own system prompt.
- **FR-CON-2 (MUST)** Presets for OpenAI-compatible / Anthropic / Gemini / Custom.
- **FR-CON-3 (MUST)** Placeholder templating: `{{model}} {{messages}} {{messages_system}} {{system}} {{prompt}} {{temperature}} {{max_tokens}} {{api_key}}`; response extraction via dot/index path.
- **FR-CON-4 (MUST)** The API key is stored server-side only and never returned to the client.
- **FR-CON-5 (SHOULD)** "Test connection" button that does a live round-trip and surfaces the upstream error verbatim.

### 3.6 Global chat management
- **FR-CHM-1 (MUST)** A Chats tab listing all recent conversations newest-first with avatar, last-message snippet, and relative time.
- **FR-CHM-2 (MUST)** Delete a chat (cascades messages).

---

## 4. Non-Functional Requirements

### 4.1 Responsiveness & UI (delivered in v0.2)
- **NFR-UI-1 (MUST)** Usable and polished from 320px phones to ≥2560px ultrawide. Breakpoint map: ≥1600 ultrawide (wider grid), 1024–1599 docked sidebar, 768–1023 slim icon rail, ≤767 off-canvas drawer + top app bar.
- **NFR-UI-2 (MUST)** Respect safe-area insets on notched devices; chat composer and app bar honor `env(safe-area-inset-*)`.
- **NFR-UI-3 (SHOULD)** `prefers-reduced-motion` disables the aurora/background animation (already honored).
- **NFR-UI-4 (SHOULD)** WCAG AA contrast and full keyboard navigation; visible focus rings (partially present).
- **NFR-UI-5 (MAY)** Installable PWA (manifest + service worker + offline shell).

### 4.2 Performance
- **NFR-PERF-1 (MUST)** First contentful paint < 2.5s on a mid-range phone over 4G (wasm bundle is ~1.6 MB; consider compression/splitting).
- **NFR-PERF-2 (SHOULD)** Gzip/Brotli static assets; long-cache immutable hashed assets.
- **NFR-PERF-3 (SHOULD)** API p95 < 150ms for non-LLM endpoints; LLM latency is upstream-bound.
- **NFR-PERF-4 (SHOULD)** Blocking SQLite work runs on `spawn_blocking` (already done) and uses an r2d2 pool.

### 4.3 Reliability
- **NFR-REL-1 (MUST)** No panics on malformed input; multibyte-safe string handling (prior `truncate` byte-slice crash is fixed — keep regression coverage).
- **NFR-REL-2 (MUST)** Upstream LLM errors surface to the user as a dismissible, retryable banner without corrupting chat state.
- **NFR-REL-3 (MUST)** Persistent data survives restarts/redeploys (verified via `/data` bucket; health reports `persistent:true`).
- **NFR-REL-4 (SHOULD)** A documented backup/restore path for `roleplay.db`.

### 4.4 Security & privacy
- **NFR-SEC-1 (MUST)** API keys never leave the server; never logged.
- **NFR-SEC-2 (MUST)** XSS-safe markdown; image-src scheme allowlist; request body size limits.
- **NFR-SEC-3 (SHOULD)** Optional access control (single shared password / token) for public deployments, since there is currently no auth.
- **NFR-SEC-4 (SHOULD)** Secrets only via environment variables; **no tokens committed to git** (standing rule — the current GitHub PAT and HF token were pasted in plaintext and MUST be rotated).
- **NFR-SEC-5 (SHOULD)** Rate-limit the `/send` and `/regenerate` endpoints to bound cost/abuse.

### 4.5 Maintainability & quality
- **NFR-QA-1 (MUST)** `cargo test` green across shared + backend; `trunk build --release` clean.
- **NFR-QA-2 (SHOULD)** Browser-level smoke tests (Playwright) for the golden paths at representative breakpoints, runnable in CI.
- **NFR-QA-3 (SHOULD)** CI pipeline: fmt + clippy + test + build on push.
- **NFR-QA-4 (SHOULD)** Structured logging via `tracing`; a request-id on API calls.

### 4.6 Portability
- **NFR-PORT-1 (MUST)** Runs as a single container on any Docker host and on a bare VPS.
- **NFR-PORT-2 (MUST)** Configuration via env: `DATA_DIR`, `STATIC_DIR`, `PORT`.

---

## 5. External Interfaces (current REST API)

All under `/api`, JSON in/out.

| Method | Path | Purpose |
|---|---|---|
| GET | `/api/health` | data dir + persistence + db existence |
| GET/POST | `/api/characters` | list / create |
| POST | `/api/characters/import` | import ST card (JSON/PNG) |
| GET/PUT/DELETE | `/api/characters/{id}` | fetch / update / delete |
| GET/POST | `/api/characters/{id}/chats` | list/create chats for a character |
| GET | `/api/chats` | recent chats (limit 200) |
| GET/DELETE | `/api/chats/{id}` | fetch / delete a chat |
| PUT | `/api/chats/{id}/title` | rename |
| PUT | `/api/chats/{id}/memory` | update memory note |
| POST | `/api/chats/{id}/send` | send a user message, get a reply |
| POST | `/api/chats/{id}/regenerate` | regenerate last reply (new variant) |
| PUT/DELETE | `/api/messages/{id}` | edit / delete a message |
| PUT | `/api/messages/{id}/variant` | select a swipe variant |
| GET/PUT | `/api/settings` | proxy store + persona store |
| GET | `/healthz` | always-OK probe |

---

## 6. Roadmap (prioritized from the current baseline)

**P0 — production hardening**
1. Optional access password/token for public deploys (NFR-SEC-3).
2. CI: fmt + clippy + test + `trunk build` + Playwright smoke (NFR-QA-2/3).
3. Brotli/gzip + asset caching; investigate wasm size reduction (NFR-PERF-1/2).
4. Backup/restore docs for `roleplay.db` (NFR-REL-4).
5. **Rotate the compromised GitHub PAT + HF token** and move to env-only secrets.

**P1 — headline features**
6. Streaming responses + stop button (FR-CHT-10/11) — biggest perceived-quality jump.
7. Character export (JSON + PNG round-trip) (FR-CHR-5).
8. Token meter + context trimming (FR-CHT-12).
9. "Test connection" in settings (FR-CON-5).

**P2 — scale & polish**
10. Server-side gallery pagination/search (FR-GAL-5).
11. PWA install + offline shell (NFR-UI-5).
12. Accessibility audit to WCAG AA (NFR-UI-4).

**P3 — optional**
13. Favorites/pinning, group chats, multi-tenant accounts (a larger architectural change, currently out of scope).

---

## 7. Acceptance Criteria (definition of "production-grade")
A release is production-grade when: all **MUST** requirements pass; CI is green on every push; the golden paths (browse → open character → start chat → send → swipe → edit memory → create/import a character) pass Playwright at 360 / 768 / 1440px; data persists across a redeploy; no secret is present in the repo; and a public deployment is gated behind at least a shared access token.
