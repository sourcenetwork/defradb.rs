#!/usr/bin/env python3
"""Counts testo scenario outcomes per binary for one client, and merges them.

    testo-report.py shard <junit.xml> <client> <known-failures.txt> <out.json>
    testo-report.py summary <dir of shard json files>

A test whose failure output says `unsupported op:` stopped at an action the
client does not implement yet (`Op::run`'s default, or an explicit
`Failure::unsupported`). It is counted as unsupported, not as a failure.

The known-failures file lists `<binary> <test>` lines, `#` starts a comment.
A shard fails when a test fails that is not listed, or when a listed test
passes, so the list shrinks as fixes land.
"""

import json
import os
import re
import sys
import xml.etree.ElementTree as ET
from collections import Counter, defaultdict

UNSUPPORTED = re.compile(r"unsupported op: ([^\n\r]*)")


def summary_file():
    return open(os.environ.get("GITHUB_STEP_SUMMARY", os.devnull), "a")


def known_failures(path):
    known = set()
    if not os.path.exists(path):
        return known
    for line in open(path):
        line = line.split("#", 1)[0].strip()
        if line:
            binary, test = line.split(None, 1)
            known.add((binary, test.strip()))
    return known


def outcome(case):
    if case.find("skipped") is not None:
        return "skipped", None
    if case.find("failure") is None and case.find("error") is None:
        return "pass", None
    text = "\n".join(
        (element.get("message") or "") + "\n" + (element.text or "") for element in case.iter()
    )
    match = UNSUPPORTED.search(text)
    if match:
        return "unsupported", match.group(1).split(" {")[0].strip()[:100]
    return "fail", None


def shard(junit, client, known_path, out):
    known = known_failures(known_path)
    counts = defaultdict(Counter)
    ops = Counter()
    unexpected, fixed, known_seen = [], [], []
    for case in ET.parse(junit).getroot().iter("testcase"):
        binary = case.get("classname", "").split("::")[-1]
        test = case.get("name", "")
        result, op = outcome(case)
        listed = (binary, test) in known
        if op:
            ops[op] += 1
        if result == "fail" and listed:
            result = "known"
            known_seen.append(f"{binary} {test}")
        elif result == "fail":
            unexpected.append(f"{binary} {test}")
        elif listed and result == "pass":
            fixed.append(f"{binary} {test}")
        counts[binary][result] += 1
    report = {
        "client": client,
        "binaries": {binary: dict(c) for binary, c in counts.items()},
        "unsupported_ops": dict(ops),
        "unexpected": unexpected,
        "fixed": fixed,
    }
    json.dump(report, open(out, "w"), indent=1)
    with summary_file() as summary:
        summary.write(f"### testo, {client}\n\n")
        write_table(summary, report["binaries"])
        for title, tests in (
            ("Failing, not in the known-failures list", unexpected),
            ("Listed as known-failing but passed: remove from the list", fixed),
        ):
            if tests:
                summary.write(f"\n**{title}** ({len(tests)})\n\n")
                summary.writelines(f"- `{t}`\n" for t in sorted(tests)[:200])
    for test in unexpected:
        print(f"::error::{client}: {test} failed and is not in {known_path}")
    for test in fixed:
        print(f"::error::{client}: {test} passed; remove it from {known_path}")
    print(f"{client}: {len(unexpected)} unexpected failures, {len(known_seen)} known, {len(fixed)} fixed")
    return 1 if unexpected or fixed else 0


def write_table(summary, binaries):
    columns = ["pass", "fail", "known", "unsupported"]
    summary.write("| binary | " + " | ".join(columns) + " |\n")
    summary.write("|---|" + "---:|" * len(columns) + "\n")
    total = Counter()
    for binary in sorted(binaries):
        row = binaries[binary]
        total.update(row)
        summary.write(f"| {binary} | " + " | ".join(str(row.get(c, 0)) for c in columns) + " |\n")
    summary.write("| **total** | " + " | ".join(f"**{total.get(c, 0)}**" for c in columns) + " |\n")


def merge(directory):
    clients = defaultdict(lambda: {"binaries": defaultdict(Counter), "ops": Counter()})
    for root, _, files in os.walk(directory):
        for name in files:
            if not name.endswith(".json"):
                continue
            report = json.load(open(os.path.join(root, name)))
            merged = clients[report["client"]]
            for binary, counts in report["binaries"].items():
                merged["binaries"][binary].update(counts)
            merged["ops"].update(report["unsupported_ops"])
    with summary_file() as summary:
        summary.write("## testo scenarios per client\n\n")
        summary.write("`fail` is unexpected, `known` is listed in "
                      "`.github/testo/known-failures/<client>.txt`, `unsupported` stopped at an "
                      "action the client does not implement.\n\n")
        for client in ("in-process", "rust-http", "go-http", "cli", "legacy"):
            if client not in clients:
                summary.write(f"### {client}\n\nno report\n\n")
                continue
            summary.write(f"### {client}\n\n")
            write_table(summary, clients[client]["binaries"])
            ops = clients[client]["ops"].most_common(15)
            if ops:
                summary.write("\nMost common unsupported actions:\n\n")
                summary.writelines(f"- {count} × `{op}`\n" for op, count in ops)
            summary.write("\n")
    return 0


if __name__ == "__main__":
    mode = sys.argv[1]
    if mode == "shard":
        sys.exit(shard(*sys.argv[2:6]))
    if mode == "summary":
        sys.exit(merge(sys.argv[2]))
    sys.exit(f"unknown mode {mode}")
