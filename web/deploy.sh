#!/bin/sh
# Publishes web/dist (build.sh's output) to the page's host: the files plus a
# gzip of each beside it, for nginx's gzip_static. EQOA_WEB_DEST is the
# destination in rsync syntax (user@host:/path/to/play/).
set -e
cd "$(dirname "$0")/dist"
DEST="${EQOA_WEB_DEST:?set EQOA_WEB_DEST to user@host:/path/ (rsync syntax)}"
for f in eqoa_bg.wasm eqoa.js main.js index.html; do gzip -9 -k -f "$f"; done
rsync -a --delete ./ "$DEST"
echo "published to $DEST"
