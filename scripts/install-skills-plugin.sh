#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONFIG_DIR="${PM_CONFIG_DIR:-$HOME/.config/pm}"
SRC_DIR="$ROOT_DIR/plugins/commands/skills"
DST_DIR="$CONFIG_DIR/plugins/commands/skills"

if [[ ! -d "$SRC_DIR" ]]; then
  echo "skills plugin source not found: $SRC_DIR" >&2
  exit 1
fi

mkdir -p "$DST_DIR"
cp "$SRC_DIR/plugin.toml" "$DST_DIR/plugin.toml"
cp "$SRC_DIR/main.py" "$DST_DIR/main.py"
chmod +x "$DST_DIR/main.py"

echo "Installed skills plugin to $DST_DIR"
