#!/usr/bin/env python3
"""Qualify the native consumer; raw output and fixture state stay in RUNNER_TEMP."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time

SDK_MANIFESTS = ("crates/vera/Cargo.toml", "tools/integration-test/Cargo.toml")
UNIT_NAMES = (
    "abi::tests::delete_relationship_subject_userset_round_trips",
    "abi::tests::set_relationship_subject_object_edge_round_trips",
    "bearer::tests::bearer_token_uses_vera_rs_es256k_shape",
    "bearer::tests::invalid_lifetime_is_rejected",
    "client::tests::rpc_binds_response_and_requires_a_result",
    "client::tests::rpc_enforces_declared_and_streamed_byte_limits",
    "client::tests::stalled_transport_times_out_and_transient_rpc_errors_are_retryable",
    "client::tests::submission_id_must_match_locally_signed_bytes",
    "provider::tests::resolve_registered_or_passthrough_bearer_token_builds_local_secp256k1_token",
    "provider::tests::resolve_registered_or_passthrough_bearer_token_uses_request_token_for_remote_identity",
    "provider::tests::trusted_consensus_key_is_required_before_connecting",
    "provider_commands::tests::encodes_register_object_as_vera_rs_policy_cmd",
    "provider_commands::tests::encodes_relationship_subjects_as_vera_rs_policy_cmds",
    "worker::tests::pending_request_and_identity_survive_reopen",
)
UNIT_TESTS = tuple("vera_rs::" + name for name in UNIT_NAMES)
LIVE_CASES = (
    ("policy-lifecycle", "native_protected_collection_keeps_revocations_across_relation_recreation"),
    ("replication-restart", "native_replicated_collection_keeps_revocations_across_relation_recreation_and_restart"),
    ("peer-authorization", "peer_authorization::native_ungranted_peer_is_filtered_until_explicitly_trusted_replay"),
    ("reader-update", "strict_replication::native_replicated_update_rejects_reader_only_signer"),
    ("revoked-delete", "strict_replication::native_replicated_delete_rejects_revoked_writer"),
    ("writer-update", "strict_replication::native_replicated_update_accepts_granted_writer"),
    ("writer-delete", "strict_replication::native_replicated_delete_accepts_granted_writer"),
)
CARGO = ["cargo", "+1.98.0"]
BUILD = ["--release", "--frozen", "-j", "2"]
PUBLIC_FIELDS = ("phase", "exit_code", "seconds", "passed", "panic_site")


def public_phase(record):
    return {key: record[key] for key in PUBLIC_FIELDS if key in record}


def panic_site(log):
    pattern = re.compile(r"panicked at (?:[^\n]*[/\\])?([A-Za-z0-9_-]+\.rs):(\d+):(\d+):")
    with log.open() as stream:
        for line in stream:
            match = pattern.search(line)
            if match is not None:
                file, row, column = match.groups()
                return {"file": file, "line": int(row), "column": int(column)}
    return None


def revision(root):
    pins = []
    for name in SDK_MANIFESTS:
        for line in (root / name).read_text().splitlines():
            if 'git = "https://github.com/sourcenetwork/vera.rs"' in line:
                match = re.search(r'rev = "([0-9a-f]{40})"', line)
                if match is None:
                    raise ValueError("SDK revision must be immutable")
                pins.append(match[1])
    if len(pins) != 7 or len(set(pins)) != 1:
        raise ValueError("native SDK declarations disagree or are missing")
    return pins[0]


def clean_environment(environ):
    env = dict(environ)
    exact = {"RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTDOCFLAGS", "CARGO_ENCODED_RUSTDOCFLAGS",
             "RUSTC", "RUSTDOC", "RUSTUP_TOOLCHAIN", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER",
             "CARGO_INCREMENTAL", "CARGO_BUILD_INCREMENTAL", "CARGO_BUILD_TARGET", "CARGO_TARGET_DIR",
             "CARGO_BUILD_RUSTFLAGS", "RUST_LOG", "VERAD_BINARY", "VERA_TRACE_SPANS"}
    for key in list(env):
        if key in exact or key.startswith(("CARGO_PROFILE_", "CARGO_BUILD_RUSTC", "CARGO_BUILD_RUSTDOC",
                                          "DEFRA_", "DEFRADB_", "VERA_E2E_", "ORBIS_LOCAL_STORAGE_")) or (
                key.startswith("CARGO_TARGET_") and key.endswith("_RUSTFLAGS")):
            del env[key]
    # The node harness observes INFO-level HTTP and P2P readiness events.
    env.update(CARGO_BUILD_JOBS="2", CARGO_TERM_COLOR="never", RUST_LOG="info")
    return env


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def artifact(log, name, target):
    found = []
    with log.open() as stream:
        for line in stream:
            try:
                item = json.loads(line)
            except ValueError:
                continue
            if item.get("reason") == "compiler-artifact" and item.get("target", {}).get("name") == name and item.get("executable"):
                if item["profile"]["debug_assertions"]:
                    raise ValueError("non-release artifact")
                path = Path(item["executable"]).resolve()
                path.relative_to(target.resolve())
                found.append(path)
    if len(found) != 1 or not found[0].is_file():
        raise ValueError("expected one executable")
    return found[0]


def passed_tests(log, expected):
    names, summaries = [], []
    with log.open() as stream:
        for line in stream:
            match = re.fullmatch(r"test ([a-zA-Z0-9_:]+) \.\.\. ok\n?", line)
            if match:
                names.append(match[1])
            match = re.match(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;", line)
            if match:
                summaries.append(tuple(map(int, match.groups())))
    if sorted(names) != sorted(expected) or summaries != [(len(expected), 0, 0)]:
        raise ValueError("test selection or result differs")
    return len(names)


def source(root):
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True, stderr=subprocess.PIPE).strip()
    dirty = subprocess.check_output(["git", "status", "--porcelain", "--untracked-files=no"], cwd=root, stderr=subprocess.PIPE)
    if dirty:
        raise ValueError("tracked source changed")
    return {"revision": head, "lock_sha256": digest(root / "Cargo.lock")}


def stop_group(child):
    try:
        os.killpg(child.pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        child.poll()
        try:
            os.killpg(child.pid, 0)
        except ProcessLookupError:
            return
        time.sleep(0.05)
    try:
        os.killpg(child.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    child.wait()


def interrupted(_signal, _frame):
    raise KeyboardInterrupt


class Runner:
    def __init__(self, root, vera, temporary):
        self.root, self.vera = root, vera
        self.private = Path(tempfile.mkdtemp(prefix="defra-native-consumer-", dir=temporary))
        self.summary = temporary / "native-consumer-summary.json"
        self.env = clean_environment(os.environ)
        self.target = self.private / "target"
        self.env["CARGO_TARGET_DIR"] = str(self.target)
        self.env.update(VERA_E2E_DIR=str(self.private / "vera-state"), VERA_E2E_KEEP="1", DEFRA_E2E_KEEP="1")
        self.records, self.binaries = [], {}
        self.sources = {"defra": source(root), "vera": source(vera)}
        if self.sources["vera"]["revision"] != revision(root):
            raise ValueError("runtime does not match native SDK")
        self.save()

    def guard(self):
        if self.sources != {"defra": source(self.root), "vera": source(self.vera)}:
            raise ValueError("source changed during qualification")
        for name, item in self.binaries.items():
            if digest(self.private / name) != item["sha256"]:
                raise ValueError("staged executable changed")

    def save(self):
        public = [public_phase(record) for record in self.records]
        self.summary.write_text(json.dumps({"phases": public}, indent=2) + "\n")
        (self.private / "manifest.json").write_text(json.dumps({"sources": self.sources, "binaries": self.binaries,
                                                              "phases": self.records}, indent=2) + "\n")

    def run(self, phase, args, cwd=None):
        self.guard()
        log = self.private / (phase + ".log")
        record = {"phase": phase, "command": args, "exit_code": None}
        self.records.append(record)
        self.save()
        started = time.monotonic()
        with log.open("x") as stream:
            child = subprocess.Popen(args, cwd=cwd or self.root, env=self.env, stdout=stream,
                                     stderr=subprocess.STDOUT, start_new_session=True)
            try:
                record["exit_code"] = child.wait()
            finally:
                # Signal only this phase's group, including children left by a failed test.
                stop_group(child)
                record["seconds"] = round(time.monotonic() - started, 3)
                self.save()
        if record["exit_code"] != 0:
            site = panic_site(log)
            if site is not None:
                record["panic_site"] = site
                self.save()
        print(json.dumps(public_phase(record)), flush=True)
        if record["exit_code"] != 0:
            raise RuntimeError("qualification phase failed")
        self.guard()
        return log

    def stage(self, log, target_name, name):
        path = artifact(log, target_name, self.target)
        shutil.copy2(path, self.private / name)
        self.binaries[name] = {"sha256": digest(self.private / name)}
        self.save()
        return str(self.private / name)

    def test(self, phase, binary, selectors, expected):
        log = self.run(phase, [binary, *selectors, "--test-threads=1"], cwd=self.private)
        self.records[-1]["passed"] = passed_tests(log, expected)
        self.save()


def qualify(runner):
    # Locked metadata downloads the exact host dependencies before frozen compilation.
    fetch = CARGO + ["metadata", "--locked", "--format-version=1", "--filter-platform=x86_64-unknown-linux-gnu"]
    runner.run("fetch-vera", fetch, runner.vera)
    log = runner.run("build-vera", CARGO + ["build", *BUILD, "-p", "verad", "--bin", "verad", "--message-format=json"], runner.vera)
    runner.env["VERAD_BINARY"] = runner.stage(log, "verad", "verad")
    # This target was created by this driver, never shared with another job.
    shutil.rmtree(runner.target)
    runner.run("fetch-defra", fetch)
    log = runner.run("build-defra", CARGO + ["build", *BUILD, "-p", "cli", "--bin", "defra", "--features", "vera", "--message-format=json"])
    runner.env["DEFRA_RUST_BINARY"] = runner.stage(log, "defra", "defra")
    log = runner.run("compile-provider", CARGO + ["test", *BUILD, "-p", "vera", "--lib", "--no-run", "--message-format=json"])
    provider = runner.stage(log, "vera", "provider-tests")
    log = runner.run("compile-native", CARGO + ["test", *BUILD, "-p", "integration-test", "--test", "verars", "--no-run", "--message-format=json"])
    native = runner.stage(log, "verars", "native-tests")
    runner.test("provider-units", provider, ["vera_rs::"], UNIT_TESTS)
    for phase, suffix in LIVE_CASES:
        selector = "policy_generations::" + suffix
        runner.test(phase, native, [selector, "--exact"], [selector])
    runner.run("lint-provider", CARGO + ["clippy", *BUILD, "-p", "vera", "-p", "cli", "--features", "cli/vera", "--lib", "--tests", "--", "-D", "warnings"])
    runner.run("lint-native", CARGO + ["clippy", *BUILD, "-p", "integration-test", "--test", "verars", "--", "-D", "warnings"])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--vera-ref", action="store_true")
    parser.add_argument("--vera", type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    if args.vera_ref:
        print(revision(root))
        return
    if args.vera is None or not os.environ.get("RUNNER_TEMP"):
        parser.error("--vera and RUNNER_TEMP are required")
    os.umask(0o077)
    signal.signal(signal.SIGTERM, interrupted)
    runner = Runner(root, args.vera.resolve(), Path(os.environ["RUNNER_TEMP"]).resolve())
    try:
        qualify(runner)
        runner.records.append({"phase": "qualification", "exit_code": 0, "passed": 21})
        runner.save()
    except BaseException as error:
        (runner.private / "failure.txt").write_text(repr(error) + "\n")
        runner.records.append({"phase": "qualification", "exit_code": 1})
        runner.save()
        print('{"phase":"qualification","exit_code":1}', flush=True)
        raise SystemExit(1) from None


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.SubprocessError):
        print('{"phase":"preflight","exit_code":1}', flush=True)
        raise SystemExit(1) from None
