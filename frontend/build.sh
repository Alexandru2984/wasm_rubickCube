#!/usr/bin/env bash
# Release build: Trunk (Rust → WASM, wasm-opt -Oz) + post-processing for a
# strict CSP, immutable caching and gzip_static. Output: frontend/dist/
set -euo pipefail
cd "$(dirname "$0")"

# Keep the build machine's paths out of the binary. Panic locations embed
# absolute source paths, which would publish the user name and home layout
# of whoever built it. Stable cargo has no trim-paths yet.
cargo_home="${CARGO_HOME:-$HOME/.cargo}"
sysroot="$(rustc --print sysroot)"
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$cargo_home=/cargo --remap-path-prefix=$sysroot=/rust --remap-path-prefix=$PWD=/cube"

trunk build --release
python3 tools/postbuild.py dist

if strings dist/*_bg.wasm | grep -q "$HOME"; then
    echo "build: local paths leaked into the WASM" >&2
    exit 1
fi
echo
du -sh dist
ls -la dist | sed -n '1,40p'
