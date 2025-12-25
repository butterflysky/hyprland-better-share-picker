#!/usr/bin/env bash
set -euo pipefail

REPO_URL="https://github.com/hyprwm/hyprland-protocols.git"
TARGET_DIR="third_party/hyprland-protocols"
XML_PATH="protocols/hyprland-toplevel-export-v1.xml"

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <commit-or-tag>" >&2
  exit 2
fi

REF="$1"

mkdir -p "${TARGET_DIR}"

curl -fsSL -o "${TARGET_DIR}/hyprland-toplevel-export-v1.xml" \
  "https://raw.githubusercontent.com/hyprwm/hyprland-protocols/${REF}/${XML_PATH}"

curl -fsSL -o "${TARGET_DIR}/LICENSE" \
  "https://raw.githubusercontent.com/hyprwm/hyprland-protocols/${REF}/LICENSE"

cat <<EOF_SUMMARY
Updated vendored Hyprland protocol XML:
- repo: ${REPO_URL}
- ref:  ${REF}
- file: ${XML_PATH}

Remember to update the commit hash in README.md.
EOF_SUMMARY
