#!/usr/bin/env bash
set -euo pipefail

echo "==> Running Rust Logic Unit & BDD Spec Tests..."
cd src/rust && cargo test --lib
echo "==> Rust Logic Unit Tests Passed!"
