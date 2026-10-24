#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release --locked
python3 tests/http_smoke.py
python3 tests/workspace_smoke.py
python3 tests/platform_smoke.py
