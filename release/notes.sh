#!/bin/bash
# The dev release's page: the build, its downloads, and the update notes
# (UPDATE-NOTES.md as checked out, newest first, at most the newest 40
# sections). A section the previous build's notes did not have is this
# build's, and says so (with no notes file then, the newest section is); a
# build that changed something and brought no section has its commits listed
# instead, so the page never passes off an older build's notes as this
# one's. The last line records both builds for notes.yml, which writes the
# page again when only the notes change.
#
#     release/notes.sh <build commit> [<previous build commit>] > notes.md
set -euo pipefail
build=$1
prev=${2:-}
notes=UPDATE-NOTES.md
max=40
repo="https://github.com/${GITHUB_REPOSITORY:-DabDavis/eqoa-rust-client-vanilla}"

headers() { grep '^## ' || true; }

echo "The newest build of \`master\`: $(TZ=UTC git log -1 --format='%h, %cd: %s' --date=format-local:'%Y-%m-%d %H:%M UTC' "$build")."
echo
echo "| | with README and licence | the program alone |"
echo "|---|---|---|"
echo "| Linux (x86-64) | \`eqoa-vanilla-linux-x86_64.tar.gz\` | \`eqoa-vanilla-linux-x86_64\` |"
echo "| Windows (x86-64) | \`eqoa-vanilla-windows-x86_64.zip\` | \`eqoa-vanilla-windows-x86_64.exe\` |"
echo "| macOS (Apple silicon and Intel) | \`eqoa-vanilla-macos.tar.gz\` | \`eqoa-vanilla-macos\` |"
echo
echo "The program alone, on Linux and macOS, needs \`chmod +x\` after the download (a download is not kept runnable)."
echo
echo "You need your own *Frontiers* disc image (SLUS-20744) and an account on a server; README.txt in each archive says how to run it, and the repository's README says more. Each push to \`master\` replaces this build: only the newest can be downloaded, and its notes below say what each build changed."
echo
echo "## Update notes"
echo

# this build's sections: those the previous build's notes did not have
new=""
if [ -n "$prev" ]; then
    if git cat-file -e "$prev:$notes" 2>/dev/null; then
        new=$(headers < "$notes" | grep -vxF -f <(git show "$prev:$notes" | headers; echo) || true)
    else
        new=$(headers < "$notes" | head -n 1)
    fi
    if [ -z "$new" ] && [ -n "$(git log --format=%h "$prev..$build")" ]; then
        echo "### This build: not in the notes yet"
        echo
        git log --format='- %s' "$prev..$build"
        echo
    fi
fi

# A bullet's wrapped lines are joined: the page breaks a line where the file
# does.
awk -v new="$new" -v max="$max" -v more="$repo/blob/master/$notes" '
    function flush() { if (held != "") print held; held = "" }
    BEGIN { n = split(new, a, "\n"); for (i = 1; i <= n; i++) if (a[i] != "") mine[a[i]] = 1 }
    /^## / {
        flush()
        if (++k > max) { cut = 1; exit }
        printf "### %s%s\n", substr($0, 4), ($0 in mine) ? " (this build)" : ""
        on = 1
        next
    }
    !on { next }
    /^$/ { flush(); print ""; next }
    /^  +[^ ]/ && held != "" { sub(/^ +/, ""); held = held " " $0; next }
    { flush(); held = $0 }
    END { flush(); if (cut) printf "\nOlder notes: %s\n", more }
' "$notes"
echo
echo "<!-- notes: build=$(git rev-parse "$build") previous=$prev -->"
