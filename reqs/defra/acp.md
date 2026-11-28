# `defra.acp`: Access control

- **Parents:** ["defra"]

Access-control behaviour beyond what Go's integration suite checks.

# `defra.acp.nac_restart_enforced`: Stored NAC stays enforced after a restart without the flag

- **Parents:** ["defra.acp"]
- **Scenarios:** ["acp::nac::nac_restart_enforced::a_node_restarted_without_the_nac_flag_still_refuses_an_anonymous_caller"]

A store NAC was enabled on comes back with NAC enforced when the node restarts without `--node-acp-enable`: the owner still sees status `enabled`, an anonymous caller adding a document fails with `not authorized to perform operation. Permission: get-collection`, and the owner's add succeeds. A store NAC was never configured on serves no NAC checks: its status is `not configured` and anonymous callers are not refused (`go.acp.nac.lifecycle.start`).

# `defra.acp.vera_delete_archive`: A vera.rs delete retires its object

- **Parents:** ["defra.acp"]
- **Scenarios:** ["embedded_vera::archive::a_writers_delete_archives_the_object_as_its_owner_and_a_reader_may_not_delete", "access::external_archive::an_owed_archive_leaves_its_actor_the_commit_history_and_nothing_else", "access::external_archive::a_permitted_non_owners_delete_commits_and_denies_the_document_to_everyone_at_once", "access::external_archive::a_non_owners_delete_parks_the_archive_until_the_owners_authority_appears", "access::external_archive::an_actor_without_the_delete_permission_is_refused_before_commit", "access::external_archive::a_delete_in_a_transaction_owes_its_archive_under_that_transaction", "access::external_archive::an_owed_delete_of_the_actors_owner_relation_takes_its_commit_history_too", "access::external_archive::an_owed_archive_denies_before_an_owed_registration_or_an_unregistered_object_could_grant", "external::archive_tests::a_non_owners_deletes_leave_the_owners_admission_untouched", "external::archive_tests::a_node_full_of_parked_archives_still_admits_new_changes", "external::archive_tests::a_transactions_deletes_leave_its_own_staging_bound_untouched"]

On a vera.rs deployment, deleting a document queues its object's archive with the delete. Any actor the policy grants the delete permission may delete, owner or not, and its delete commits; an actor without that permission deletes nothing, and the document remains readable and its object remains owned. The archive is always attributed to the document's owner, never to a deleter that is not the owner: it is signed with the owner's authority when the node holds it (a key the node holds for the owner's DID, or the delegation the owner's own delete carries), and otherwise it is parked as awaiting the owner's authority and sent once the owner hands one in. A delete made in a transaction is owed under that transaction's ID. The archive counts against no bound on owed changes, parked or not: neither an actor's unsettled changes, nor the node's, nor the 256 changes one transaction may stage, so deletes never stop the owner, the deleter or anyone else from staging changes of their own, in the deleting transaction or any other. From the delete's commit until the archive settles, whether it is queued or parked, every read of the document is denied to everyone, the owner, the deleter and former readers alike, also while the document's registration is still owed to its creator and once the deployment no longer has its object, and only an owner that deleted the document itself still reads its commit history, unless a delete of that owner's owner relation is owed too: then nobody reads it. After someone else's delete nobody reads the commit history. After the archive settles, the deployment names no owner and holds no relationship for the object, and the former reader's permission and document reads are denied.

# `defra.acp.vera_owner_transfer`: A vera.rs document is given to another owner after its registration

- **Parents:** ["defra.acp"]
- **Scenarios:** ["embedded_vera::transfer::a_transferred_document_is_its_new_owners_on_the_chain_after_its_creators_grant", "embedded_vera::transfer::a_transferred_gated_document_keeps_its_derived_relationship", "transfer_owner_tests::a_transfer_lands_once_after_the_creators_grant_and_hands_the_document_over", "transfer_owner_tests::a_transfer_by_anyone_but_the_owner_is_refused_before_the_commit", "transfer_owner_tests::an_owed_registration_holds_the_transfer_which_lands_once_after_a_restart", "transfer_owner_tests::a_refused_transfer_is_reported_and_leaves_the_document_its_creators", "transfer_owner_tests::a_transfer_whose_answer_was_lost_settles_once_and_is_not_reported_refused", "external::transfer_tests::a_transfer_waits_for_its_objects_earlier_changes_and_later_ones_wait_for_it", "external::transfer_tests::an_owed_transfer_denies_its_old_owner_and_grants_its_new_owner_nothing", "external::transfer_tests::only_the_owner_stages_a_transfer_and_only_to_one_other_actor", "external::transfer_tests::a_refused_transfer_is_kept_and_told_and_denies_until_its_actor_abandons_it", "external::transfer_tests::a_transfer_whose_answer_was_lost_is_settled_and_not_reported_refused", "external::transfer_tests::a_transfer_staged_beside_a_grant_that_commits_first_is_aborted", "external::transfer_tests::a_grant_staged_beside_a_transfer_that_commits_first_is_aborted", "external::transfer_tests::concurrent_grants_on_one_object_do_not_conflict"]
- **Scope:** "crate tests, not testo: the call is an embedder's Rust API (`node.with_transaction(id).acting_for(actor).transfer_dac_object_owner(collection, doc_id, new_owner)`) that no testo client (in-process, rust-http, go-http, cli) reaches, and a testo node takes no Vera deployment. Known divergence from AGENTS.md §8 (nothing remote is fetched inside a transaction's closure): the staging check asks Vera for the owner inside the staging statement, as staging a relationship change does; the answer is advisory, a retried attempt asks again, and Vera checks the owner when the transfer lands"

An embedder gives a document to another owner with `node.with_transaction(id).acting_for(actor).transfer_dac_object_owner(collection, doc_id, new_owner)` inside a node-owned transaction, as the actor it authenticated. On a vera.rs deployment the transfer is owed in that transaction, like a relationship change, and sent after the commit: after the document's registration, after every change on the document staged before it, and before every change on the document staged after it. Changes the owner stages on the document before the transfer, in the same transaction or earlier, reach the deployment while it is still the owner and are applied with its authority; the deployment then names `new_owner`, in the deployment's spelling of the DID, as the owner, and every other relationship on the document stays, the relationships a governed collection's bound fields registered it with included. The transfer takes effect once, also when the node restarts or the deployment is away at the commit, in which case it is sent once both are back.

The transfer is not atomic with the commit. From the commit until the deployment holds the transfer, the previous owner is denied every permission on the document on this node, also while its registration is still owed; a change it stages on the document after the transfer, in the same transaction or a later one, is refused with `permission denied: a revocation owed to the provider denies <DID> on <resource> <doc>`; and the new owner is let in, and may manage the document, only once the deployment names it. After the transfer lands, the previous owner is refused like any actor without a managing relation (`UNAUTHORIZED: not document owner, cannot create actor relationship` where the policy names no manager of the relation) and the new owner manages the document as any owner does.

A transfer and another change on the same document staged in transactions open at the same time cannot both commit: the one that commits second fails with a transaction conflict, and staged again it is ordered after the first, so changes reach the deployment around a transfer in the order their transactions committed; two changes neither of which is a transfer do not conflict this way. Only the owner may give a document away: anyone else is refused before the commit with `UNAUTHORIZED: not document owner, cannot transfer the document's ownership`, a second transfer by the previous owner while one is owed with `permission denied: a change owed to the provider denies <DID> on <resource> <doc>`, and a transfer while the deployment cannot say who owns the document with `cannot check <DID>'s authority to revoke on <doc> while the ACP provider is unavailable; try again`. A new owner that is not a DID, `*` included, is refused with `invalid new owner DID: …`. Giving a document to its owner changes nothing.

When the deployment refuses the transfer, the commit stands and the document stays the owner's on the deployment. The refusal is kept with the owed change, shown by the actor's `owed_acp` as a refused change of kind transfer, reported by `await_owed_acp` as parked, and published as an `acp-owed-change-refused` event with `transfer` set; it is not sent again. The previous owner stays denied on this node while the refusal is kept, and is decided by the deployment again once it abandons the change (`abandon_owed_acp_change`). A transfer the deployment applied whose answer the node lost is sent again; when the deployment refuses the repeat because it already names the new owner, the transfer is settled as applied, never reported refused. An answer that never says whether the transfer was taken is retried at most five times and then kept as refused.

# `defra.acp.local_owner_transfer`: A local document is given to another owner in its transaction

- **Parents:** ["defra.acp"]
- **Scenarios:** ["transfer_owner_tests::a_local_transfer_replaces_the_owner_in_the_transaction"]
- **Scope:** "crate test, not testo: the call is an embedder's Rust API that no testo client reaches"

On a node's own document ACP, `ActingFor::transfer_dac_object_owner` replaces the document's owner tuple with one naming `new_owner` in the transaction, and the change commits or rolls back with the transaction's rows. Once it commits, the new owner holds every permission an owner holds and manages the document's relationships; the previous owner holds none by ownership, and its relationship changes are refused as any non-manager's are and its transfers with `UNAUTHORIZED: not document owner, cannot transfer the document's ownership`. Anyone but the owner is refused with `UNAUTHORIZED: not document owner, cannot transfer the document's ownership`. A rolled-back transfer leaves the owner as it was.

# `defra.acp.vera_uncovered_takeover_recovery`: An uncovered takeover recovers without a new worker

- **Parents:** ["defra.acp"]
- **Scenarios:** ["external::recovery::tests::a_pass_owes_what_the_deployment_lost_registration_first_and_then_ends", "external::recovery::tests::a_pass_checks_the_records_past_its_first_page", "external::recovery::tests::a_page_not_recorded_is_read_again_and_a_recorded_one_is_not", "external::recovery::tests::a_new_term_starts_the_pass_again", "external::recovery::tests::what_the_outbox_settles_is_recorded_and_owed_again_when_the_deployment_loses_it", "vera_rs::provider::recovery_tests::an_uncovered_journal_recovers_through_the_outbox_under_the_same_worker"]

When a Vera deployment takeover may have voided confirmed submissions older than the journal keeps, the node closes its journal as recoverable and refuses every access decision until a recovery pass finishes. The node keeps a record of every registration, unregistration, grant and revocation its provider confirmed, written in the commit that settles it. The pass walks that record in bounded pages, checks each page against the deployment under the certified term, and owes through the outbox what the deployment lost: a missing registration first, then the grants on it, and a delete for a grant the deployment still holds after the node revoked it. A page is recorded as delivered only after the outbox holds it; a restart resumes after the last recorded page, a new certified term starts the pass again, and the pass ends once the last record has been handed over. The node serves no access decision before then, and a journal closed for any other reason stays closed.

What the pass cannot restore is reported in the `acp::audit` log, never dropped: an object the deployment holds for another owner, a registration the node removed that the deployment holds again, a relationship whose registration was never recorded, a relationship the deployment does not let the pass read, and a registration or grant the provider never answered and the deployment does not hold. A delete the provider never answered is owed when the deployment still holds the grant. Changes other actors made directly on the deployment, and what the provider confirmed before the node kept this record, are not in it and are not restored.

# `defra.acp.deleted_history`: A deleted document's history keeps its access control

- **Parents:** ["defra.acp"]

Deleting a document in a collection with a policy keeps its access-control object: its owner and its relationships stay, so its retained history (its `_commits`, its blocks by CID, what P2P serves of it) is read under the same decision as before the delete, as Go keeps them. Only a purge, which removes the document's history from the node, removes the object as well; a truncate keeps it, as Go's does (`go.collection.truncate.acp`).

# `defra.acp.deleted_history.hidden`: A deleted document's history stays hidden from an actor without access

- **Parents:** ["defra.acp.deleted_history"]
- **Scenarios:** ["acp::dac::deleted_history::a_deleted_documents_history_stays_hidden_from_an_actor_without_access", "acp::dac::deleted_history::a_delete_committed_in_a_transaction_keeps_the_history_hidden", "acp::dac::deleted_history::a_delete_committed_in_a_transaction_keeps_the_commits_hidden_for_every_client", "acp::dac::deleted_history::a_deleted_documents_history_stays_hidden_after_peer_replication"]

After the owner deletes a document, alone or in a transaction that commits, an actor with no relationship to it still gets an empty `_commits` for it, and verifying one of its blocks' signatures still fails with `missing permission`, also after the node restarts. The owner still reads its commits, the delete's included, and its blocks.

# `defra.acp.deleted_history.grants`: Grants on a deleted document stay, and revoking one takes effect

- **Parents:** ["defra.acp.deleted_history"]
- **Scenarios:** ["acp::dac::deleted_history::a_reader_keeps_a_deleted_documents_history_until_revoked", "acp::dac::deleted_history::a_reader_keeps_a_deleted_documents_commits_until_revoked_for_every_client"]

An actor granted `reader` before the delete still reads the deleted document's commits and blocks. The owner can still revoke the grant (`recordFound: true`); from then on that actor gets an empty `_commits` and `missing permission` on its blocks.

# `defra.acp.deleted_history.purge`: A purged document added again takes no earlier grant

- **Parents:** ["defra.acp.deleted_history"]
- **Scenarios:** ["acp::dac::deleted_history::a_purged_document_added_again_takes_no_earlier_grant"]

Purging a deleted document removes its access-control object with its history. When the same content is added again, under the same DocID, it is owned by its new creator only: an actor granted `reader` on the purged document sees neither the new document nor its commits.

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
- **Scope:** "crate tests, not testo: a testo node takes no document ACP provider and none of its client types (in-process, rust-http, go-http, cli) can start a server on a Vera deployment, and a Vera delegation exists only there; both tests start the `defradb` binary with `--document-acp-type vera-embedded` and call it over HTTP"

A `defradb start` server whose document ACP is Vera (`--document-acp-type vera` or `vera-embedded`) takes a caller's Vera delegation as its credential in `Authorization: Bearer <token>`, as an in-process node does. The token is one the caller signed with the scope `acp:policy` for the worker and deployment the server names under `delegation` in `GET /api/v0/acp/status`. The request then acts as the delegation's issuer, and the server registers documents that caller creates under that delegation without holding the caller's key; the creator owns them and other callers do not see them. A caller presenting only a DefraDB identity token, whose key the server does not hold, has its document created but not registered: the server logs `failed to register ACP document <docID>: no authority to submit ACP changes as <DID>: the node holds no key for it and the request carries no delegation` and sends Vera nothing.

A token that declares itself a Vera delegation and fails a check is refused with 403 `Token verification failed: …` before the request runs, and is never read as an identity token: a signature that does not verify against its issuer, a malformed token, one over 16 KiB, a validity interval that does not hold at the server's time, a scope other than `acp:policy`, a request-bound delegation, or another worker or deployment.

# `defra.acp.vera_issuer_spelling`: A Vera delegation is issued under any issuer spelling vera.rs takes

- **Parents:** ["defra.acp"]
- **Scenarios:** ["bearer::issuer_spellings::a_secp256k1_signer_issues_under_every_spelling_vera_rs_takes_and_keeps_it", "bearer::issuer_spellings::an_ed25519_or_p256_signer_named_in_base64url_issues_under_its_canonical_did_only", "bearer::issuer_spellings::no_spelling_of_another_key_is_a_signers_issuer", "request_bound::the_chain_executes_a_request_bound_delegation_for_its_command_only"]
- **Scope:** "crate tests and the `vera` integration binary's request-bound delegation test, not testo: only a host embedding DefraDB supplies a `DelegationSigner` of its own, and no testo client takes one or starts a node on a Vera deployment"

A host may name its delegation signer by any `did:key` spelling of the signer's key that `crypto::parse_did_key` decodes. A delegation it issues for a secp256k1 key may name as its issuer any of the four spellings vera.rs takes for that key: the compressed or the uncompressed point, in base58btc (`did:key:z…`) or base64url (`did:key:u…`), whichever spelling names the signer itself. The token carries the issuer exactly as given, and vera.rs verifies it as that key's actor. An Ed25519 or P-256 key is taken by vera.rs only under its one base58btc DID (for P-256, of the compressed point); a signer named by any other spelling of such a key issues under that DID. An issuer that is not a spelling vera.rs takes for the signer's key, including the base64url spelling of an Ed25519 or P-256 key and any spelling of another key, is refused before anything is signed with `issuer DID is not a spelling vera.rs takes for the signing key`.

# `go.acp.nac.gates.signature`: Block signature verification is gated

- **Parents:** ["go.acp.nac.gates"]
- **Scenarios:** ["acp::nac::verify_signature::nac_gates_verify_signature_authorized_identity_allow_access", "acp::nac::verify_signature::nac_gates_verify_signature_go_client_authorized_identity_allow_access", "acp::nac::verify_signature::nac_gates_verify_signature_no_identity_not_authorized_error", "acp::nac::verify_signature::nac_gates_verify_signature_wrong_identity_not_authorized_error"]
- **Scope:** "authorized-identity tests run on HTTP and CLI clients (default signer) or Go and C clients (signed documents), one test each"

With NAC enabled, verifying a block signature succeed for the NAC owner and fail without identity or with a wrong identity with `not authorized to perform operation. Permission: verify-signature`.

# `defra.acp.create_gate`: Policy relations bound to the fields a new document carries

- **Parents:** ["defra.acp"]

A policy resource may say who may add documents to its collection (`collection: { create: <expr> }`), based on relations the new document carries. A collection names those relations by binding single-object relation fields to them with `@policy(relation:)`. A create in a governed collection whose resource sets `create` (not omitted, not `"*"`) is decided before anything is written: a refused create stores no document, no document ID and no index entry, so it takes nothing from a creator the policy admits. `create` holds when, for an arrow `<relation>-><name>`, a relationship the document derives in `<relation>` links an object on which the creator holds `<name>`; unions, intersections and differences combine the arrows, and any other name admits nobody. A refused create fails with `not authorized to create document <docID> in its collection: <identity> may not create <resource>/<docID> in its collection`. A create without an identity in such a collection is refused with `not authorized to create a document in collection <Type> without an identity: its policy restricts who may create documents`. A collection whose resource has no `create`, and a collection without a policy, are unaffected.

# `defra.acp.create_gate.vera`: A create in a gated collection under vera

- **Parents:** ["defra.acp.create_gate"]
- **Scenarios:** ["create_gate::a_gated_collection_admits_creators_its_policy_lets_create"]
- **Scope:** "the `vera` integration suite: a testo node takes no Vera document ACP"

On a node whose document ACP is vera, `create` is decided on vera's certified state before the create's transaction, and a refused create writes nothing (`defra.acp.create_gate`). An admitted document is registered with the relationships its bound fields carry (`<resource>:<doc>#<relation>@<target resource>:<field value>`), and vera checks the resource's `create` over them again and stores them with the owner relationship in one execution. This holds for a single create, every document of a batched create, and the create branch of an upsert; the registration made when the batch commits without its parent leaves the parent-bound registrations intact. Until vera holds the registration of a document in such a collection, nobody reads it, its creator included, also when a takeover voided the registration and it is owed again. A creator admitted by the certified state whose right is revoked before vera checks has its create commit and its registration refused: the node keeps the owed registration as refused with a reason starting `not authorized to create document <docID>`, never sends it again, and nobody reads the document. A document received from a peer is owed as this node's own registration; refused as a create it is sent at most three times and then kept as refused, and it is cleared without being sent once vera holds the document's registration. Once registered, the bound relationship is the owner's to change through the relationship API like any other.

# `defra.acp.create_gate.local`: A create in a gated collection under the local ACP

- **Parents:** ["defra.acp.create_gate"]
- **Scenarios:** ["acp::dac::create_gate::local_gate::a_refused_create_stores_nothing_and_an_admitted_one_takes_its_id", "acp::dac::create_gate::local_gate::an_anonymous_create_in_a_gated_collection_is_refused"]

Under the local ACP engine, `create` is decided in the create's own transaction over the relationships the document's bound fields carry, and an admitted document is registered with them, stored beside its owner relationship, so a permission that follows them (`read: author->publish`) reads through them.

# `defra.acp.create_gate.policy_section`: The `collection` section of a policy resource

- **Parents:** ["defra.acp.create_gate"]
- **Scenarios:** ["acp::dac::create_gate::policy_section::a_policy_with_a_collection_section_is_added", "acp::dac::create_gate::policy_section::an_empty_collection_create_is_refused"]

A resource of a DAC policy may carry a `collection` section with one field, `create`, a string: omitted or `"*"` means anyone may add a document, anything else is an expression acp evaluates. The section gates a governed collection only (`defra.acp.create_gate`): in a collection that is not governed a document is created as it would be without it. A policy without the section is accepted, and gets the same policy ID, as before the section existed. An empty `create` is refused with `collection create of resource '<resource>' must not be empty: omit it or write "*" to let anyone create`; an unknown field in the section is refused as any unknown policy field is.

# `defra.acp.create_gate.binding`: `@policy(relation:)` on a relation field

- **Parents:** ["defra.acp.create_gate"]
- **Scenarios:** ["acp::dac::create_gate::binding::a_binding_on_a_governed_primary_immutable_field_is_accepted", "acp::dac::create_gate::binding::a_binding_outside_a_governed_collection_is_refused", "acp::dac::create_gate::binding::a_binding_on_the_secondary_side_is_refused", "acp::dac::create_gate::binding::a_binding_on_a_mutable_field_is_refused", "acp::dac::create_gate::binding::a_binding_to_a_collection_under_another_policy_is_refused", "acp::dac::create_gate::binding::a_binding_to_a_relation_of_another_type_is_refused", "acp::dac::create_gate::binding::a_binding_on_a_scalar_field_is_refused"]

`author: Author @immutable @policy(relation: "author")` binds the relation field `author` to the policy relation `author`; the binding is kept on the `_authorID` field, which holds the related document's ID. Adding the collection is refused, and nothing is created, unless:

- the field is a single-object relation (`@policy(relation:) requires a single-object relation field. Field: <Type>.<field>`);
- the collection is governed (`invalid @policy(relation:) on field <Type>.<field>: the collection must be governed (@governed(root:))`);
- the field is on the primary side of the relation (`... the binding must be on the primary side of the relation, which stores the related document ID`);
- the field is `@immutable` (`... the field must be @immutable`);
- the collection has a `@policy` (`... the collection must have a @policy`), and the related collection has the same policy ID (`... the related collection <Type> must have the same policy (<policyID>)`);
- the relation is declared on the collection's resource (`... relation does not exist on the resource. PolicyID: <id>, ResourceName: <resource>, Relation: <relation>`), is not `owner`, and its `types` are exactly the related collection's resource (`... relation types must be exactly the related collection's resource. PolicyID: <id>, ResourceName: <resource>, Relation: <relation>, Expected: <resource>`). These two are checked when the node has a DAC ACP, as `@policy` itself is.

On a field, `@policy` without `relation` does nothing, as before; `relation` with `id` or `resource` is refused (`@policy on a field takes only the relation argument; the policy is the collection's. Field: <Type>.<field>`), an empty `relation` is refused (`@policy relation must not be empty. Field: <Type>.<field>`), and `relation` on a type is refused (`@policy(relation:) binds a field and is not allowed on a type`). A governed collection's identity commits to its bindings; an ungoverned collection's identity, and that of any collection without a binding, is unchanged.

# `defra.acp.create_gate.immutable_relation`: `@immutable` on a single-object relation field

- **Parents:** ["defra.acp.create_gate"]
- **Scenarios:** ["acp::dac::create_gate::immutable_relation::an_immutable_relation_cannot_be_changed"]

`@immutable` on a single-object relation field `x` makes its `_xID` field immutable: an update that changes `_xID`, through `_xID` or through `x`, fails with `immutable field '_xID' cannot be changed`. `@immutable` on an array relation is refused as before (`invalid immutable field <field>: only LWW register fields can be immutable`), and on the secondary side of a relation with `invalid immutable field _<field>ID: only the primary side of a relation can be immutable`.

# `defra.acp.create_gate.not_encrypted`: A bound field is never encrypted

- **Parents:** ["defra.acp.create_gate"]
- **Scenarios:** ["acp::dac::create_gate::not_encrypted::encrypting_a_bound_field_is_refused"]

A document create that would encrypt a bound field, the whole document or the field itself, fails with `field <field> is bound to policy relation <relation> and cannot be encrypted`, and nothing is created: the relationship is derived from the field's plaintext. Encryption is chosen per request, so this is checked at create, not when the collection is added.
