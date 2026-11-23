# `defra.acp`: Access control

- **Parents:** ["defra"]

Access-control behaviour beyond what Go's integration suite checks.

# `defra.acp.nac_restart_enforced`: Stored NAC stays enforced after a restart without the flag

- **Parents:** ["defra.acp"]
- **Scenarios:** ["acp::nac::nac_restart_enforced::a_node_restarted_without_the_nac_flag_still_refuses_an_anonymous_caller"]

A store NAC was enabled on comes back with NAC enforced when the node restarts without `--node-acp-enable`: the owner still sees status `enabled`, an anonymous caller adding a document fails with `not authorized to perform operation. Permission: get-collection`, and the owner's add succeeds. A store NAC was never configured on serves no NAC checks: its status is `not configured` and anonymous callers are not refused (`go.acp.nac.lifecycle.start`).

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
