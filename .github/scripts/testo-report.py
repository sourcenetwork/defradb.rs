#!/usr/bin/env python3
"""Counts testo scenario outcomes per binary for one client, and checks the
known-failure lists.

    testo-report.py shard <junit.xml> <client> <known-failures.txt> <out.json>
    testo-report.py summary <dir of shard json files> <known-failures dir>

A test whose failure output says `unsupported op:` stopped at an action the
client does not implement yet (`Op::run`'s default, or an explicit
`Failure::unsupported`). It is counted as unsupported, not as a failure.

Each known-failures line is `<binary> <test> #<issue>`; a line starting with
`#` is a comment. A shard fails when a test fails that is not listed, or when
a listed test passes ("fixed: remove from the list"), so a list only shrinks.
`summary` fails on a line without an issue or whose issue is closed, compares
each list's size with the base branch's copy, and warns on every entry the
pull request adds.
"""

import json
import os
import re
import subprocess
import sys
import xml.etree.ElementTree as ET
from collections import Counter, defaultdict

UNSUPPORTED = re.compile(r"unsupported op: ([^\n\r]*)")
ENTRY = re.compile(r"^(\S+)\s+(\S+)\s+#(\d+)\s*$")
CLIENTS = ("in-process", "rust-http", "go-http", "cli", "legacy")


def summary_file():
    return open(os.environ.get("GITHUB_STEP_SUMMARY", os.devnull), "a")


def parse_known(text, source):
    """`{(binary, test): issue}` and the lines that break the format."""
    known, bad = {}, []
    for number, line in enumerate(text.splitlines(), 1):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        match = ENTRY.match(line)
        if match:
            known[(match.group(1), match.group(2))] = int(match.group(3))
        else:
            bad.append(f"{source}:{number}: expected `<binary> <test> #<issue>`, got `{line}`")
    return known, bad


def read_known(path):
    text = open(path).read() if os.path.exists(path) else ""
    return parse_known(text, path)


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
    known, bad = read_known(known_path)
    for line in bad:
        print(f"::error::{line}")
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
            fixed.append(f"{binary} {test} #{known[(binary, test)]}")
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
        summary.write(f"### {client}\n\n")
        write_table(summary, report["binaries"])
        for title, tests in (
            ("Failing, not in the known-failures list", unexpected),
            ("fixed: remove from the list", fixed),
        ):
            if tests:
                summary.write(f"\n**{title}** ({len(tests)})\n\n")
                summary.writelines(f"- `{t}`\n" for t in sorted(tests)[:200])
    for test in unexpected:
        print(f"::error::{client}: {test} failed and is not in {known_path}")
    for test in fixed:
        print(f"::error::{client}: {test} passed. fixed: remove from the list {known_path}")
    print(f"{client}: {len(unexpected)} unexpected failures, {len(known_seen)} known, {len(fixed)} fixed")
    return 1 if unexpected or fixed or bad else 0


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


def gh(*args):
    return subprocess.run(["gh", *args], capture_output=True, text=True)


def issue_state(issue, cache={}):
    if issue not in cache:
        result = gh("issue", "view", str(issue), "--repo", os.environ["GITHUB_REPOSITORY"],
                    "--json", "state", "--jq", ".state")
        cache[issue] = result.stdout.strip() if result.returncode == 0 else f"unreadable ({result.stderr.strip()})"
    return cache[issue]


def base_known(path):
    """The base branch's copy of a list, from the API; empty when it has none."""
    base = os.environ.get("GITHUB_BASE_REF") or "main"
    result = gh("api", "-H", "Accept: application/vnd.github.raw",
                f"repos/{os.environ['GITHUB_REPOSITORY']}/contents/{path}?ref={base}")
    if result.returncode != 0:
        return {}, base
    return parse_known(result.stdout, f"{base}:{path}")[0], base


def pull_request():
    match = re.match(r"refs/pull/(\d+)/", os.environ.get("GITHUB_REF", ""))
    return f"#{match.group(1)}" if match else os.environ.get("GITHUB_REF_NAME", "this change")


def check_lists(known_dir, summary):
    errors = 0
    summary.write("## Known-failure lists\n\n| list | entries | base | change |\n|---|---:|---:|---:|\n")
    notes = []
    for client in CLIENTS:
        path = os.path.join(known_dir, f"{client}.txt")
        known, bad = read_known(path)
        for line in bad:
            print(f"::error::{line}")
            errors += 1
        for (binary, test), issue in sorted(known.items()):
            state = issue_state(issue)
            if state != "OPEN":
                print(f"::error::{path}: {binary} {test} names #{issue}, which is {state.lower()}")
                errors += 1
        base, ref = base_known(path)
        summary.write(f"| {client} | {len(known)} | {len(base)} | {len(known) - len(base):+d} |\n")
        for (binary, test), issue in sorted(known.items()):
            if (binary, test) not in base:
                note = f"{pull_request()} adds {binary} {test} #{issue} to {path}"
                print(f"::warning::{note}")
                notes.append(note)
    if notes:
        summary.write(f"\n**Added against {ref}** ({len(notes)})\n\n")
        summary.writelines(f"- {note}\n" for note in notes[:300])
    summary.write("\n")
    return errors


def merge(directory, known_dir):
    clients = defaultdict(lambda: {"binaries": defaultdict(Counter), "ops": Counter()})
    for root, _, files in os.walk(directory):
        for name in files:
            if not name.endswith(".json"):
                continue
            report = json.load(open(os.path.join(root, name)))
            if not isinstance(report, dict) or "client" not in report:
                continue
            merged = clients[report["client"]]
            for binary, counts in report["binaries"].items():
                merged["binaries"][binary].update(counts)
            merged["ops"].update(report["unsupported_ops"])
    with summary_file() as summary:
        errors = check_lists(known_dir, summary)
        summary.write("## Outcomes per client\n\n")
        summary.write("`fail` is unexpected, `known` is listed in "
                      "`.github/testo/known-failures/<client>.txt`, `unsupported` stopped at an "
                      "action the client does not implement.\n\n")
        for client in CLIENTS:
            if client not in clients:
                summary.write(f"### {client}\n\nno report\n\n")
                errors += 1
                continue
            summary.write(f"### {client}\n\n")
            write_table(summary, clients[client]["binaries"])
            ops = clients[client]["ops"].most_common(15)
            if ops:
                summary.write("\nMost common unsupported actions:\n\n")
                summary.writelines(f"- {count} × `{op}`\n" for op, count in ops)
            summary.write("\n")
    return 1 if errors else 0


if __name__ == "__main__":
    mode = sys.argv[1]
    if mode == "shard":
        sys.exit(shard(*sys.argv[2:6]))
    if mode == "summary":
        sys.exit(merge(sys.argv[2], sys.argv[3]))
    sys.exit(f"unknown mode {mode}")
