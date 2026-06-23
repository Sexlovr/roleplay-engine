# syntax=docker/dockerfile:1
#
# Roleplay Engine — multi-stage build for Hugging Face Spaces (Docker SDK).
#
# Stage 1 compiles the Rust/WASM frontend with Trunk.
# Stage 2 compiles the Rust backend.
# Stage 3 is a slim runtime image that serves both on port 7860.
#
# Build from the local context (what Hugging Face does when it clones your Space
# repo). For a Space that clones a public GitHub repo at build time instead, see
# `Dockerfile.from-git`.

############################  frontend build  #############################
FROM rust:1-bookworm AS fe

ARG TRUNK_VERSION=0.21.7
RUN rustup target add wasm32-unknown-unknown \
 && curl -fsSL "https://github.com/trunk-rs/trunk/releases/download/v${TRUNK_VERSION}/trunk-x86_64-unknown-linux-gnu.tar.gz" \
    | tar -xz -C /usr/local/bin trunk

WORKDIR /app
COPY . .
RUN cd frontend && trunk build --release

############################  backend build  #############################
FROM rust:1-bookworm AS be

WORKDIR /app
COPY . .
RUN cargo build --release -p backend

############################  runtime  ###################################
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/* && \
    useradd --create-home --uid 1000 app

COPY --from=fe /app/frontend/dist /app/dist
COPY --from=be /app/target/release/backend /app/backend
RUN mkdir -p /data && chown app:app /data

USER app
WORKDIR /app

ENV STATIC_DIR=/app/dist DATA_DIR=/data PORT=7860
EXPOSE 7860

CMD ["/app/backend"]
