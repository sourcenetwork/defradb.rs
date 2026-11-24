# `defra.acp`: Access control

- **Parents:** ["defra"]

Access-control behaviour beyond what Go's integration suite checks.

# `defra.acp.nac_restart_enforced`: Stored NAC stays enforced after a restart without the flag

- **Parents:** ["defra.acp"]
- **Scenarios:** ["acp::nac::nac_restart_enforced::a_node_restarted_without_the_nac_flag_still_refuses_an_anonymous_caller"]

A store NAC was enabled on comes back with NAC enforced when the node restarts without `--node-acp-enable`: the owner still sees status `enabled`, an anonymous caller adding a document fails with `not authorized to perform operation. Permission: get-collection`, and the owner's add succeeds. A store NAC was never configured on serves no NAC checks: its status is `not configured` and anonymous callers are not refused (`go.acp.nac.lifecycle.start`).

# `defra.acp.vera_delete_archive`: A vera.rs delete retires its object

- **Parents:** ["defra.acp"]
- **Scenarios:** ["embedded_vera::archive::a_deleted_document_is_archived_on_the_deployment_and_only_its_owner_may_delete_it", "access::external_archive::an_owed_archive_leaves_its_actor_the_commit_history_and_nothing_else", "access::external_archive::a_delete_by_anyone_but_the_owner_is_refused_in_these_words", "access::external_archive::a_delete_in_a_transaction_owes_its_archive_under_that_transaction", "access::external_archive::an_owed_delete_of_the_actors_owner_relation_takes_its_commit_history_too", "access::external_archive::an_owed_archive_denies_before_an_owed_registration_or_an_unregistered_object_could_grant"]

On a vera.rs deployment, deleting a document queues its object's archive with the delete. Only the document's owner may archive: an actor with the delete permission but no owner relation has its delete refused with `permission denied: UNAUTHORIZED: not document owner, cannot archive the deleted document's object`, and the document remains readable and its object remains owned. The owner's delete succeeds, and a delete made in a transaction is owed under that transaction's ID. From the delete's commit until the archive settles, every read of the document is denied to everyone, also while the document's registration is still owed to its creator and once the deployment no longer has its object, and only the owner that deleted it still reads its commit history, unless a delete of that owner's owner relation is owed too: then nobody reads it. After the archive settles, the deployment names no owner and holds no relationship for the object, and the former reader's permission and document reads are denied.

# `defra.acp.vera_takeover_report`: Vera takeover outcomes are published

- **Parents:** ["defra.acp"]
- **Scenarios:** ["term_takeover::a_provider_follows_a_confirmed_takeover_and_sends_again_what_it_voided"]

When a Vera provider follows a takeover that voided submissions it had confirmed, the call that checks the new term publishes one `acp-takeover-reconciled` event on the node's bus, after every voided submission was sent again. The event names the checked term and the cut of that term's certificate, and partitions the submissions into those applied again, those owed again and still pending, and those that could not be restored. Each entry names the DID its request acted as; a policy creation is named by its derived policy ID and its creator. Every unrestorable item carries the reason. A refused archive is owed again like any other change, never unrestorable, and does not close the node. A policy creation refused because the policy is there is applied again, not unrestorable. A later call that only delivers what the term left owed publishes nothing more. A check that fails closed publishes the event with `closed` set to the reason and what it had learnt until then, also when it sent nothing again; a node that fails closed later, because it cannot owe what the term left, publishes one more event for that term with `closed` set and the changes still owed. An event says `closed` only once the node has recorded the close: when that record cannot be written, the call that checked the term publishes the event of a checked term, and the next call closes again and publishes the closed event then. `closed` is null in the event of a term that was checked. A C API subscriber and the node's HTTP event stream receive the event as `acp-takeover-reconciled` with `term`, `cut_height`, `cut_block`, `closed`, `applied_again`, `owed_again` and `unrestorable`.

# `go.acp.nac.gates.signature`: Block signature verification is gated

- **Parents:** ["go.acp.nac.gates"]
- **Scenarios:** ["acp::nac::verify_signature::nac_gates_verify_signature_authorized_identity_allow_access", "acp::nac::verify_signature::nac_gates_verify_signature_go_client_authorized_identity_allow_access", "acp::nac::verify_signature::nac_gates_verify_signature_no_identity_not_authorized_error", "acp::nac::verify_signature::nac_gates_verify_signature_wrong_identity_not_authorized_error"]
- **Scope:** "authorized-identity tests run on HTTP and CLI clients (default signer) or Go and C clients (signed documents), one test each"

With NAC enabled, verifying a block signature succeed for the NAC owner and fail without identity or with a wrong identity with `not authorized to perform operation. Permission: verify-signature`.
