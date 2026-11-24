# `defra.acp`: Access control

- **Parents:** ["defra"]

Access-control behaviour beyond what Go's integration suite checks.

# `defra.acp.nac_restart_enforced`: Stored NAC stays enforced after a restart without the flag

- **Parents:** ["defra.acp"]
- **Scenarios:** ["acp::nac::nac_restart_enforced::a_node_restarted_without_the_nac_flag_still_refuses_an_anonymous_caller"]

A store NAC was enabled on comes back with NAC enforced when the node restarts without `--node-acp-enable`: the owner still sees status `enabled`, an anonymous caller adding a document fails with `not authorized to perform operation. Permission: get-collection`, and the owner's add succeeds. A store NAC was never configured on serves no NAC checks: its status is `not configured` and anonymous callers are not refused (`go.acp.nac.lifecycle.start`).

# `defra.acp.vera_uncovered_takeover_recovery`: An uncovered takeover recovers without a new worker

- **Parents:** ["defra.acp"]
- **Scenarios:** ["external::recovery::tests::a_pass_owes_what_the_deployment_lost_registration_first_and_then_ends", "external::recovery::tests::a_pass_checks_the_records_past_its_first_page", "external::recovery::tests::a_page_not_recorded_is_read_again_and_a_recorded_one_is_not", "external::recovery::tests::a_new_term_starts_the_pass_again", "external::recovery::tests::what_the_outbox_settles_is_recorded_and_owed_again_when_the_deployment_loses_it", "vera_rs::provider::recovery_tests::an_uncovered_journal_recovers_through_the_outbox_under_the_same_worker"]

When a Vera deployment takeover may have voided confirmed submissions older than the journal keeps, the node closes its journal as recoverable and refuses every access decision until a recovery pass finishes. The node keeps a record of every registration, unregistration, grant and revocation its provider confirmed, written in the commit that settles it. The pass walks that record in bounded pages, checks each page against the deployment under the certified term, and owes through the outbox what the deployment lost: a missing registration first, then the grants on it, and a delete for a grant the deployment still holds after the node revoked it. A page is recorded as delivered only after the outbox holds it; a restart resumes after the last recorded page, a new certified term starts the pass again, and the pass ends once the last record has been handed over. The node serves no access decision before then, and a journal closed for any other reason stays closed.

What the pass cannot restore is reported in the `acp::audit` log, never dropped: an object the deployment holds for another owner, a registration the node removed that the deployment holds again, a relationship whose registration was never recorded, a relationship the deployment does not let the pass read, and a registration or grant the provider never answered and the deployment does not hold. A delete the provider never answered is owed when the deployment still holds the grant. Changes other actors made directly on the deployment, and what the provider confirmed before the node kept this record, are not in it and are not restored.

# `defra.acp.vera_takeover_report`: Vera takeover outcomes are published

- **Parents:** ["defra.acp"]
- **Scenarios:** ["term_takeover::a_provider_follows_a_confirmed_takeover_and_sends_again_what_it_voided"]

When a Vera provider follows a takeover that voided submissions it had confirmed, the call that checks the new term publishes one `acp-takeover-reconciled` event on the node's bus, after every voided submission was sent again. The event names the checked term and the cut of that term's certificate, and partitions the submissions into those applied again, those owed again and still pending, and those that could not be restored. Each entry names the DID its request acted as; a policy creation is named by its derived policy ID and its creator. Every unrestorable item carries the reason. A policy creation refused because the policy is there is applied again, not unrestorable. A later call that only delivers what the term left owed publishes nothing more. A check that fails closed publishes the event with `closed` set to the reason and what it had learnt until then, also when it sent nothing again; a node that fails closed later, because it cannot owe what the term left, publishes one more event for that term with `closed` set and the changes still owed. An event says `closed` only once the node has recorded the close: when that record cannot be written, the call that checked the term publishes the event of a checked term, and the next call closes again and publishes the closed event then. `closed` is null in the event of a term that was checked. A C API subscriber and the node's HTTP event stream receive the event as `acp-takeover-reconciled` with `term`, `cut_height`, `cut_block`, `closed`, `applied_again`, `owed_again` and `unrestorable`.

# `go.acp.nac.gates.signature`: Block signature verification is gated

- **Parents:** ["go.acp.nac.gates"]
- **Scenarios:** ["acp::nac::verify_signature::nac_gates_verify_signature_authorized_identity_allow_access", "acp::nac::verify_signature::nac_gates_verify_signature_go_client_authorized_identity_allow_access", "acp::nac::verify_signature::nac_gates_verify_signature_no_identity_not_authorized_error", "acp::nac::verify_signature::nac_gates_verify_signature_wrong_identity_not_authorized_error"]
- **Scope:** "authorized-identity tests run on HTTP and CLI clients (default signer) or Go and C clients (signed documents), one test each"

With NAC enabled, verifying a block signature succeed for the NAC owner and fail without identity or with a wrong identity with `not authorized to perform operation. Permission: verify-signature`.
