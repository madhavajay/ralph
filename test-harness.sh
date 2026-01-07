#!/usr/bin/env bash
set -euo pipefail

# Run harness integration tests
# Usage:
#   ./test-harness.sh          # Run all harness tests
#   ./test-harness.sh claude   # Run only claude tests
#   ./test-harness.sh codex    # Run only codex tests
#   ./test-harness.sh pi       # Run only pi tests
#   ./test-harness.sh gemini   # Run only gemini tests

HARNESS="${1:-all}"

echo "=== Harness Integration Tests ==="
echo ""

# First check which harnesses are available
echo "Checking harness availability..."
cargo test --test harness_integration test_harness_availability_check -- --nocapture 2>/dev/null || true
echo ""

case "$HARNESS" in
    claude)
        echo "Running Claude tests..."
        cargo test --test harness_integration test_claude -- --ignored --nocapture
        ;;
    codex)
        echo "Running Codex tests..."
        cargo test --test harness_integration test_codex -- --ignored --nocapture
        ;;
    pi)
        echo "Running Pi tests..."
        cargo test --test harness_integration test_pi -- --ignored --nocapture
        ;;
    gemini)
        echo "Running Gemini tests..."
        cargo test --test harness_integration test_gemini -- --ignored --nocapture
        ;;
    all)
        echo "Running all harness tests..."
        cargo test --test harness_integration -- --ignored --nocapture
        ;;
    *)
        echo "Unknown harness: $HARNESS"
        echo "Usage: $0 [claude|codex|pi|gemini|all]"
        exit 1
        ;;
esac

echo ""
echo "=== Done ==="
