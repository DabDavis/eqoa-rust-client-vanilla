#!/bin/sh
# The browser client: web/dist/ gets the page, eqoa.js and eqoa_bg.wasm
# (wasm-bindgen 0.2.129, the version Cargo.lock pins: `cargo install
# wasm-bindgen-cli --version 0.2.129`). wasm-opt is used when it is on the
# PATH or in ~/emsdk. No game data is in dist/ but the HUD's atlas table
# (built in, see the README): the player picks their own disc image in the page.
set -e
cd "$(dirname "$0")/.."
export TMPDIR="${TMPDIR:-$HOME/.cache/rust-tmp}"
mkdir -p "$TMPDIR"
cargo build --release --target wasm32-unknown-unknown -p eqoa
rm -rf web/dist
mkdir -p web/dist
wasm-bindgen --target web --no-typescript --out-dir web/dist --out-name eqoa target/wasm32-unknown-unknown/release/eqoa.wasm
OPT="$(command -v wasm-opt || true)"
[ -z "$OPT" ] && [ -x "$HOME/emsdk/upstream/bin/wasm-opt" ] && OPT="$HOME/emsdk/upstream/bin/wasm-opt"
if [ -n "$OPT" ]; then
    "$OPT" -O2 --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals \
        --enable-reference-types --enable-multivalue web/dist/eqoa_bg.wasm -o web/dist/eqoa_bg.wasm
fi
cp web/index.html web/main.js web/unpack.js web/dist/
ls -la web/dist
