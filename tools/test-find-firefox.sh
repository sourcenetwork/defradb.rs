#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/bin" "$work/tooling/firefox" "$work/home with spaces/Applications/Firefox.app/Contents/MacOS"

app="$work/home with spaces/Applications/Firefox.app/Contents/MacOS/firefox"
local_browser="$work/tooling/firefox/firefox"
path_browser="$work/bin/firefox"
for browser in "$app" "$local_browser" "$path_browser"; do
    printf '#!/bin/sh\nexit 0\n' > "$browser"
    chmod +x "$browser"
done

find_browser() {
    HOME="$work/home with spaces" PATH="$work/bin" /bin/bash "$root/tools/find-firefox.sh" "$work/tooling"
}

[[ "$(find_browser)" == "$path_browser" ]]
rm "$path_browser"
[[ "$(find_browser)" == "$local_browser" ]]
rm "$local_browser"
[[ "$(find_browser)" == "$app" ]]
chmod -x "$app"
# A real system bundle may be present on the host running this test.
if [[ -x /Applications/Firefox.app/Contents/MacOS/firefox ]]; then
    [[ "$(find_browser)" == /Applications/Firefox.app/Contents/MacOS/firefox ]]
else
    if find_browser; then
        echo 'unexpected browser found' >&2
        exit 1
    fi
fi
echo 'Firefox discovery tests passed'
