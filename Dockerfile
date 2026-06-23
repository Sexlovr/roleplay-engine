# syntax=docker/dockerfile:1
#
# Roleplay Engine — multi-stage build for Hugging Face Spaces (Docker SDK).
#
# Stage 1 compiles the Rust → WASM bundle with Trunk; stage 2 serves the static
# `dist/` with nginx on port 7860 (HF's required app port).
#
# This builds from the local build context (`COPY . .`), which is what Hugging
# Face does when it clones your Space repo and builds the Dockerfile — and it
# also works for a private source repo (no clone credentials needed). For a
# Space that should clone a *public* GitHub repo at build time instead, see
# `Dockerfile.from-git`.

############################  build stage  ############################
FROM rust:1-bookworm AS build

# WebAssembly target + Trunk (prebuilt binary, so we don't compile Trunk itself).
ARG TRUNK_VERSION=0.21.7
RUN rustup target add wasm32-unknown-unknown \
 && curl -fsSL "https://github.com/trunk-rs/trunk/releases/download/v${TRUNK_VERSION}/trunk-x86_64-unknown-linux-gnu.tar.gz" \
    | tar -xz -C /usr/local/bin trunk

WORKDIR /app

# Build the app. cargo (via trunk) fetches crates; trunk also pulls the matching
# wasm-bindgen + wasm-opt on first run.
COPY . .
RUN trunk build --release

############################  serve stage  ###########################
FROM nginxinc/nginx-unprivileged:1.27-alpine

# Serve config (listens on 7860) + the built static bundle.
COPY nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=build /app/dist /usr/share/nginx/html

EXPOSE 7860
# nginx-unprivileged already runs as a non-root user and starts nginx in the
# foreground via its default entrypoint/CMD.
