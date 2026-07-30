#!/bin/bash
set -e -o pipefail
cd "$(dirname "$0")/.."

./build_arm64/version.sh

IMAGE=${IMAGE:-messense/rust-musl-cross:aarch64-musl}
cargo_options=$*

CARGO_HOME=${CARGO_HOME:-$PWD/.cargo}
mkdir -p $CARGO_HOME
cp build_arm64/config.toml $CARGO_HOME

docker run --rm \
    -e CARGO_HOME=/home/rust/cargo \
    -v $PWD:/home/rust/src \
    -v $CARGO_HOME:/home/rust/cargo \
    -w /home/rust/src \
    -u $UID:$GID \
    --entrypoint cargo \
    $IMAGE \
    $cargo_options
