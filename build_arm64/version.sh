#!/bin/bash
set -e -o pipefail
cd "$(dirname "$0")/.."

VERSION=$(git describe --tags --dirty 2>/dev/null || true)
VERSION="${VERSION#v}"

if [ -z "$VERSION" ]; then
    COMMIT_HASH=$(git rev-parse --short HEAD 2>/dev/null || true)

    if [ -n "$COMMIT_HASH" ]; then
        VERSION="0.0.0-dev.$COMMIT_HASH"
    else
        echo "Warning: Could not determine git version." >&2
        VERSION="0.0.1-dummy"
    fi
fi

echo "$VERSION" > version.txt
echo "Generated version.txt: $VERSION"
