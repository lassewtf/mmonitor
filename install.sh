#!/usr/bin/env bash
set -euo pipefail

readonly ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly INSTALL_DIR="/opt/ma/mmonitor"
readonly CONFIG_PATH="$INSTALL_DIR/checks.toml"

if [[ "$(uname -s)" != "Darwin" || "$(uname -m)" != "arm64" ]]; then
  echo "install.sh supports only macOS on Apple Silicon" >&2
  exit 1
fi

for command in brew cargo; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "required command not found: $command" >&2
    exit 1
  fi
done

if ! brew list --formula monitoring-plugins >/dev/null 2>&1; then
  brew install monitoring-plugins
fi

cd "$ROOT_DIR"
cargo build --release --locked

sudo install -d -m 0755 "$INSTALL_DIR"
sudo install -m 0755 \
  target/release/mmonitor \
  target/release/check_mmonitor_memory \
  "$INSTALL_DIR/"

if [[ ! -e "$CONFIG_PATH" ]]; then
  brew_prefix="$(brew --prefix)"
  config="$(mktemp)"
  trap 'rm -f "$config"' EXIT
  cat >"$config" <<EOF
[checks.system_disk]
kind = "system_disk"
program = "$brew_prefix/sbin/check_disk"
args = ["-w", "0%", "-c", "0%", "-p", "/"]
timeout_ms = 3000

[checks.cpu_load]
kind = "cpu_load"
program = "$brew_prefix/sbin/check_load"
args = ["-r"]
timeout_ms = 3000

[checks.memory]
kind = "memory"
program = "$INSTALL_DIR/check_mmonitor_memory"
args = []
timeout_ms = 3000
EOF
  sudo install -m 0644 "$config" "$CONFIG_PATH"
else
  echo "preserving existing configuration: $CONFIG_PATH"
fi

"$INSTALL_DIR/mmonitor" --help >/dev/null
"$INSTALL_DIR/check_mmonitor_memory" >/dev/null

echo "installed mmonitor in $INSTALL_DIR"
echo "configuration: $CONFIG_PATH"
