#!/bin/sh
# The browser client: web/dist/ gets the page (index.html, and beside it the
# GM panel's preview fxview.html and the community page spellfx.html) and its build,
# v/<build>/: main.js and report.js, the workers (unpack.js, tick.js), the AudioWorklet
# (sound.js), the update notes (update-notes.md), the pages' icon (disc.svg), the app's
# manifest (manifest.json) and icons (tools/webicons: the program's drawn disc as PNGs),
# eqoa.js and eqoa_bg.wasm (wasm-bindgen 0.2.129, the version
# Cargo.lock pins: `cargo install wasm-bindgen-cli --version 0.2.129`), and
# build.json. A build is named by the digest of its files, the page and the
# commit, so its files never change under that name: the host keeps them
# for good, and only the page and version.json (the newest build's name, for
# the pages already open) are asked for again (web/deploy.sh). wasm-opt is
# used when it is on the PATH or in ~/emsdk. No game data is in dist/: the
# player picks their own disc image in the page.
set -e
cd "$(dirname "$0")/.."
export TMPDIR="${TMPDIR:-$HOME/.cache/rust-tmp}"
mkdir -p "$TMPDIR"
cargo build --release --target wasm32-unknown-unknown -p eqoa
rm -rf web/dist
STAGE=web/dist/stage
mkdir -p "$STAGE"
wasm-bindgen --target web --no-typescript --out-dir "$STAGE" --out-name eqoa "${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release/eqoa.wasm"
OPT="$(command -v wasm-opt || true)"
[ -z "$OPT" ] && [ -x "$HOME/emsdk/upstream/bin/wasm-opt" ] && OPT="$HOME/emsdk/upstream/bin/wasm-opt"
if [ -n "$OPT" ]; then
    "$OPT" -O2 --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals \
        --enable-reference-types --enable-multivalue "$STAGE/eqoa_bg.wasm" -o "$STAGE/eqoa_bg.wasm"
fi
cp web/main.js web/report.js web/unpack.js web/sound.js web/tick.js web/fxview.js web/discstore.js web/spellfx.js "$STAGE/"
# the pages' icon: the program's own drawn disc (crates/eqoa/src/icon_art.rs)
cp web/disc.svg "$STAGE/"
# the page as an installed app (main.js "installing"): its manifest, whose start_url and
# scope ("../../") are the page beside v/, and the icons a browser installs it with, the
# same disc drawn as PNGs (192 and 512, a maskable 512, iOS's apple-touch-icon)
cp web/manifest.json "$STAGE/"
cargo run --release -q -p webicons -- "$STAGE"
# the preview again inside the build, for the community page (spellfx.html) to frame on its
# own origin: the page at the top level is framed by the GM panel alone (nginx)
cp web/fxview.html "$STAGE/fxview.html"
# the update notes: an open page of an older build shows the sections this
# build's have and its own lack, with its Reload notice (main.js)
cp UPDATE-NOTES.md "$STAGE/update-notes.md"
# the commit, with a + when tracked files differ from it
COMMIT="$(git rev-parse --short HEAD 2>/dev/null || echo none)"
git diff --quiet HEAD 2>/dev/null || COMMIT="$COMMIT+"
BUILD="$( { (cd "$STAGE" && sha256sum -- *); sha256sum < web/index.html; sha256sum < web/fxview.html; sha256sum < web/spellfx.html; echo "$COMMIT"; } | sha256sum | cut -c1-12)"
printf '{"build":"%s","commit":"%s"}\n' "$BUILD" "$COMMIT" > "$STAGE/build.json"
mkdir -p web/dist/v
mv "$STAGE" "web/dist/v/$BUILD"
sed -e "s|src=\"\./main\.js\"|src=\"./v/$BUILD/main.js\"|" -e "s|src=\"\./report\.js\"|src=\"./v/$BUILD/report.js\"|" -e "s|href=\"\./disc\.svg\"|href=\"./v/$BUILD/disc.svg\"|" \
    -e "s|href=\"\./manifest\.json\"|href=\"./v/$BUILD/manifest.json\"|" -e "s|href=\"\./apple-touch-icon\.png\"|href=\"./v/$BUILD/apple-touch-icon.png\"|" web/index.html > web/dist/index.html
# the GM panel's spell-visual preview (fxview.js, crates/eqoa/src/fxview.rs),
# a page of its own beside the game's, on the same build
sed -e "s|src=\"\./fxview\.js\"|src=\"./v/$BUILD/fxview.js\"|" -e "s|href=\"\./disc\.svg\"|href=\"./v/$BUILD/disc.svg\"|" web/fxview.html > web/dist/fxview.html
# the community page that matches spells to their visuals, on the same build
sed -e "s|src=\"\./spellfx\.js\"|src=\"./v/$BUILD/spellfx.js\"|" -e "s|href=\"\./disc\.svg\"|href=\"./v/$BUILD/disc.svg\"|" web/spellfx.html > web/dist/spellfx.html
# a page that names no build would load files that are not there
grep -q "src=\"./v/$BUILD/main.js\"" web/dist/index.html && grep -q "src=\"./v/$BUILD/report.js\"" web/dist/index.html \
    || { echo "web/index.html: its scripts are not ./main.js and ./report.js" >&2; exit 1; }
grep -q "href=\"./v/$BUILD/manifest.json\"" web/dist/index.html && grep -q "href=\"./v/$BUILD/apple-touch-icon.png\"" web/dist/index.html \
    || { echo "web/index.html: its manifest and Home Screen icon are not ./manifest.json and ./apple-touch-icon.png" >&2; exit 1; }
grep -q "src=\"./v/$BUILD/fxview.js\"" web/dist/fxview.html || { echo "web/fxview.html: its script is not ./fxview.js" >&2; exit 1; }
grep -q "src=\"./v/$BUILD/spellfx.js\"" web/dist/spellfx.html || { echo "web/spellfx.html: its script is not ./spellfx.js" >&2; exit 1; }
for page in index fxview spellfx; do
    grep -q "href=\"./v/$BUILD/disc.svg\"" "web/dist/$page.html" || { echo "web/$page.html: its icon is not ./disc.svg" >&2; exit 1; }
done
cp "web/dist/v/$BUILD/build.json" web/dist/version.json
ls -la "web/dist" "web/dist/v/$BUILD"
