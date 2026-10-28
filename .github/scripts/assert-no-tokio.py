#!/usr/bin/env python3
"""No workspace crate depends on tokio directly.

nuclei runs every task, timer and socket of ours. A library that needs a
tokio runtime brings tokio itself and is reached through `edge::compat`, the
one place async-compat is used. So, for every workspace package and every
dependency kind (normal, dev, build) on every target:

- tokio, tokio-stream and tokio-util are never a direct dependency;
- n0-future (tokio natively) is a dependency only for wasm32, except in p2p,
  which names iroh-gossip's clock type, `n0_future::time::Instant`;
- async-compat is a dependency of `edge` alone.

deny.toml bans the same for what ships; this covers dev-dependencies too,
which cargo-deny's graph excludes.
"""

import json
import subprocess
import sys

BANNED = {"tokio", "tokio-stream", "tokio-util"}
WASM_ONLY = {"n0-future"}
NATIVE_TYPE_ONLY = {"n0-future": {"p2p"}}
ONLY_IN = {"async-compat": {"edge"}}


def is_wasm_only(target):
    return target is not None and "wasm32" in target and "not(" not in target


def main():
    metadata = json.loads(
        subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--no-deps", "--locked"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout
    )
    problems = []
    for package in metadata["packages"]:
        for dependency in package["dependencies"]:
            name = dependency["name"]
            kind = dependency["kind"] or "normal"
            target = dependency["target"]
            where = f"{package['name']} ({kind}{', ' + target if target else ''})"
            if name in BANNED:
                problems.append(f"{where} depends on {name}: use nuclei")
            elif (
                name in WASM_ONLY
                and not is_wasm_only(target)
                and package["name"] not in NATIVE_TYPE_ONLY.get(name, set())
            ):
                problems.append(f"{where} depends on {name} natively: use nuclei or futures-lite")
            elif name in ONLY_IN and package["name"] not in ONLY_IN[name]:
                problems.append(f"{where} depends on {name}: go through edge::compat")
    if problems:
        print("tokio-free contract broken:", file=sys.stderr)
        for problem in problems:
            print(f"  {problem}", file=sys.stderr)
        sys.exit(1)
    print("ok: no workspace crate depends on tokio")


if __name__ == "__main__":
    main()
