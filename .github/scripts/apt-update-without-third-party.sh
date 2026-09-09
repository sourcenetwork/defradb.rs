#!/usr/bin/env bash
set -euo pipefail

# Runners carry third-party APT repositories that DefraDB builds never install
# from: NodeSource for operator-managed Node upgrades, Google Chrome for the
# preinstalled browser. When one of those endpoints is stale or unavailable,
# `apt-get update` fails and takes the Ubuntu package refresh down with it.
# Build a request-local source set instead of mutating the runner's global APT
# configuration.
unneeded_hosts=(
  deb.nodesource.com
  dl.google.com
)

grep_args=()
for host in "${unneeded_hosts[@]}"; do
  grep_args+=(-e "${host}")
done

filtered_root="$(mktemp -d)"
trap 'rm -rf "${filtered_root}"' EXIT
mkdir -p "${filtered_root}/sources.list.d"

if [[ -f /etc/apt/sources.list ]]; then
  grep -Fv "${grep_args[@]}" /etc/apt/sources.list \
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
  if grep -Fq "${grep_args[@]}" "${source_file}"; then
    echo "Skipping unneeded third-party APT source: ${source_file}"
    continue
  fi
  cp "${source_file}" "${filtered_root}/sources.list.d/"
done

sudo apt-get "$@" \
  -o "Dir::Etc::sourcelist=${filtered_root}/sources.list" \
  -o "Dir::Etc::sourceparts=${filtered_root}/sources.list.d" \
  update
