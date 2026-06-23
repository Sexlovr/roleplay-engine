---
title: Roleplay Engine
emoji: 🎭
colorFrom: pink
colorTo: indigo
sdk: docker
app_port: 7860
pinned: false
license: mit
---

# Roleplay Engine

A Janitor-AI-style character-chat full-stack app, built in **Rust** with
[Leptos](https://leptos.dev) (CSR frontend) and [axum](https://docs.rs/axum)
(backend). Single Docker image — frontend, backend, and SQLite database in one
process on port 7860.

> The YAML block above is [Hugging Face Space metadata](https://huggingface.co/docs/hub/spaces-config-reference) — GitHub renders it as a table, HF reads it to build the Docker Space. See [Deploy to Hugging Face](#deploy-to-hugging-face).

## Features

- **Full backend** — axum 0.8 REST API with SQLite (via rusqlite + r2d2 pool).
  All data lives on the server; nothing in localStorage.
- **Server-side LLM proxy** — your API key stays in the database, never in the
  browser. The backend builds the request, fires it with `reqwest`, and returns
  the reply. No CORS issues.
- **Character gallery** — responsive card grid with search, tag chips, sort tabs
  (Popular / New / Trending), NSFW toggle, and pagination.
- **Character detail** — collapsible definition sections (personality, first
  message, scenario) with token estimates.
- **Chat view** — per-conversation chat with send, regenerate, edit, and delete.
  Persona + per-chat memory injected into the system prompt server-side.
- **Create-a-character** — form with live card preview; persisted to the database.
- **Provider-agnostic** — configure any HTTP+JSON API (OpenAI, Anthropic, Gemini,
  or custom) via the Settings drawer. Request body template + response path cover
  every API shape.

## Stack

| Layer | Tech |
|-------|------|
| Frontend | Leptos 0.7 CSR + Trunk |
| Backend | axum 0.8 + tokio |
| Database | SQLite (rusqlite + r2d2 pool, WAL mode) |
| Templates | Hand-written CSS with Inter + Fraunces fonts |
| Deployment | Single Docker image on HF Spaces (port 7860) |

The repo is a Cargo workspace with three crates:

| Crate | Role |
|-------|------|
| `shared/` | Data types, API DTOs, LLM request templating (pure Rust — compiles for both native and wasm32) |
| `backend/` | axum HTTP server, SQLite DB layer, server-side LLM proxy |
| `frontend/` | Leptos CSR app; thin REST client over `/api/*` |

## Run locally

```sh
# 1. Build the frontend
cd frontend
trunk build --release         # produces dist/

# 2. Run the backend (serves API + static frontend)
cd ..
cargo run --release -p backend -- --data-dir ./data --static-dir frontend/dist
# App at http://localhost:7860
```

Or with Docker:

```sh
docker build -t roleplay-engine .
docker run --rm -p 7860:7860 -v rpdata:/data roleplay-engine
```

## Data persistence (Hugging Face Spaces)

The backend stores everything at `${DATA_DIR:-/data}/roleplay.db`. On HF Spaces,
`/data` is **ephemeral** unless you attach a **Storage Bucket** (available since
2024) mounted at `/data`. Attach one in your Space settings to persist characters,
chats, and configuration across rebuilds and restarts.

The app probes the data directory on startup — if it can't write, it falls back to
`./data` and logs a warning.

## API routes

```
GET  /api/health                     → { data_dir, persistent, db_exists }
GET  /api/characters                 → [Character…]
POST /api/characters                 → Character
GET  /api/characters/{id}            → Character
PUT  /api/characters/{id}            → Character
DELETE /api/characters/{id}          → { ok }
GET  /api/characters/{cid}/chats     → [ChatListEntry…]
POST /api/characters/{cid}/chats     → ChatDetail (seeds greeting as msg #1)
GET  /api/chats/{id}                 → ChatDetail
DELETE /api/chats/{id}               → { ok }
PUT  /api/chats/{id}/memory          → { ok }
POST /api/chats/{id}/send            → SendMessageResp
POST /api/chats/{id}/regenerate      → SendMessageResp
PUT  /api/messages/{id}              → { ok }
DELETE /api/messages/{id}            → { ok }
GET  /api/settings                   → SettingsResp (api_key is NEVER returned)
PUT  /api/settings                   → { ok }
```

All responses are JSON. Error responses have shape `{ "error": "…" }`.

## Deploy to Hugging Face

The repo ships two Dockerfiles:

- **`Dockerfile`** — builds from the local context (the Space's source).
  Push the repo to a Space with HF's Docker SDK and it builds + serves.
- **`Dockerfile.from-git`** — clones a public GitHub repo at build time.
  Put this as the Space's `Dockerfile` alongside a README; no source in the Space.

Deploy script (recommended):

```sh
HF_TOKEN=hf_xxx ./deploy-hf.sh <your-username>/roleplay-engine
```

## Layout

| File | Responsibility |
|------|----------------|
| `shared/src/types.rs` | `Character`, `Chat`, `Persona` (shared across backend + frontend) |
| `shared/src/dto.rs` | All API request/response types |
| `shared/src/template.rs` | LLM request templating engine (provider-agnostic) |
| `backend/src/main.rs` | Server entrypoint: pool init, migration, router, serve |
| `backend/src/db.rs` | Database init, migration, data-dir resolution |
| `backend/src/routes/*.rs` | REST handlers (characters, chats, messages, settings, health) |
| `backend/src/llm.rs` | Server-side LLM proxy (load config, build system prompt, send) |
| `backend/src/error.rs` | `AppError` → HTTP response mapping |
| `frontend/src/main.rs` | App shell, page state, context, settings bootstrap |
| `frontend/src/api.rs` | Thin `gloo-net` REST client over `/api/*` |
| `frontend/src/header.rs` | Sticky nav: logo, search, NSFW toggle, create, settings/persona |
| `frontend/src/home.rs` | Hero, sort tabs, tag filter, card grid, pagination, empty state |
| `frontend/src/character.rs` | Character detail page with chat-start CTA |
| `frontend/src/chat.rs` | Chat view: send, regenerate, edit, delete, memory panel |
| `frontend/src/create.rs` | Create-a-character form with live preview |
| `frontend/src/settings.rs` | API Settings drawer (presets, URL, model, templates) |
| `frontend/src/persona.rs` | Persona editor drawer |
| `frontend/style.css` | Design system (JAI-style indigo palette, Fraunces hero, responsive grid) |

## License

MIT
