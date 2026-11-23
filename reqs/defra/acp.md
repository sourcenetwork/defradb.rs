# `defra.acp`: Access control

- **Parents:** ["defra"]

Access-control behaviour beyond what Go's integration suite checks.

# `defra.acp.nac_restart_enforced`: Stored NAC stays enforced after a restart without the flag

- **Parents:** ["defra.acp"]
- **Scenarios:** ["acp::nac::nac_restart_enforced::a_node_restarted_without_the_nac_flag_still_refuses_an_anonymous_caller"]

A store NAC was enabled on comes back with NAC enforced when the node restarts without `--node-acp-enable`: the owner still sees status `enabled`, an anonymous caller adding a document fails with `not authorized to perform operation. Permission: get-collection`, and the owner's add succeeds. A store NAC was never configured on serves no NAC checks: its status is `not configured` and anonymous callers are not refused (`go.acp.nac.lifecycle.start`).

# `defra.acp.vera_delete_archive`: A vera.rs delete retires its object

- **Parents:** ["defra.acp"]
- **Scenarios:** ["embedded_vera::archive::a_deleted_document_is_archived_on_the_deployment_and_only_its_owner_may_delete_it"]

On a vera.rs deployment, deleting a document queues its object's archive with the delete. Only the document's owner may archive: an actor with the delete permission but no owner relation has its delete refused with an error containing `not document owner`, and the document remains readable and its object remains owned. The owner's delete succeeds; after the archive settles, the deployment names no owner and holds no relationship for the object, and the former reader's permission and document reads are denied.
