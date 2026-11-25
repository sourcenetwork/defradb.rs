#!/usr/bin/env python3
"""Qualify the native consumer; raw output and fixture state stay in RUNNER_TEMP.

Reading the manifests needs Python 3.11+ or the tomli package; so does test-native-consumer.py.
"""
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

try:
    import tomllib
except ModuleNotFoundError:
    try:
        import tomli as tomllib
    except ModuleNotFoundError:
        sys.exit("native-consumer.py needs Python 3.11+ or the tomli package to read Cargo manifests")

SDK_MANIFESTS = ("crates/engine/vera/Cargo.toml", "tools/integration-test/Cargo.toml")
# Crates built against the unpublished Vera node crates the harness also uses.
NODE_MANIFESTS = ("crates/engine/vera-embedded/Cargo.toml",)
# The vera.rs repository on github.com under every scheme, user, port and letter case: Cargo resolves
# those spellings to the same source. Another host is another source. Matching more spellings than
# Cargo equates only adds declarations that must carry the pin.
VERA_URL = re.compile(
    r"(?:[a-z][a-z0-9+.-]*://)?(?:[^@/]*@)?(?:www\.)?github\.com(?::\d+)?[:/]+sourcenetwork/vera\.rs(?:\.git)?/?"
    r"(?:[?#].*)?",
    re.IGNORECASE)
SDK_CRATES = ("vera-client", "vera-domain", "vera-modules")
DEPENDENCY_TABLES = ("dependencies", "dev-dependencies", "build-dependencies")
UNIT_NAMES = (
    "bearer::caller_tests::a_delegation_names_its_signer_only_to_the_worker_and_deployment_it_was_made_for",
    "bearer::caller_tests::a_plain_delegation_outside_its_window_names_the_window_and_the_provider_time",
    "bearer::caller_tests::an_identity_token_is_not_a_delegation",
    "bearer::key_types::a_registered_ed25519_or_p256_identity_gets_a_delegation_vera_rs_verifies",
    "bearer::request::tests::a_binding_made_from_the_reported_audience_is_taken_for_its_command",
    "bearer::request::tests::a_request_delegation_for_another_genesis_is_refused_where_the_genesis_is_known",
    "bearer::request::tests::a_request_delegation_holds_from_the_clock_skew_allowance_before_it_was_issued",
    "bearer::request::tests::a_request_delegation_is_not_a_callers_credential",
    "bearer::request::tests::a_request_delegation_is_taken_for_its_request_and_no_other",
    "bearer::request::tests::a_request_delegation_lasts_no_longer_than_vera_rs_keeps_a_request_open",
    "bearer::request::tests::a_request_delegation_outside_its_window_names_the_window_and_the_provider_time",
    "bearer::request::tests::vera_rs_takes_the_token_as_its_own_builder_makes_it",
    "client::tests::injected_transport_keeps_the_submission_and_receipt_checks",
    "client::tests::rpc_binds_response_and_requires_a_result",
    "client::tests::rpc_enforces_declared_and_streamed_byte_limits",
    "client::tests::stalled_transport_times_out_and_transient_rpc_errors_are_retryable",
    "client::tests::submission_id_must_match_locally_signed_bytes",
    "client::tests::transport_failures_say_whether_the_call_may_be_sent_again",
    "delegation_parity::a_policy_identifier_parses_as_vera_rs_client_parses_it",
    "delegation_parity::the_acp_precompile_is_the_one_vera_rs_client_names",
    "follow::effect::policy_creation_tests::a_lost_policy_creation_names_its_derived_id",
    "follow::effect::tests::a_voided_batch_keeps_every_archive_it_carried_and_counts_the_rest",
    "follow::effect::tests::an_archive_and_other_requests_are_told_apart",
    "follow::effect::tests::registrations_and_relationship_changes_are_read_back_with_their_delegation",
    "follow::journal::tests::a_failed_write_leaves_what_the_journal_reports_as_it_was",
    "follow::journal::tests::a_gap_in_the_sequences_is_refused",
    "follow::journal::tests::a_journal_begun_for_a_worker_that_signed_before_cannot_vouch_for_those_requests",
    "follow::journal::tests::an_uncovered_journal_is_reopened_after_its_own_state_rebuilds_it",
    "follow::journal::tests::entries_survive_a_reopen_and_a_new_worker_starts_with_no_floor",
    "follow::journal::tests::pruning_keeps_the_newest_entries_and_records_the_floor_before_it_removes",
    "follow::journal::tests::the_term_checked_what_is_owed_and_a_closure_are_kept",
    "follow::lost::tests::a_policy_creation_refused_because_the_policy_is_there_is_held_not_lost",
    "follow::lost::tests::a_refused_archive_is_owed_again_and_nothing_is_unrestorable",
    "follow::lost::tests::a_refused_batch_owes_its_archives_and_counts_what_it_could_not_read",
    "follow::lost::tests::a_refused_replay_taints_the_later_ones_on_its_object_only",
    "follow::lost::tests::what_was_owed_before_a_replay_stopped_still_taints",
    "follow::owed::tests::a_lost_change_is_owed_as_its_delegations_issuer_under_the_key_the_node_holds_or_the_delegation",
    "follow::tests::a_checked_term_is_published_with_what_it_leaves_to_owe",
    "follow::tests::a_journal_begun_after_the_worker_signed_cannot_vouch_for_a_void_reaching_back",
    "follow::tests::a_missing_registration_and_grant_are_sent_again_and_what_is_refused_is_owed_in_order",
    "follow::tests::a_read_that_a_term_check_overtook_is_refused",
    "follow::tests::a_refused_revocation_whose_answer_was_lost_is_still_owed_after_a_restart",
    "follow::tests::a_replay_an_earlier_term_left_unfinished_is_sent_again_under_the_next",
    "follow::tests::a_replay_cut_short_still_owes_what_it_learnt_and_taints_its_object",
    "follow::tests::a_revocation_on_a_void_block_that_is_refused_again_is_owed_as_a_delete",
    "follow::tests::a_term_that_voided_nothing_of_this_node_is_checked_at_once",
    "follow::tests::a_void_archive_that_is_refused_again_is_owed_again",
    "follow::tests::a_void_older_than_the_journal_fails_closed_for_good",
    "follow::tests::a_void_within_the_journal_after_pruning_is_checked",
    "follow::tests::every_term_is_a_new_epoch_for_cached_decisions",
    "follow::tests::whatever_names_a_request_sent_again_learns_what_it_came_to",
    "outcome_note::tests::a_note_of_another_version_is_refused",
    "outcome_note::tests::a_noted_outcome_is_revised_to_what_a_replay_came_to",
    "outcome_note::tests::a_recorded_hash_reads_its_own_outcome_and_no_other",
    "outcome_note::tests::a_request_noted_again_keeps_its_latest_outcome_once",
    "outcome_note::tests::an_outcome_of_an_earlier_request_with_the_same_bytes_is_not_the_callers",
    "outcome_note::tests::an_outcome_survives_a_reopen_and_the_oldest_is_pushed_out",
    "provider::audience_tests::a_provider_checks_the_genesis_identity_it_reports",
    "provider::assignments::tests::position_floor_rejects_replays_and_accepts_a_verified_new_term",
    "provider::exact_relationship::tests::a_previous_owners_grant_is_not_issued_by_the_new_owner",
    "provider::exact_relationship::tests::a_proof_at_another_root_answers_nothing",
    "provider::exact_relationship::tests::an_absent_owner_refuses_as_a_registered_document_does",
    "provider::exact_relationship::tests::an_observation_overtaken_at_every_ask_ends_in_the_refusal",
    "provider::exact_relationship::tests::an_overtaken_observation_is_asked_again_at_one_root",
    "provider::exact_relationship::tests::only_a_verified_record_of_the_grant_answers",
    "provider::genesis::tests::a_genesis_identity_is_taken_only_from_a_proven_first_block",
    "provider::owed_request::tests::a_recorded_hash_names_the_request_and_bytes_alone_do_not",
    "provider::owed_request::tests::without_a_hash_the_bytes_name_the_request",
    "provider::reach::tests::a_transport_failure_is_told_from_an_answer",
    "provider::recovery_tests::a_journal_closed_for_good_is_not_recovered",
    "provider::recovery_tests::a_pass_a_second_outbox_joined_leaves_the_term_unrecorded",
    "provider::recovery_tests::an_uncovered_journal_recovers_through_the_outbox_under_the_same_worker",
    "provider::registration_list::tests::a_list_beyond_what_vera_rs_applies_at_once_is_not_built",
    "provider::registration_list::tests::a_list_carries_each_registration_in_order_under_one_token",
    "provider::registration_list::tests::a_policy_identifier_that_names_no_policy_is_a_query_error_as_alone",
    "provider::terms::tests::a_check_that_failed_closed_says_why_even_with_nothing_replayed",
    "provider::terms::tests::a_lost_policy_creation_is_named_with_its_creator",
    "provider::terms::tests::the_event_keeps_what_is_owed_again_and_the_cut_of_its_term",
    "provider::signer_tests::a_signature_vera_rs_would_refuse_is_never_sent",
    "provider::signer_tests::the_drainer_sends_nothing_while_the_provider_is_not_admitted",
    "provider::signer_tests::the_provider_signs_its_delegations_through_its_signer",
    "provider::tests::a_request_token_under_either_issuer_spelling_is_passed_through",
    "provider::tests::resolve_registered_or_passthrough_bearer_token_builds_local_secp256k1_token",
    "provider::tests::resolve_registered_or_passthrough_bearer_token_uses_request_token_for_remote_identity",
    "provider::tests::trusted_consensus_key_is_required_before_connecting",
    "provider::transport_tests::a_node_that_serves_no_header_fails_as_an_unreachable_url_does",
    "provider::transport_tests::an_invalid_trusted_key_is_refused_before_a_transport_is_used",
    "provider::verified::tests::a_decision_the_chain_overtook_is_asked_again",
    "provider::verified::tests::a_late_read_is_told_from_a_wrong_one",
    "provider::verified::tests::a_read_at_a_stale_revision_fails_once_the_patience_is_spent",
    "provider::verified::tests::a_read_at_a_stale_revision_waits_for_the_next_and_is_answered",
    "provider::verified::tests::a_read_overtaken_at_every_ask_returns_the_refusal_after_the_last",
    "provider::verified::tests::a_read_the_chain_overtook_is_asked_again_until_answered",
    "provider::verified::tests::a_read_the_node_answered_late_is_asked_again_until_answered",
    "provider::verified::tests::a_read_the_node_answered_late_once_is_answered_on_the_wall_clock",
    "provider::verified::tests::a_slow_busy_node_spends_patience_early",
    "provider::verified::tests::another_node_error_is_returned_at_the_first_ask",
    "provider::verified::tests::any_other_error_is_returned_at_the_first_ask",
    "provider::verified::tests::busy_retries_of_two_readers_are_spread",
    "provider::verified::tests::busy_retries_stay_within_patience",
    "provider::verified::tests::late_and_overtaken_answers_are_counted_apart",
    "provider_commands::request_bearer_tests::a_remote_identity_acts_with_the_token_its_request_carries",
    "provider_commands::request_bearer_tests::the_node_holds_a_registered_local_key_and_not_a_request_token",
    "receipt_wait::tests::a_header_verified_while_the_request_is_sent_still_ends_the_wait",
    "receipt_wait::tests::a_receipt_is_asked_for_at_the_next_verified_height_and_not_after_a_sleep",
    "receipt_wait::tests::with_no_header_each_wait_ends_at_its_fallback_and_an_unheard_send_is_sent_again",
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
    ("archived-record", "verified_records::archived_owner_record_does_not_authorize_access"),
    ("permission-rules", "verified_records::native_permissions_honor_policy_exclusions_and_cross_object_rules"),
    ("worker-recovery", "native_worker::native_worker_recovers_pending_and_rejected_submissions"),
    ("provider-recovery", "native_worker::native_provider_recovers_pending_before_concurrent_requests"),
    ("in-process-archived-record", "in_process::in_process_archived_owner_record_does_not_authorize_access"),
    ("in-process-permission-rules", "in_process::in_process_permissions_are_proven_and_end_with_the_node"),
    ("in-process-provider-recovery", "in_process::in_process_provider_recovers_pending_before_concurrent_requests"),
    ("in-process-command-kinds", "command_kinds::in_process_every_command_and_subject_kind_is_taken"),
    ("in-process-request-bound", "request_bound::the_chain_executes_a_request_bound_delegation_for_its_command_only"),
    ("in-process-owed-note", "owed_note::a_command_that_resolves_a_journaled_request_with_its_bytes_notes_the_outcome"),
    ("in-process-owed-order", "owed_order::a_grant_still_in_flight_cannot_land_after_the_revocation_that_follows_it"),
    ("smoke", "smoke::rust_verars_smoke"),
    ("policy-administration", "policy_lifecycle::rust_verars_policy_lifecycle"),
    ("compartments", "compartments::rust_verars_compartments"),
    ("p2p-replication", "p2p_acp::rust_verars_p2p_acp"),
)
CARGO = ["cargo", "+1.99.1"]
BUILD = ["--release", "--frozen"]
PUBLIC_FIELDS = ("phase", "exit_code", "seconds", "passed", "panic_site", "error")


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


class Guard(ValueError):
    """A check of this driver that refused to go on, with a fixed message that names no path or value."""


def guard_failure(error):
    """The public name of a guard this driver raised itself.

    Only `Guard` and the driver's own phase failure are named. Anything else comes from a library
    and may carry private text: a decoding error holds the bytes it could not read, and a path
    error holds both paths.
    """
    if type(error) is Guard and len(error.args) == 1 and isinstance(error.args[0], str):
        return {"error": error.args[0]}
    if type(error) is RuntimeError and error.args == ("qualification phase failed",):
        return {"error": error.args[0]}
    return {}


def dependencies(text):
    """Yield the name and specification of every dependency a manifest declares.

    The manifest is parsed as TOML, so a dependency is read the same however it is written: inline, as
    its own table, with dotted or quoted keys. Only Cargo's dependency tables are read: the package's,
    the workspace's and each target's. Features and metadata may name anything.
    """
    manifest = tomllib.loads(text)
    targets = manifest.get("target", {})
    scopes = [manifest, manifest.get("workspace", {}), *(targets.values() if isinstance(targets, dict) else ())]
    for scope in scopes:
        for kind in DEPENDENCY_TABLES:
            table = scope.get(kind, {}) if isinstance(scope, dict) else {}
            yield from table.items() if isinstance(table, dict) else ()


def git_pins(text):
    """Return the immutable revision of every Vera git dependency in a manifest."""
    pins = []
    for _, spec in dependencies(text):
        source = spec.get("git") if isinstance(spec, dict) else None
        if not isinstance(source, str) or VERA_URL.fullmatch(source) is None:
            continue
        pin = spec.get("rev")
        if not isinstance(pin, str) or re.fullmatch(r"[0-9a-f]{40}", pin) is None:
            raise Guard("SDK revision must be immutable")
        pins.append(pin)
    return pins


def sdk_versions(text):
    """Return the version of every published SDK crate a manifest depends on.

    A crate must be declared as `name = "x.y.z"`. Any other form, and the crate under another name
    (`package = "..."`), is refused, not skipped.
    """
    versions = []
    for name, spec in dependencies(text):
        crate = spec.get("package", name) if isinstance(spec, dict) else name
        if crate not in SDK_CRATES:
            continue
        if not isinstance(spec, str) or re.fullmatch(r"\d+\.\d+\.\d+", spec) is None:
            raise Guard("published native SDK declarations must be plain versions")
        versions.append(spec)
    return versions


def revision(root):
    """Return the pinned Vera harness revision after checking the SDK and node crate declarations."""
    pins, versions = [], []
    for name in SDK_MANIFESTS:
        text = (root / name).read_text()
        pins += git_pins(text)
        versions += sdk_versions(text)
    if len(versions) != 6 or len(set(versions)) != 1:
        raise Guard("published native SDK declarations disagree or are missing")
    if len(pins) != 1:
        raise Guard("native harness revision is missing or ambiguous")
    for name in NODE_MANIFESTS:
        if set(git_pins((root / name).read_text())) != {pins[0]}:
            raise Guard("node crates and the native harness pin different Vera revisions")
    return pins[0]


def runtime_revision(value):
    if not isinstance(value, str) or re.fullmatch(r"[0-9a-f]{40}", value) is None:
        raise Guard("runtime revision must be a full lowercase commit SHA")
    return value


def clean_environment(environ):
    env = dict(environ)
    exact = {"RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTDOCFLAGS", "CARGO_ENCODED_RUSTDOCFLAGS",
             "RUSTC", "RUSTDOC", "RUSTUP_TOOLCHAIN", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER",
             "CARGO_INCREMENTAL", "CARGO_BUILD_INCREMENTAL", "CARGO_BUILD_JOBS", "CARGO_BUILD_TARGET", "CARGO_TARGET_DIR",
             "CARGO_BUILD_RUSTFLAGS", "RUST_LOG", "VERAD_BINARY", "VERA_TRACE_SPANS"}
    for key in list(env):
        if key in exact or key.startswith(("CARGO_PROFILE_", "CARGO_BUILD_RUSTC", "CARGO_BUILD_RUSTDOC",
                                          "DEFRA_", "DEFRADB_", "VERA_E2E_", "ORBIS_LOCAL_STORAGE_")) or (
                key.startswith("CARGO_TARGET_") and key.endswith("_RUSTFLAGS")):
            del env[key]
    # The node harness observes INFO-level HTTP and P2P readiness events.
    env.update(CARGO_TERM_COLOR="never", RUST_LOG="info")
    # defra-node otherwise stamps the commit and a per-second build time, so it and every crate
    # above it rebuilt on every run. Nothing in the qualification reads either value.
    env.update(DEFRA_NODE_BUILD_TIME_OVERRIDE="native-consumer", DEFRA_NODE_GIT_HASH_OVERRIDE="native-consumer",
               DEFRA_NODE_GIT_DIRTY_OVERRIDE="")
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
                    raise Guard("non-release artifact")
                path = Path(item["executable"]).resolve()
                path.relative_to(target.resolve())
                found.append(path)
    if len(found) != 1 or not found[0].is_file():
        raise Guard("expected one executable")
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
        raise Guard("test selection or result differs")
    return len(names)


def source(root):
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True, stderr=subprocess.PIPE).strip()
    dirty = subprocess.check_output(["git", "status", "--porcelain", "--untracked-files=no"], cwd=root, stderr=subprocess.PIPE)
    if dirty:
        raise Guard("tracked source changed")
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
            raise Guard("runtime does not match selected revision")
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
            raise Guard("cached Vera runtime checksum differs")
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
            raise Guard("source changed during qualification")
        for name, item in self.binaries.items():
            if digest(self.private / name) != item["sha256"]:
                raise Guard("staged executable changed")

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
        runner.records.append({"phase": "qualification", "exit_code": 1, **guard_failure(error)})
        runner.save()
        print('{"phase":"qualification","exit_code":1}', flush=True)
        raise SystemExit(1) from None


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.SubprocessError):
        print('{"phase":"preflight","exit_code":1}', flush=True)
        raise SystemExit(1) from None
