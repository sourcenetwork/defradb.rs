# `defra.acp`: Access control

- **Parents:** ["defra"]

Access-control behaviour beyond what Go's integration suite checks.

# `defra.acp.nac_restart_enforced`: Stored NAC stays enforced after a restart without the flag

- **Parents:** ["defra.acp"]
- **Scenarios:** ["acp::nac::nac_restart_enforced::a_node_restarted_without_the_nac_flag_still_refuses_an_anonymous_caller"]

A store NAC was enabled on comes back with NAC enforced when the node restarts without `--node-acp-enable`: the owner still sees status `enabled`, an anonymous caller adding a document fails with `not authorized to perform operation. Permission: get-collection`, and the owner's add succeeds. A store NAC was never configured on serves no NAC checks: its status is `not configured` and anonymous callers are not refused (`go.acp.nac.lifecycle.start`).

# `defra.acp.vera_delete_archive`: A vera.rs delete retires its object

- **Parents:** ["defra.acp"]
- **Scenarios:** ["embedded_vera::archive::a_writers_delete_archives_the_object_as_its_owner_and_a_reader_may_not_delete", "access::external_archive::an_owed_archive_leaves_its_actor_the_commit_history_and_nothing_else", "access::external_archive::a_permitted_non_owners_delete_commits_and_denies_the_document_to_everyone_at_once", "access::external_archive::a_non_owners_delete_parks_the_archive_until_the_owners_authority_appears", "access::external_archive::an_actor_without_the_delete_permission_is_refused_before_commit", "access::external_archive::a_delete_in_a_transaction_owes_its_archive_under_that_transaction", "access::external_archive::an_owed_delete_of_the_actors_owner_relation_takes_its_commit_history_too", "access::external_archive::an_owed_archive_denies_before_an_owed_registration_or_an_unregistered_object_could_grant"]

On a vera.rs deployment, deleting a document queues its object's archive with the delete. Any actor the policy grants the delete permission may delete, owner or not, and its delete commits; an actor without that permission deletes nothing, and the document remains readable and its object remains owned. The archive is always attributed to the document's owner, never to a deleter that is not the owner: it is signed with the owner's authority when the node holds it (a key the node holds for the owner's DID, or the delegation the owner's own delete carries), and otherwise it is parked as awaiting the owner's authority and sent once the owner hands one in. A delete made in a transaction is owed under that transaction's ID. From the delete's commit until the archive settles, whether it is queued or parked, every read of the document is denied to everyone, the owner, the deleter and former readers alike, also while the document's registration is still owed to its creator and once the deployment no longer has its object, and only an owner that deleted the document itself still reads its commit history, unless a delete of that owner's owner relation is owed too: then nobody reads it. After someone else's delete nobody reads the commit history. After the archive settles, the deployment names no owner and holds no relationship for the object, and the former reader's permission and document reads are denied.

# `defra.acp.vera_uncovered_takeover_recovery`: An uncovered takeover recovers without a new worker

- **Parents:** ["defra.acp"]
- **Scenarios:** ["external::recovery::tests::a_pass_owes_what_the_deployment_lost_registration_first_and_then_ends", "external::recovery::tests::a_pass_checks_the_records_past_its_first_page", "external::recovery::tests::a_page_not_recorded_is_read_again_and_a_recorded_one_is_not", "external::recovery::tests::a_new_term_starts_the_pass_again", "external::recovery::tests::what_the_outbox_settles_is_recorded_and_owed_again_when_the_deployment_loses_it", "vera_rs::provider::recovery_tests::an_uncovered_journal_recovers_through_the_outbox_under_the_same_worker"]

When a Vera deployment takeover may have voided confirmed submissions older than the journal keeps, the node closes its journal as recoverable and refuses every access decision until a recovery pass finishes. The node keeps a record of every registration, unregistration, grant and revocation its provider confirmed, written in the commit that settles it. The pass walks that record in bounded pages, checks each page against the deployment under the certified term, and owes through the outbox what the deployment lost: a missing registration first, then the grants on it, and a delete for a grant the deployment still holds after the node revoked it. A page is recorded as delivered only after the outbox holds it; a restart resumes after the last recorded page, a new certified term starts the pass again, and the pass ends once the last record has been handed over. The node serves no access decision before then, and a journal closed for any other reason stays closed.

What the pass cannot restore is reported in the `acp::audit` log, never dropped: an object the deployment holds for another owner, a registration the node removed that the deployment holds again, a relationship whose registration was never recorded, a relationship the deployment does not let the pass read, and a registration or grant the provider never answered and the deployment does not hold. A delete the provider never answered is owed when the deployment still holds the grant. Changes other actors made directly on the deployment, and what the provider confirmed before the node kept this record, are not in it and are not restored.

# `defra.acp.parent_registration`: Registration under a parent object

- **Parents:** ["defra.acp"]

A policy resource may carry a `create: {parent: <relation>, requires: <permission>}` rule. A document of such a resource is registered only under a parent object: the object of the relation's single type whose ID the document carries in the field its collection names with `@policy(parentField:)`.

# `defra.acp.parent_registration.create`: A create needs the rule's permission on the parent

- **Parents:** ["defra.acp.parent_registration"]
- **Scenarios:** ["acp::dac::parent_registration::a_document_is_registered_only_under_a_parent_its_creator_owns"]

With `create: {parent: parent, requires: owner}` on `jobs` and `Job @policy(resource: "jobs", parentField: "nodeID")`, a create whose `nodeID` names a node the creator owns succeeds, and the creator owns the new document. A create by an identity that does not own that node fails with `parent requires 'owner'`. A create without an identity fails with `parent-authorized registration requires an authenticated creator and ACP provider`. A create whose `nodeID` is absent or empty fails with `missing parent document ID in 'nodeID'`. A refused create stores no document.

# `defra.acp.parent_registration.parent_field`: `parentField` goes with the create rule

- **Parents:** ["defra.acp.parent_registration"]
- **Scenarios:** ["acp::dac::parent_registration::a_collection_names_a_parent_field_exactly_when_its_resource_has_a_create_rule"]

Adding a collection whose `@policy` names a resource with a create rule but no `parentField`, or a `parentField` for a resource without one, fails with `parentField must be supplied exactly when resource '<resource>' declares a create rule`. A `parentField` that is not a field of the collection fails with `parent field '<field>' does not exist on '<collection>'`.

# `defra.acp.parent_registration.immutable`: The parent field of a registered document does not change

- **Parents:** ["defra.acp.parent_registration"]
- **Scenarios:** ["acp::dac::parent_registration::an_update_cannot_move_a_registered_document_to_another_parent"]

An update that writes another value, or null, into the parent field of a document in such a collection fails with `parent field '<field>' of a registered document cannot change` and changes nothing. An update that writes the value the field already holds succeeds, together with its other fields.

# `defra.acp.parent_registration.last_parent`: A registered document keeps a parent link

- **Parents:** ["defra.acp.parent_registration"]
- **Scenarios:** ["acp::dac::parent_registration::a_registered_document_retains_at_least_one_parent_link"]

Local document ACP refuses to delete a registered document's last parent relation with `invalid relation: cannot delete the last parent relation`. The owner can still read the document. Once the owner adds another authorized parent, removing the first link succeeds. Removing the remaining link is refused.

Vera makes the registration parent immutable. A relationship edit cannot remove or replace that parent.

# `defra.acp.shared_transport_restoration`: Restore a branchable shared transport collection

- **Parents:** ["defra.acp"]

A device can restore an existing branchable collection using its shared transport policy. The request authenticates that device and identifies one collection with a matching policy, resource and collection ID. The provider verifies at one certified policy root that the collection has the stated parent Node, the account owns that Node, the device has its publisher grant on its registered Node, and the device is a member of the selected group. An absent grant, an unsettled local change, or inconsistent proofs cannot authorize restoration.

This API uses the Gents household policy vocabulary (`nodes`, `device_groups`, `parent`, `owner`, `transport_publisher` and `member`). The caller supplies the policy and object identities. A caller cannot substitute a different relation for the publisher or membership check.

# `defra.acp.vera_takeover_report`: Vera takeover outcomes are published

- **Parents:** ["defra.acp"]
- **Scenarios:** ["term_takeover::a_provider_follows_a_confirmed_takeover_and_sends_again_what_it_voided"]

When a Vera provider follows a takeover that voided submissions it had confirmed, the call that checks the new term publishes one `acp-takeover-reconciled` event on the node's bus, after every voided submission was sent again. The event names the checked term and the cut of that term's certificate, and partitions the submissions into those applied again, those owed again and still pending, and those that could not be restored. Each entry names the DID its request acted as; a policy creation is named by its derived policy ID and its creator. Every unrestorable item carries the reason. A refused archive is owed again like any other change, never unrestorable, and does not close the node. A policy creation refused because the policy is there is applied again, not unrestorable. A later call that only delivers what the term left owed publishes nothing more. A check that fails closed publishes the event with `closed` set to the reason and what it had learnt until then, also when it sent nothing again; a node that fails closed later, because it cannot owe what the term left, publishes one more event for that term with `closed` set and the changes still owed. An event says `closed` only once the node has recorded the close: when that record cannot be written, the call that checked the term publishes the event of a checked term, and the next call closes again and publishes the closed event then. `closed` is null in the event of a term that was checked. A C API subscriber and the node's HTTP event stream receive the event as `acp-takeover-reconciled` with `term`, `cut_height`, `cut_block`, `closed`, `applied_again`, `owed_again` and `unrestorable`.

# `defra.acp.vera_http_delegation`: An HTTP caller authenticates with its own Vera delegation

- **Parents:** ["defra.acp"]
- **Scenarios:** ["embedded_vera::delegation::a_caller_the_server_holds_no_key_for_creates_documents_under_its_delegation", "embedded_vera::delegation::a_delegation_that_fails_a_check_is_refused"]

A `defradb start` server whose document ACP is Vera (`--document-acp-type vera` or `vera-embedded`) takes a caller's Vera delegation as its credential in `Authorization: Bearer <token>`, as an in-process node does. The token is one the caller signed with the scope `acp:policy` for the worker and deployment the server names under `delegation` in `GET /api/v0/acp/status`. The request then acts as the delegation's issuer, and the server registers documents that caller creates under that delegation without holding the caller's key; the creator owns them and other callers do not see them. A caller presenting only a DefraDB identity token, whose key the server does not hold, has its document created but not registered: the server logs `failed to register ACP document <docID>: no authority to submit ACP changes as <DID>: the node holds no key for it and the request carries no delegation` and sends Vera nothing.

A token that declares itself a Vera delegation and fails a check is refused with 403 `Token verification failed: …` before the request runs, and is never read as an identity token: a signature that does not verify against its issuer, a malformed token, one over 16 KiB, a validity interval that does not hold at the server's time, a scope other than `acp:policy`, a request-bound delegation, or another worker or deployment.

# `go.acp.nac.gates.signature`: Block signature verification is gated

- **Parents:** ["go.acp.nac.gates"]
- **Scenarios:** ["acp::nac::verify_signature::nac_gates_verify_signature_authorized_identity_allow_access", "acp::nac::verify_signature::nac_gates_verify_signature_go_client_authorized_identity_allow_access", "acp::nac::verify_signature::nac_gates_verify_signature_no_identity_not_authorized_error", "acp::nac::verify_signature::nac_gates_verify_signature_wrong_identity_not_authorized_error"]
- **Scope:** "authorized-identity tests run on HTTP and CLI clients (default signer) or Go and C clients (signed documents), one test each"

With NAC enabled, verifying a block signature succeed for the NAC owner and fail without identity or with a wrong identity with `not authorized to perform operation. Permission: verify-signature`.
