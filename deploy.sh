#!/usr/bin/env bash
# Build + deploy la /var/www/cube.micutu.com (servit de nginx cu gzip_static).
set -euo pipefail

cd "$(dirname "$0")/frontend"
trunk build --release

# Precompresie: nginx (gzip_static) serveste direct .gz-ul, fara CPU per request.
find dist -type f \( -name '*.wasm' -o -name '*.js' \) -exec gzip -9 -kf {} \;

DEST=/var/www/cube.micutu.com
rm -f "$DEST"/*
cp dist/* "$DEST"/

echo "Deployed:"
ls -lah "$DEST"
