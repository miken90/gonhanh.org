#!/usr/bin/env bash
# Wrapper script for Windows release build
# Usage: ./build-release.sh <version>

set -e

if [ -z "$1" ]; then
    echo "Usage: $0 <version>"
    echo "Example: $0 1.5.9"
    exit 1
fi

VERSION="$1"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "Building GoNhanh v$VERSION..."
powershell -ExecutionPolicy Bypass -File "$SCRIPT_DIR/build-release.ps1" -Version "$VERSION"
