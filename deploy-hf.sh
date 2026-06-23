#!/usr/bin/env bash
#
# Push this repo to a Hugging Face Space (Docker SDK) and trigger a deploy.
# A Space *is* a git repo, so "deploying" = committing the source there and
# pushing; HF then builds the Dockerfile and serves it on port 7860.
#
# Usage:
#   HF_TOKEN=hf_xxx ./deploy-hf.sh <hf-username>/<space-name>
#
# HF_TOKEN must be a *write* token: https://huggingface.co/settings/tokens
#
set -euo pipefail

SPACE="${1:-}"
if [[ -z "$SPACE" || "$SPACE" != */* ]]; then
  echo "usage: HF_TOKEN=hf_xxx $0 <username>/<space-name>" >&2
  exit 2
fi
: "${HF_TOKEN:?set HF_TOKEN to a write token from https://huggingface.co/settings/tokens}"

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REMOTE="https://user:${HF_TOKEN}@huggingface.co/spaces/${SPACE}"

# Create the Space if it doesn't exist yet (needs the huggingface_hub CLI).
if command -v hf >/dev/null 2>&1; then
  hf repo create "$SPACE" --repo-type space --space_sdk docker -y 2>/dev/null || true
elif command -v huggingface-cli >/dev/null 2>&1; then
  huggingface-cli repo create "$SPACE" --type space --space_sdk docker -y 2>/dev/null || true
else
  echo "note: 'hf' CLI not found — create the Space manually (SDK: Docker) if it doesn't exist:"
  echo "      https://huggingface.co/new-space?sdk=docker"
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

echo ">> cloning Space $SPACE"
if ! git clone "$REMOTE" "$WORK" 2>/dev/null; then
  echo ">> empty/new Space — initialising"
  git -C "$WORK" init -q
  git -C "$WORK" remote add origin "$REMOTE"
fi

echo ">> syncing source into the Space repo"
# Copy the app (excluding VCS + build artifacts). The Space keeps its own .git.
if command -v rsync >/dev/null 2>&1; then
  rsync -a --delete \
    --exclude='.git' --exclude='/target' --exclude='/dist' \
    --exclude='node_modules' --exclude='*.log' \
    "$REPO_DIR"/ "$WORK"/
else
  find "$WORK" -mindepth 1 -maxdepth 1 ! -name '.git' -exec rm -rf {} +
  (cd "$REPO_DIR" && git archive --format=tar HEAD) | tar -x -C "$WORK"
fi

cd "$WORK"
git add -A
git -c user.email="deploy@local" -c user.name="deploy" \
  commit -q -m "Deploy roleplay-engine" || { echo ">> nothing to deploy (no changes)"; exit 0; }

# HF Spaces use the 'main' branch.
git branch -M main
echo ">> pushing to HF"
git push -u origin main

echo ">> done. Build status: https://huggingface.co/spaces/${SPACE}"
echo ">> live (after build): https://${SPACE/\//-}.hf.space"
