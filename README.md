# Roleplay Engine

A Janitor-AI-style character-chat frontend, built in **Rust** with [Leptos](https://leptos.dev) (CSR) and bundled by [Trunk](https://trunkrs.dev). Standalone, pure-frontend SPA — no backend. Dark theme, mobile-first, responsive.

![home](docs/desktop.png)

## Features

- **Character gallery** — responsive card grid (avatar, name, tagline, tags, creator, chat/like counts).
- **Live filtering** — search box + tag chips + NSFW toggle, all reactive.
- **Chat view** — per-character conversation seeded with the character's intro; type a message and get an in-character reply.
- **No router dependency** — navigation is a single `RwSignal<Page>` in context.

## Stack

- Rust + Leptos 0.7 (`csr` feature)
- Trunk for WASM bundling
- Hand-written CSS (theme tokens in `:root`), Inter font

## Run

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk            # or grab a prebuilt binary
trunk serve --release          # http://127.0.0.1:8080
```

Build for deploy:

```sh
trunk build --release          # output in dist/
```

## Layout

| File | Responsibility |
|------|----------------|
| `src/main.rs`   | App shell, page state, context |
| `src/types.rs`  | `Character`, `ChatMessage`, `Page` |
| `src/data.rs`   | Mock character roster |
| `src/header.rs` | Sticky nav: logo, search, NSFW toggle, login |
| `src/home.rs`   | Hero, tag filter, card grid |
| `src/chat.rs`   | Chat view + composer |
| `style.css`     | Theme + all component styles |

Character data in `src/data.rs` is mock; wire `chat.rs`'s `send` closure to a real API to make it live.
