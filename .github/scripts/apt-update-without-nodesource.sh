#!/usr/bin/env bash
set -euo pipefail

# Runners may have third-party repositories configured for preinstalled tools.
# They are not needed by DefraDB builds, and stale or partially propagated
# metadata must not prevent Ubuntu packages from being refreshed. Build a
# request-local source set instead of mutating the runner's global APT
# configuration.
unneeded_sources='deb\.nodesource\.com|dl\.google\.com/linux/chrome'
filtered_root="$(mktemp -d)"
trap 'rm -rf "${filtered_root}"' EXIT
mkdir -p "${filtered_root}/sources.list.d"

if [[ -f /etc/apt/sources.list ]]; then
  grep -Ev "${unneeded_sources}" /etc/apt/sources.list \
    >"${filtered_root}/sources.list" || true
else
  : >"${filtered_root}/sources.list"
fi

shopt -s nullglob
for source_file in /etc/apt/sources.list.d/*; do
  case "${source_file}" in
    *.list | *.sources) ;;
    *) continue ;;
  esac
  if grep -Eq "${unneeded_sources}" "${source_file}"; then
    echo "Skipping unrelated third-party APT source: ${source_file}"
    continue
  fi
  cp "${source_file}" "${filtered_root}/sources.list.d/"
done

sudo apt-get "$@" \
  -o "Dir::Etc::sourcelist=${filtered_root}/sources.list" \
  -o "Dir::Etc::sourceparts=${filtered_root}/sources.list.d" \
  update
