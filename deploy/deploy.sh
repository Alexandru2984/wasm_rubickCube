#!/usr/bin/env bash
# Build + deploy to /var/www/cube.micutu.com (served by nginx with gzip_static).
set -euo pipefail

cd "$(dirname "$0")/frontend"
trunk build --release

# Precompression: nginx (gzip_static) serves the .gz directly, no CPU per request.
find dist -type f \( -name '*.wasm' -o -name '*.js' \) -exec gzip -9 -kf {} \;

DEST=/var/www/cube.micutu.com
rm -f "$DEST"/*
cp dist/* "$DEST"/

echo "Deployed:"
ls -lah "$DEST"
