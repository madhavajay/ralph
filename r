#!/bin/bash
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cargo run --manifest-path "$SCRIPT_DIR/Cargo.toml" -- "$@"
