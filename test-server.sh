#!/bin/bash
set -e -o pipefail
cd "$(dirname "$0")"
cargo build --release --bin monster-server
RUST_LOG=info ./target/release/monster-server