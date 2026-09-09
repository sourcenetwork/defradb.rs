#!/usr/bin/env bash
set -euo pipefail

# Runners carry third-party APT repositories that DefraDB builds never install
# from: NodeSource for operator-managed Node upgrades, Google Chrome for the
# preinstalled browser. When one of those endpoints is stale or unavailable,
# `apt-get update` fails and takes the Ubuntu package refresh with it.
# Build a request-local source set instead of mutating the runner's global APT
# configuration.
#
# A single file can mix an unneeded repository with one we do need, so filter
# individual entries rather than whole files: one-line-per-entry for .list,
# stanza-by-stanza for Deb822 .sources, down to individual URIs within a stanza.
unneeded_hosts=(
  deb.nodesource.com
  dl.google.com
)

hosts_pattern="$(printf '%s\n' "${unneeded_hosts[@]}")"

filter_one_line_per_entry() {
  grep -Fv -f <(printf '%s\n' "${hosts_pattern}") "$1" || true
}

# Deb822 stanzas are separated by blank lines; a field's value may continue on
# following indented lines. Only unneeded hosts are struck from the URIs field,
# and the stanza itself goes only when nothing is left in it. Matching is
# confined to that field, so a comment naming a host does not lose the entry.
filter_deb822_stanzas() {
  awk -v hosts="${hosts_pattern}" -v source_file="$1" '
    BEGIN { host_count = split(hosts, host_list, "\n") }

    function unneeded(uri,   i) {
      for (i = 1; i <= host_count; i++)
        if (host_list[i] != "" && index(uri, host_list[i]) > 0) return 1
      return 0
    }

    function flush(   i, j, token_count, tokens, kept, dropped, printed) {
      if (line_count == 0) return
      token_count = split(uris, tokens, /[[:space:]]+/)
      kept = ""; dropped = ""
      for (i = 1; i <= token_count; i++) {
        if (tokens[i] == "") continue
        if (unneeded(tokens[i])) dropped = dropped " " tokens[i]
        else kept = kept " " tokens[i]
      }
      if (dropped != "")
        printf "Skipping unneeded third-party APT URI in %s:%s\n",
          source_file, dropped > "/dev/stderr"
      if (kept == "") { reset(); return }

      if (emitted) print ""
      printed = 0
      for (j = 1; j <= line_count; j++) {
        if (is_uri_line[j]) {
          if (!printed) { printf "URIs:%s\n", kept; printed = 1 }
          continue
        }
        print stanza[j]
      }
      emitted = 1
      reset()
    }

    function reset(   j) {
      for (j = 1; j <= line_count; j++) is_uri_line[j] = 0
      line_count = 0; uris = ""; in_uris = 0
    }

    /^[[:space:]]*$/ { flush(); next }

    {
      line = $0
      stanza[++line_count] = line
      if (tolower(line) ~ /^uris[[:space:]]*:/) {
        in_uris = 1
        is_uri_line[line_count] = 1
        sub(/^[^:]*:/, "", line)
        uris = uris " " line
      } else if (in_uris && line ~ /^[[:space:]]/) {
        is_uri_line[line_count] = 1
        uris = uris " " line
      } else if (line ~ /^[^[:space:]#]+[[:space:]]*:/) {
        in_uris = 0
      }
    }

    END { flush() }
  ' "$1"
}

filtered_root="$(mktemp -d)"
trap 'rm -rf "${filtered_root}"' EXIT
mkdir -p "${filtered_root}/sources.list.d"

if [[ -f /etc/apt/sources.list ]]; then
  filter_one_line_per_entry /etc/apt/sources.list \
    >"${filtered_root}/sources.list"
else
  : >"${filtered_root}/sources.list"
fi

shopt -s nullglob
for source_file in /etc/apt/sources.list.d/*; do
  destination="${filtered_root}/sources.list.d/$(basename "${source_file}")"
  case "${source_file}" in
    *.list)
      filter_one_line_per_entry "${source_file}" >"${destination}"
      ;;
    *.sources)
      filter_deb822_stanzas "${source_file}" >"${destination}"
      ;;
    *) continue ;;
  esac
done

sudo apt-get "$@" \
  -o "Dir::Etc::sourcelist=${filtered_root}/sources.list" \
  -o "Dir::Etc::sourceparts=${filtered_root}/sources.list.d" \
  update
