#!/usr/bin/env bash
# Print the browser executable shared by setup and browser test recipes.
set -euo pipefail

if command -v firefox >/dev/null 2>&1; then
    command -v firefox
    exit 0
fi

for firefox in \
    "$1/firefox/firefox" \
    "$HOME/Applications/Firefox.app/Contents/MacOS/firefox" \
    "/Applications/Firefox.app/Contents/MacOS/firefox"; do
    if [ -x "$firefox" ]; then
        printf '%s\n' "$firefox"
        exit 0
    fi
done
exit 1
