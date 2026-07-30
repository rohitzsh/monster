#!/bin/sh
set -e
cd /app
export RUST_LOG=${RUST_LOG:-info}

exec ./monster-agent
