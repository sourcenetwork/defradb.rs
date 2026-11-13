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
# Crates built against the unpublished Vera node crates the harness also uses.
NODE_MANIFESTS = ("crates/vera-embedded/Cargo.toml",)
VERA_GIT = 'git = "https://github.com/sourcenetwork/vera.rs"'
UNIT_NAMES = (
    "abi::tests::delete_relationship_subject_userset_round_trips",
    "abi::tests::set_relationship_subject_object_edge_round_trips",
    "bearer::pinned::a_delegation_token_is_these_exact_bytes",
    "bearer::tests::a_scoped_token_keeps_the_issuer_spelling_it_was_given",
    "bearer::tests::bearer_token_uses_vera_rs_es256k_shape",
    "bearer::tests::invalid_lifetime_is_rejected",
    "client::tests::injected_transport_keeps_the_submission_and_receipt_checks",
    "client::tests::rpc_binds_response_and_requires_a_result",
    "client::tests::rpc_enforces_declared_and_streamed_byte_limits",
    "client::tests::stalled_transport_times_out_and_transient_rpc_errors_are_retryable",
    "client::tests::submission_id_must_match_locally_signed_bytes",
    "client::tests::transport_failures_say_whether_the_call_may_be_sent_again",
    "did_spelling::tests::an_actor_vera_rs_names_in_the_compressed_spelling_is_the_same_actor",
    "did_spelling::tests::both_spellings_of_a_secp256k1_key_meet_at_the_uncompressed_did",
    "did_spelling::tests::other_identifiers_pass_through_unchanged",
    "did_spelling::tests::the_uncompressed_spelling_is_the_one_defradb_derives",
    "provider::signer_tests::a_signature_vera_rs_would_refuse_is_never_sent",
    "provider::signer_tests::the_provider_signs_its_delegations_through_its_signer",
    "provider::tests::a_request_token_under_either_issuer_spelling_is_passed_through",
    "provider::tests::resolve_registered_or_passthrough_bearer_token_builds_local_secp256k1_token",
    "provider::tests::resolve_registered_or_passthrough_bearer_token_uses_request_token_for_remote_identity",
    "provider::tests::trusted_consensus_key_is_required_before_connecting",
    "provider::transport_tests::a_node_that_serves_no_header_fails_as_an_unreachable_url_does",
    "provider::transport_tests::an_invalid_trusted_key_is_refused_before_a_transport_is_used",
    "provider::verified::tests::a_decision_the_chain_overtook_is_asked_again",
    "provider::verified::tests::a_read_overtaken_at_every_ask_returns_the_refusal_after_the_last",
    "provider::verified::tests::a_read_the_chain_overtook_is_asked_again_until_answered",
    "provider::verified::tests::any_other_error_is_returned_at_the_first_ask",
    "provider_commands::tests::an_actor_subject_is_sent_under_one_spelling",
    "provider_commands::tests::encodes_register_object_as_vera_rs_policy_cmd",
    "provider_commands::tests::encodes_relationship_subjects_as_vera_rs_policy_cmds",
    "provider_commands::request_bearer_tests::a_remote_identity_acts_with_the_token_its_request_carries",
    "signer::tests::a_signature_is_admitted_exactly_when_vera_rs_would_verify_it",
    "signer::tests::a_signer_must_name_a_secp256k1_key",
    "signer::tests::the_secp256k1_signer_names_its_actor_in_the_uncompressed_spelling",
    "worker::tests::pending_request_and_identity_survive_reopen",
)
UNIT_TESTS = tuple("vera_rs::" + name for name in UNIT_NAMES)
LIVE_CASES = (
    ("policy-lifecycle", "policy_generations::native_protected_collection_keeps_revocations_across_relation_recreation"),
    ("replication-restart", "policy_generations::native_replicated_collection_keeps_revocations_across_relation_recreation_and_restart"),
    ("peer-authorization", "policy_generations::peer_authorization::native_ungranted_peer_is_filtered_until_explicitly_trusted_replay"),
    ("reader-update", "policy_generations::strict_replication::native_replicated_update_rejects_reader_only_signer"),
    ("revoked-delete", "policy_generations::strict_replication::native_replicated_delete_rejects_revoked_writer"),
    ("writer-update", "policy_generations::strict_replication::native_replicated_update_accepts_granted_writer"),
    ("writer-delete", "policy_generations::strict_replication::native_replicated_delete_accepts_granted_writer"),
    ("node-builder", "node_builder::node_built_with_vera_rs_enforces_verified_document_access"),
    ("node-provider", "node_builder::node_built_with_a_host_provider_enforces_verified_document_access"),
    ("actor-spelling", "actor_spelling::either_spelling_of_a_secp256k1_actor_is_one_vera_actor"),
)
CARGO = ["cargo", "+1.99.1"]
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


def git_pins(lines):
    """Return the immutable revision of every Vera git dependency among `lines`."""
    pins = []
    for line in lines:
        if VERA_GIT in line:
            match = re.search(r'rev = "([0-9a-f]{40})"', line)
            if match is None:
                raise ValueError("SDK revision must be immutable")
            pins.append(match[1])
    return pins


def revision(root):
    """Return the pinned Vera harness revision after checking the SDK and node crate declarations."""
    pins, versions = [], []
    for name in SDK_MANIFESTS:
        lines = (root / name).read_text().splitlines()
        pins += git_pins(lines)
        for line in lines:
            match = re.fullmatch(r'vera-(?:client|domain|modules) = "(\d+\.\d+\.\d+)"', line)
            if match is not None:
                versions.append(match[1])
    if len(versions) != 6 or len(set(versions)) != 1:
        raise ValueError("published native SDK declarations disagree or are missing")
    if len(pins) != 1:
        raise ValueError("native harness revision is missing or ambiguous")
    for name in NODE_MANIFESTS:
        if set(git_pins((root / name).read_text().splitlines())) != {pins[0]}:
            raise ValueError("node crates and the native harness pin different Vera revisions")
    return pins[0]


def runtime_revision(value):
    if not isinstance(value, str) or re.fullmatch(r"[0-9a-f]{40}", value) is None:
        raise ValueError("runtime revision must be a full lowercase commit SHA")
    return value


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
    def __init__(self, root, vera, temporary, runtime_ref=None, cache_dir=None):
        self.root, self.vera = root, vera
        self.sdk_revision = revision(root)
        expected = self.sdk_revision if runtime_ref is None else runtime_revision(runtime_ref)
        self.sources = {"defra": source(root), "vera": source(vera)}
        if self.sources["vera"]["revision"] != expected:
            raise ValueError("runtime does not match selected revision")
        self.private = Path(tempfile.mkdtemp(prefix="defra-native-consumer-", dir=temporary))
        self.summary = temporary / "native-consumer-summary.json"
        self.env = clean_environment(os.environ)
        self.cache_dir = cache_dir
        self.target = self.private / "target"
        self.env["CARGO_TARGET_DIR"] = str(self.target)
        self.env.update(VERA_E2E_DIR=str(self.private / "vera-state"), VERA_E2E_KEEP="1", DEFRA_E2E_KEEP="1")
        self.records, self.binaries = [], {}
        self.save()

    def runtime_identity(self):
        return {"version": 1, "source": self.sources["vera"], "rust": "1.99.1",
                "profile": "release", "driver_sha256": digest(Path(__file__))}

    def restore_runtime(self):
        if self.cache_dir is None:
            return False
        directory = self.cache_dir / "vera-runtime"
        manifest = directory / "manifest.json"
        if not manifest.is_file():
            return False
        record = json.loads(manifest.read_text())
        if {k: v for k, v in record.items() if k != "sha256"} != self.runtime_identity():
            return False
        binary = directory / "verad"
        if binary.is_symlink() or not binary.is_file() or digest(binary) != record["sha256"]:
            raise ValueError("cached Vera runtime checksum differs")
        shutil.copy2(binary, self.private / "verad")
        self.binaries["verad"] = {"sha256": record["sha256"]}
        self.env["VERAD_BINARY"] = str(self.private / "verad")
        self.records.append({"phase": "restore-vera", "exit_code": 0})
        self.save()
        return True

    def cache_runtime(self):
        if self.cache_dir is None:
            return
        directory = self.cache_dir / "vera-runtime"
        directory.mkdir(parents=True, exist_ok=True)
        shutil.copy2(self.private / "verad", directory / "verad")
        (directory / "manifest.json").write_text(json.dumps({**self.runtime_identity(),
            "sha256": self.binaries["verad"]["sha256"]}, indent=2) + "\n")


    def guard(self):
        if self.sources != {"defra": source(self.root), "vera": source(self.vera)}:
            raise ValueError("source changed during qualification")
        for name, item in self.binaries.items():
            if digest(self.private / name) != item["sha256"]:
                raise ValueError("staged executable changed")

    def save(self):
        public = [public_phase(record) for record in self.records]
        provenance = {"sources": self.sources, "sdk_revision": self.sdk_revision}
        self.summary.write_text(json.dumps({**provenance, "phases": public}, indent=2) + "\n")
        (self.private / "manifest.json").write_text(json.dumps({**provenance, "binaries": self.binaries,
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
    if not runner.restore_runtime():
        runner.run("fetch-vera", fetch, runner.vera)
        log = runner.run("build-vera", CARGO + ["build", *BUILD, "-p", "verad", "--bin", "verad", "--message-format=json"], runner.vera)
        runner.env["VERAD_BINARY"] = runner.stage(log, "verad", "verad")
        runner.cache_runtime()
        # Only this driver-created temporary target is removed. Defra's cache is separate.
        shutil.rmtree(runner.target)
    if runner.cache_dir is not None:
        runner.target = runner.cache_dir / "defra-target"
        runner.env["CARGO_TARGET_DIR"] = str(runner.target)
    runner.run("fetch-defra", fetch)
    log = runner.run("build-defra", CARGO + ["build", *BUILD, "-p", "cli", "--bin", "defradb", "--features", "vera", "--message-format=json"])
    runner.env["DEFRA_RUST_BINARY"] = runner.stage(log, "defradb", "defradb")
    runner.env["DEFRA_VERA_BINARY"] = runner.env["DEFRA_RUST_BINARY"]
    log = runner.run("compile-provider", CARGO + ["test", *BUILD, "-p", "vera", "--lib", "--no-run", "--message-format=json"])
    provider = runner.stage(log, "vera", "provider-tests")
    log = runner.run("compile-native", CARGO + ["test", *BUILD, "-p", "integration-test", "--test", "verars", "--no-run", "--message-format=json"])
    native = runner.stage(log, "verars", "native-tests")
    runner.test("provider-units", provider, ["vera_rs::"], UNIT_TESTS)
    for phase, selector in LIVE_CASES:
        runner.test(phase, native, [selector, "--exact"], [selector])
    runner.run("lint-provider", CARGO + ["clippy", *BUILD, "-p", "vera", "-p", "cli", "--features", "cli/vera", "--lib", "--tests", "--", "-D", "warnings"])
    runner.run("lint-native", CARGO + ["clippy", *BUILD, "-p", "integration-test", "--test", "verars", "--", "-D", "warnings"])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--vera-ref", action="store_true")
    parser.add_argument("--vera", type=Path)
    parser.add_argument("--cache-dir", type=Path, help="Runner-local cache for compiled dependencies and pinned Vera")
    parser.add_argument("--runtime-ref", type=runtime_revision,
                        help="Vera runtime commit; defaults to the native SDK revision")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    if args.vera_ref:
        sdk = revision(root)
        print(args.runtime_ref if args.runtime_ref is not None else sdk)
        return
    if args.vera is None or not os.environ.get("RUNNER_TEMP"):
        parser.error("--vera and RUNNER_TEMP are required")
    os.umask(0o077)
    signal.signal(signal.SIGTERM, interrupted)
    runner = Runner(root, args.vera.resolve(), Path(os.environ["RUNNER_TEMP"]).resolve(), args.runtime_ref,
                    args.cache_dir.resolve() if args.cache_dir else None)
    try:
        qualify(runner)
        runner.records.append({"phase": "qualification", "exit_code": 0, "passed": len(UNIT_TESTS) + len(LIVE_CASES)})
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
