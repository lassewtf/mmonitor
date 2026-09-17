#!/usr/bin/env bash
set -euo pipefail

readonly ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly DEPLOY_DIR="$ROOT_DIR/deploy"

if [[ "$(uname -s)" != "Darwin" || "$(uname -m)" != "arm64" ]]; then
  echo "deploy.sh supports only macOS on Apple Silicon" >&2
  exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "required command not found: cargo" >&2
  exit 1
fi

cd "$ROOT_DIR"
cargo build --release --locked

staging="$(mktemp -d "$ROOT_DIR/.deploy.XXXXXX")"
trap 'rm -rf "$staging"' EXIT

install -m 0755 target/release/mmonitor "$staging/mmonitor"
install -m 0755 target/release/check_mmonitor_memory "$staging/check_mmonitor_memory"
install -m 0755 install.sh "$staging/install.sh"
install -m 0644 checks.toml.example "$staging/checks.toml.example"

rm -rf "$DEPLOY_DIR"
mv "$staging" "$DEPLOY_DIR"
trap - EXIT

echo "deployment package: $DEPLOY_DIR"
