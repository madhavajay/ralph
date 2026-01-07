#!/usr/bin/env bash
set -euo pipefail

CRATE_NAME="ralph"
REPO_URL="https://github.com/madhavajay/ralph"

if ! command -v cargo >/dev/null 2>&1; then
  cat <<'EOF'
cargo is required to install ralph.
Install Rust from https://rustup.rs, then re-run this script.
EOF
  exit 1
fi

echo "Installing ${CRATE_NAME}..."

if cargo install --locked "${CRATE_NAME}"; then
  echo "OK: installed via crates.io"
  exit 0
fi

echo "cargo install ${CRATE_NAME} failed, trying GitHub source..."
if cargo install --locked --git "${REPO_URL}" "${CRATE_NAME}"; then
  echo "OK: installed from GitHub source"
  exit 0
fi

echo "Install failed."
exit 1
