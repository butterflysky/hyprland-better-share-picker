#!/usr/bin/env bash
set -euo pipefail

if ! command -v hyprctl >/dev/null 2>&1; then
  echo "error: hyprctl not found" >&2
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "error: python3 not found" >&2
  exit 1
fi

hyprctl clients -j | python3 - <<'PY'
import json, sys

clients = json.load(sys.stdin)
parts = []
for c in clients:
    title = c.get("title", "")
    cls = c.get("class", "")
    addr = c.get("address", "0")
    try:
        handle_lo = int(str(addr), 16) & 0xFFFFFFFF
    except Exception:
        handle_lo = 0
    mapped_id = 0
    parts.append(f"{handle_lo}[HC>]{cls}[HT>]{title}[HE>]{mapped_id}[HA>]")

print("".join(parts))
PY
