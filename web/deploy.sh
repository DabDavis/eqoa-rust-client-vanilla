#!/bin/sh
# Publishes web/dist (build.sh's output) to the page's host, a build at a
# time: the build's directory (v/<build>/, new, so no page names it yet)
# goes first, then the page that names it, then version.json, which tells the
# pages already open that it is out (main.js). A build's files never change
# under its name, so the host serves v/ as immutable (nginx), and only the
# page and version.json are revalidated. The builds of the last two days,
# and the newest three, stay for the pages still running them: a page
# fetches its worker and its worklet a while after it opened. Each file gets
# a gzip beside it, for nginx's gzip_static. EQOA_WEB_DEST is the
# destination in rsync syntax (user@host:/path/to/play/, or a directory).
set -e
cd "$(dirname "$0")/dist"
DEST="${EQOA_WEB_DEST:?set EQOA_WEB_DEST to user@host:/path/ (rsync syntax)}"
DEST="${DEST%/}/"
BUILD="$(sed -n 's/.*"build":"\([0-9a-f]*\)".*/\1/p' version.json 2>/dev/null)"
[ -n "$BUILD" ] && [ -d "v/$BUILD" ] || { echo "no build in web/dist: run web/build.sh" >&2; exit 1; }
for f in "v/$BUILD"/*.js "v/$BUILD"/*.wasm index.html; do gzip -9 -k -f "$f"; done
rsync -aR "v/$BUILD/" "$DEST"
rsync -a index.html index.html.gz "$DEST"
rsync -a version.json "$DEST"
echo "published build $BUILD to $DEST"

# What no page should need any more: a build past two days old and not one
# of the newest three; and, on the same terms, the files of the pages from
# before builds had directories (2026-10-20). Run where the files are.
PRUNE='cd "$1" || exit 1
if [ -d v ]; then
    ls -1t v | tail -n +4 | while read -r b; do
        case "$b" in *[!0-9a-f]* | "") continue ;; esac
        [ "$b" = "$2" ] && continue
        [ -n "$(find "v/$b" -maxdepth 0 -mmin +2880)" ] && rm -rf "v/$b" && echo "removed build $b"
    done
fi
for f in eqoa.js eqoa_bg.wasm main.js unpack.js sound.js; do
    for g in "$f" "$f.gz"; do
        [ -f "$g" ] && [ -n "$(find "$g" -maxdepth 0 -mmin +2880)" ] && rm -f "$g" && echo "removed $g"
    done
done
true'
case "$DEST" in
    *:*) ssh "${DEST%%:*}" sh -s -- "${DEST#*:}" "$BUILD" <<END
$PRUNE
END
    ;;
    *) sh -s -- "$DEST" "$BUILD" <<END
$PRUNE
END
    ;;
esac
