Derived from Go `tests/integration/acp` at develop commit `307364d9d`.

# `go.acp`: Access control (document ACP and node ACP)

- **Parents:** ["go"]

DefraDB gates documents by per-document access control (DAC) policies and gates node operations by node access control (NAC).

# `go.acp.dac`: Document access control (DAC)

- **Parents:** ["go.acp"]

Collections linked to a DAC policy make documents created with an identity private to their owner and to actors granted relations on them.

# `go.acp.dac.policy`: Adding DAC policies

- **Parents:** ["go.acp.dac"]

`AddDACPolicy` takes a YAML or JSON policy and a creator identity, registers it, and returns a policy ID.

# `go.acp.dac.policy.resources`: Policies with any resource and relation layout are accepted

- **Parents:** ["go.acp.dac.policy"]
- **Go-Tests:** ["acp/dac/add_policy/basic_test.go::TestACP_AddPolicy_BasicYAML_ValidPolicyID", "acp/dac/add_policy/with_empty_relations_test.go::TestACP_AddPolicy_NoRelationsLabel_NoError", "acp/dac/add_policy/with_empty_relations_test.go::TestACP_AddPolicy_EmptyRelations_NoError", "acp/dac/add_policy/with_empty_resource_test.go::TestACP_AddPolicy_OneResourceThatIsEmpty_DoesntError", "acp/dac/add_policy/with_managed_relation_test.go::TestACP_AddPolicy_WithRelationManagingOtherRelation_ValidPolicyID", "acp/dac/add_policy/with_multiple_resources_test.go::TestACP_AddPolicy_MultipleResources_ValidID", "acp/dac/add_policy/with_no_resources_test.go::TestACP_AddPolicy_NoResource_ValidID", "acp/dac/add_policy/with_no_resources_test.go::TestACP_AddPolicy_NoResourceLabel_ValidID", "acp/dac/add_policy/with_unused_relations_test.go::TestACP_AddPolicy_UnusedRelation_ValidID"]

`AddDACPolicy` returns a policy ID for policies with one or many resources, an empty resource, no resources (with or without the `resources` label), no or empty `relations`, unused relations, and relations that manage other relations.

# `go.acp.dac.policy.permissions`: Policies with extra, missing or empty permissions are accepted

- **Parents:** ["go.acp.dac.policy"]
- **Go-Tests:** ["acp/dac/add_policy/with_extra_perms_and_relations_test.go::TestACP_AddPolicy_ExtraPermissionsAndExtraRelations_ValidPolicyID", "acp/dac/add_policy/with_extra_perms_test.go::TestACP_AddPolicy_ExtraPermissions_ValidPolicyID", "acp/dac/add_policy/with_extra_relations_test.go::TestACP_AddPolicy_ExtraRelations_ValidPolicyID", "acp/dac/add_policy/with_no_perms_test.go::TestACP_AddPolicy_NoPermissionsOnlyOwner_ValidID", "acp/dac/add_policy/with_no_perms_test.go::TestACP_AddPolicy_NoPermissionsMultiRelations_ValidID", "acp/dac/add_policy/with_no_perms_test.go::TestACP_AddPolicy_NoPermissionsLabelSingleRelation_ValidID", "acp/dac/add_policy/with_no_perms_test.go::TestACP_AddPolicy_NoPermissionsLabelMultiRelations_ValidID", "acp/dac/add_policy/with_perm_expr_test.go::TestACP_AddPolicy_EmptyExpressionInPermission_PermissionIsAccepted"]

`AddDACPolicy` returns a policy ID for policies with extra permissions and relations beyond the DRI set, with no `permissions` at all (only the implicit owner, or with relations), and with a permission whose `expr` is empty. DRI compliance is only checked when a collection links the policy.

# `go.acp.dac.policy.ids`: Policy IDs are assigned per added policy

- **Parents:** ["go.acp.dac.policy"]
- **Go-Tests:** ["acp/dac/add_policy/with_multi_policies_test.go::TestACP_AddPolicy_AddDuplicatePolicyByOtherCreator_ValidPolicyIDs", "acp/dac/add_policy/with_multi_policies_test.go::TestACP_AddPolicy_AddMultipleDuplicatePolicies_Error", "acp/dac/add_policy/with_multi_policies_test.go::TestACP_AddPolicy_AddMultipleDuplicatePoliciesDifferentFmts_ProducesDifferentIDs", "acp/dac/add_policy/with_multi_policies_test.go::TestACP_AddPolicy_AddMultipleDifferentPolicies_ValidPolicyIDs"]
- **Go-Bug-Suspect:** "TestACP_AddPolicy_AddMultipleDuplicatePolicies_Error is named as an error case but asserts success with a fresh ID; the IDs depend only on insertion order, not on policy content or creator"

Each successful `AddDACPolicy` returns a new policy ID: the first and second policies added to a node get the IDs `1239a044…ab81` and `166758b3…d850` whether the second is a different policy, the same policy re-added by the same or another creator, or the same policy in another format (YAML vs JSON). Re-adding an identical policy is not an error.

# `go.acp.dac.policy.missing-input`: Policy add rejects missing data, creator or name

- **Parents:** ["go.acp.dac.policy"]
- **Go-Tests:** ["acp/dac/add_policy/with_empty_args_test.go::TestACP_AddPolicy_EmptyPolicyData_Error", "acp/dac/add_policy/with_empty_args_test.go::TestACP_AddPolicy_EmptyPolicyCreator_Error", "acp/dac/add_policy/with_empty_args_test.go::TestACP_AddPolicy_EmptyCreatorAndPolicyArgs_Error", "acp/dac/add_policy/with_no_resources_test.go::TestACP_AddPolicy_PolicyWithOnlySpace_NameIsRequired"]
- **Scope:** "the whitespace-only ('name required') test runs with Local DAC only"

`AddDACPolicy` with empty policy data fails with `policy data can not be empty`; without an identity it fails with `policy creator can not be empty` (also when the data is empty too); a policy of only whitespace fails with `name required`.

# `go.acp.dac.policy.invalid`: Malformed policies are rejected

- **Parents:** ["go.acp.dac.policy"]
- **Go-Tests:** ["acp/dac/add_policy/with_extra_perms_test.go::TestACP_AddPolicy_ExtraDuplicatePermissions_Error", "acp/dac/add_policy/with_extra_relations_test.go::TestACP_AddPolicy_ExtraDuplicateRelations_Error", "acp/dac/add_policy/with_multiple_resources_test.go::TestACP_AddPolicy_MultipleResourcesUsingRelationDefinedInOther_Error", "acp/dac/add_policy/with_perm_expr_test.go::TestACP_AddPolicy_PermissionExprWithOwnerInTheEndWithMinus_ErrorsBecauseOwnerIsInExpr", "acp/dac/add_policy/with_perm_invalid_expr_test.go::TestACP_AddPolicy_PermissionExprWithIncorrectSymbol_Error", "acp/dac/add_policy/with_perm_invalid_expr_test.go::TestACP_AddPolicy_PermissionExprReferencingOwner_Error", "acp/dac/add_policy/with_perm_invalid_expr_test.go::TestACP_AddPolicy_ExpressionReferencesUndeclaredRelation_Error"]

`AddDACPolicy` fails for a duplicated permission or relation name (`key "read" already set in map`, `key "joker" already set in map`), a resource using a relation declared in another resource (`resource does not have relation`), a permission expression that references `owner` (`permission cannot reference \`owner\``), an expression with an invalid symbol (`token recognition error`), and an expression naming an undeclared relation (`BAD_INPUT`).

# `go.acp.dac.link`: Linking collections to DAC policies

- **Parents:** ["go.acp.dac"]

A collection declares `@policy(id: ..., resource: ...)` in its SDL; the link is validated against the policy's DRI (read, update, delete permissions) before the collection is created.

# `go.acp.dac.link.accepted`: Collections linking a DRI-compliant resource are created

- **Parents:** ["go.acp.dac.link"]
- **Go-Tests:** ["acp/dac/link_collection/accept_basic_dri_fmts_test.go::TestACP_LinkCollection_BasicYAML_SchemaAccepted", "acp/dac/link_collection/accept_extra_permissions_on_dri_test.go::TestACP_LinkCollection_WithExtraPermsHavingRequiredRelation_AcceptCollection", "acp/dac/link_collection/accept_managed_relation_on_dri_test.go::TestACP_LinkCollection_WithManagedRelation_AcceptCollections", "acp/dac/link_collection/accept_mixed_resources_on_partial_dri_test.go::TestACP_LinkCollection_UseValidResource_AcceptCollection", "acp/dac/link_collection/accept_multi_dris_test.go::TestACP_LinkCollection_AddPolicyTwiceWithValidDRIByDifferentActorsAndUseBoth_AcceptCollection", "acp/dac/link_collection/accept_multi_resources_on_dri_test.go::TestACP_LinkCollection_WithMultipleResources_AcceptCollection", "acp/dac/link_collection/accept_multi_resources_on_dri_test.go::TestACP_LinkCollection_WithMultipleResourcesBothBeingUsed_AcceptCollection", "acp/dac/link_collection/accept_same_resource_on_diff_collections_test.go::TestACP_LinkCollection_UseSameResourceOnDifferentSchemas_AcceptCollections"]

Adding a collection with `@policy(id, resource)` succeeds and the type appears in introspection when the resource has the required `read`, `update` and `delete` permissions, including with extra permissions, managed relations, several resources (one or both used), a policy added twice by different actors, and one resource shared by two collections.

# `go.acp.dac.link.owner-implicit`: A DRI permission expression that leaves out `owner` is accepted

- **Parents:** ["go.acp.dac.link"]
- **Go-Tests:** ["acp/dac/link_collection/accept_permission_with_omitted_owner_authority_test.go::TestACP_LinkCollection_MaliciousOwnerSpecifiedOnUpdatePermissionExprOnDRI_ACPEnforcesAccess", "acp/dac/link_collection/accept_permission_with_omitted_owner_authority_test.go::TestACP_LinkCollection_MaliciousOwnerSpecifiedOnReadPermissionExprOnDRI_ACPEnforcesOwnerAccess", "acp/dac/link_collection/accept_permission_with_omitted_owner_authority_test.go::TestACP_LinkCollection_MaliciousOwnerSpecifiedOnDeletePermissionExprOnDRI_ACPEnforcesOwnerAccess"]

A collection is accepted when its resource's `read`, `update` or `delete` permission expression leaves out `owner` and names only another relation (the Go tests use one called `ownerBad`). The Go tests check only that the collection is created, not what the owner can do afterwards.

# `go.acp.dac.link.bad-arguments`: Collections with missing or mistyped @policy arguments are rejected

- **Parents:** ["go.acp.dac.link"]
- **Go-Tests:** ["acp/dac/link_collection/reject_empty_arg_on_collection_test.go::TestACP_LinkCollection_NoArgWasSpecifiedOnCollection_CollectionRejected", "acp/dac/link_collection/reject_empty_arg_on_collection_test.go::TestACP_LinkCollection_SpecifiedArgsAreEmptyOnCollection_CollectionRejected", "acp/dac/link_collection/reject_invalid_arg_type_on_collection_test.go::TestACP_LinkCollection_InvalidPolicyIDArgTypeWasSpecifiedOnCollection_CollectionRejected", "acp/dac/link_collection/reject_invalid_arg_type_on_collection_test.go::TestACP_LinkCollection_InvalidResourceArgTypeWasSpecifiedOnCollection_CollectionRejected", "acp/dac/link_collection/reject_missing_id_arg_on_collection_test.go::TestACP_LinkCollection_NoPolicyIDWasSpecifiedOnCollection_CollectionRejected", "acp/dac/link_collection/reject_missing_id_arg_on_collection_test.go::TestACP_LinkCollection_SpecifiedPolicyIDArgIsEmptyOnCollection_CollectionRejected", "acp/dac/link_collection/reject_missing_resource_arg_on_collection_test.go::TestACP_LinkCollection_NoResourceWasSpecifiedOnCollection_CollectionRejected", "acp/dac/link_collection/reject_missing_resource_arg_on_collection_test.go::TestACP_LinkCollection_SpecifiedResourceArgIsEmptyOnCollection_CollectionRejected"]

`@policy` with no arguments or empty ones fails with `missing policy arguments, must have both id and resource`; a non-string `id` or `resource` fails with `Argument "id" has invalid value 123` / `Argument "resource" has invalid value 123`; a missing or empty id fails with `policyID must not be empty`; a missing or empty resource fails with `resource name must not be empty`. No collection is created.

# `go.acp.dac.link.bad-policy`: Collections linking an unknown or non-DRI policy are rejected

- **Parents:** ["go.acp.dac.link"]
- **Go-Tests:** ["acp/dac/link_collection/reject_invalid_owner_delete_perm_on_dri_test.go::TestACP_LinkCollection_OwnerMissingRequiredDeletePermissionLabelOnDRI_CollectionRejected", "acp/dac/link_collection/reject_invalid_owner_read_perm_on_dri_test.go::TestACP_LinkCollection_OwnerMissingRequiredReadPermissionLabelOnDRI_CollectionRejected", "acp/dac/link_collection/reject_invalid_owner_update_perm_on_dri_test.go::TestACP_LinkCollection_OwnerMissingRequiredUpdatePermissionLabelOnDRI_CollectionRejected", "acp/dac/link_collection/reject_missing_dri_test.go::TestACP_LinkCollection_WhereNoPolicyWasAdded_CollectionRejected", "acp/dac/link_collection/reject_missing_dri_test.go::TestACP_LinkCollection_WhereAPolicyWasAddedButLinkedPolicyWasNotAdded_CollectionRejected", "acp/dac/link_collection/reject_missing_perms_on_dri_test.go::TestACP_LinkCollection_MissingRequiredReadPermissionOnDRI_CollectionRejected", "acp/dac/link_collection/reject_missing_resource_on_dri_test.go::TestACP_LinkCollection_SpecifiedResourceDoesNotExistOnDRI_CollectionRejected"]

Linking a policy ID that was never added fails with `policyID specified does not exist with acp`; a resource not on the policy fails with `resource does not exist on the specified policy`; a resource missing a required `read`, `update` or `delete` permission fails with `resource is missing required permission on policy.`. No collection is created.

# `go.acp.dac.ownership`: Document ownership

- **Parents:** ["go.acp.dac"]

A document created with an identity is private to that identity; one created without identity is public.

# `go.acp.dac.ownership.read`: Only the owner reads a private document

- **Parents:** ["go.acp.dac.ownership"]
- **Go-Tests:** ["acp/dac/register_and_read_test.go::TestACP_AddWithoutIdentityAndReadWithoutIdentity_CanRead", "acp/dac/register_and_read_test.go::TestACP_AddWithoutIdentityAndReadWithIdentity_CanRead", "acp/dac/register_and_read_test.go::TestACP_AddWithIdentityAndReadWithIdentity_CanRead", "acp/dac/register_and_read_test.go::TestACP_AddWithIdentityAndReadWithoutIdentity_CanNotRead", "acp/dac/register_and_read_test.go::TestACP_AddWithIdentityAndReadWithWrongIdentity_CanNotRead"]

A document added without identity is returned to queries with or without identity. A document added with identity 1 is returned to identity 1 but not to an anonymous query or identity 2 (empty result, no error).

# `go.acp.dac.ownership.update`: Only the owner updates a private document

- **Parents:** ["go.acp.dac.ownership"]
- **Go-Tests:** ["acp/dac/register_and_update_test.go::TestACP_AddWithoutIdentityAndUpdateWithoutIdentity_CanUpdate", "acp/dac/register_and_update_test.go::TestACP_AddWithoutIdentityAndUpdateWithIdentity_CanUpdate", "acp/dac/register_and_update_test.go::TestACP_AddWithIdentityAndUpdateWithIdentity_CanUpdate", "acp/dac/register_and_update_test.go::TestACP_AddWithIdentityAndUpdateWithoutIdentity_CanNotUpdate", "acp/dac/register_and_update_test.go::TestACP_AddWithIdentityAndUpdateWithWrongIdentity_CanNotUpdate"]
- **Scope:** "the denial tests run with the collection-named mutation type only"

Public documents can be updated with or without identity. Updating a private document via the collection API with no identity or a wrong identity fails with `document not found or not authorized to access` and the document is unchanged; the owner can update it.

# `go.acp.dac.ownership.update-gql`: GraphQL update of a private document by a non-owner is a silent no-op

- **Parents:** ["go.acp.dac.ownership"]
- **Go-Tests:** ["acp/dac/register_and_update_test.go::TestACP_AddWithIdentityAndUpdateWithoutIdentityGQL_CanNotUpdate", "acp/dac/register_and_update_test.go::TestACP_AddWithIdentityAndUpdateWithWrongIdentityGQL_CanNotUpdate"]
- **Scope:** "GraphQL request mutation type only"
- **Go-Bug-Suspect:** "the collection API returns 'document not found or not authorized to access' for the same request; the test comments say the GQL path should be merged once clients behave the same (Go issue #2410)"

An `update_Users` GraphQL mutation on a private document with no identity or a wrong identity returns no error, and a subsequent owner query shows the document unchanged.

# `go.acp.dac.ownership.delete`: Only the owner deletes a private document

- **Parents:** ["go.acp.dac.ownership"]
- **Go-Tests:** ["acp/dac/register_and_delete_test.go::TestACP_AddWithoutIdentityAndDeleteWithoutIdentity_CanDelete", "acp/dac/register_and_delete_test.go::TestACP_AddWithoutIdentityAndDeleteWithIdentity_CanDelete", "acp/dac/register_and_delete_test.go::TestACP_AddWithIdentityAndDeleteWithIdentity_CanDelete", "acp/dac/register_and_delete_test.go::TestACP_AddWithIdentityAndDeleteWithoutIdentity_CanNotDelete", "acp/dac/register_and_delete_test.go::TestACP_AddWithIdentityAndDeleteWithWrongIdentity_CanNotDelete"]

Public documents can be deleted with or without identity. Deleting a private document with no identity or a wrong identity fails with `document not found or not authorized to access` and the document remains; the owner can delete it.

# `go.acp.dac.query`: DAC-filtered queries

- **Parents:** ["go.acp.dac"]

Every read surface returns only documents the requesting identity may read.

# `go.acp.dac.query.aggregates`: Aggregates count only readable documents

- **Parents:** ["go.acp.dac.query"]
- **Go-Tests:** ["acp/dac/avg_test.go::TestACP_QueryAverageWithoutIdentity", "acp/dac/avg_test.go::TestACP_QueryAverageWithIdentity", "acp/dac/avg_test.go::TestACP_QueryAverageWithWrongIdentity", "acp/dac/count_test.go::TestACP_QueryCountDocumentsWithoutIdentity", "acp/dac/count_test.go::TestACP_QueryCountRelatedObjectsWithoutIdentity", "acp/dac/count_test.go::TestACP_QueryCountDocumentsWithIdentity", "acp/dac/count_test.go::TestACP_QueryCountRelatedObjectsWithIdentity", "acp/dac/count_test.go::TestACP_QueryCountDocumentsWithWrongIdentity", "acp/dac/count_test.go::TestACP_QueryCountRelatedObjectsWithWrongIdentity"]

With Employee/Company collections holding public and private documents, `AVG(Employee: {field: salary})` and `COUNT(Employee: {})`, and `COUNT(employees: {})` inside `Company`, only include documents readable by the requester: anonymous and wrong identities see only public documents (AVG 15000, COUNT 2), the owner sees all (AVG 25000, COUNT 4).

# `go.acp.dac.query.relations`: Related objects are filtered by DAC

- **Parents:** ["go.acp.dac.query"]
- **Go-Tests:** ["acp/dac/relation_objects_test.go::TestACP_QueryManyToOneRelationObjectsWithoutIdentity", "acp/dac/relation_objects_test.go::TestACP_QueryOneToManyRelationObjectsWithoutIdentity", "acp/dac/relation_objects_test.go::TestACP_QueryManyToOneRelationObjectsWithIdentity", "acp/dac/relation_objects_test.go::TestACP_QueryOneToManyRelationObjectsWithIdentity", "acp/dac/relation_objects_test.go::TestACP_QueryManyToOneRelationObjectsWithWrongIdentity", "acp/dac/relation_objects_test.go::TestACP_QueryOneToManyRelationObjectsWithWrongIdentity"]

Querying `Employee { company { name } }` and `Company { employees { name } }` returns related documents only when readable: an unreadable related company comes back as `null`, unreadable employees are omitted, and the owner sees all relations.

# `go.acp.dac.query.indexed`: Index-backed queries respect DAC

- **Parents:** ["go.acp.dac.query"]
- **Go-Tests:** ["acp/dac/index/query_test.go::TestACPWithIndex_UponQueryingPrivateDocWithoutIdentity_ShouldNotFetch", "acp/dac/index/query_test.go::TestACPWithIndex_UponQueryingPrivateDocWithIdentity_ShouldFetch", "acp/dac/index/query_test.go::TestACPWithIndex_UponQueryingPrivateDocWithWrongIdentity_ShouldNotFetch", "acp/dac/index/query_with_relation_test.go::TestACPWithIndex_UponQueryingPrivateOneToManyRelatedDocWithoutIdentity_ShouldNotFetch", "acp/dac/index/query_with_relation_test.go::TestACPWithIndex_UponQueryingPrivateOneToManyRelatedDocWithIdentity_ShouldFetch", "acp/dac/index/query_with_relation_test.go::TestACPWithIndex_UponQueryingPrivateOneToManyRelatedDocWithWrongIdentity_ShouldNotFetch", "acp/dac/index/query_with_relation_test.go::TestACPWithIndex_UponQueryingPrivateManyToOneRelatedDocWithoutIdentity_ShouldNotFetch", "acp/dac/index/query_with_relation_test.go::TestACPWithIndex_UponQueryingPrivateManyToOneRelatedDocWithIdentity_ShouldFetch", "acp/dac/index/query_with_relation_test.go::TestACPWithIndex_UponQueryingPrivateManyToOneRelatedDocWithWrongIdentity_ShouldNotFetch"]

Filters served by a secondary index (on the queried collection or across one-to-many and many-to-one relations) return private documents only to their owner; anonymous and wrong identities get no rows.

# `go.acp.dac.query.index-maintenance`: Index maintenance under DAC

- **Parents:** ["go.acp.dac.query"]
- **Go-Tests:** ["acp/dac/delete_with_index_test.go::TestACP_DAC_DeleteDocByID_WithReaderOnlyAccess_DoesNotOrphanIndex", "acp/dac/index/new_test.go::TestACP_IndexNewWithSeparateRequest_OnCollectionWithPolicy_NoError", "acp/dac/index/new_test.go::TestACP_IndexNewWithDirective_OnCollectionWithPolicy_NoError", "acp/dac/index/update_by_non_creator_test.go::TestACPWithIndex_NonCreatorUpdatesIndexedField_Succeeds", "acp/dac/truncate_test.go::TestACP_DAC_TruncateSoftDeletedIndexedDocWithoutReadAccess"]

Indexes can be created on a policy-linked collection by `createIndex` or `@index`. A non-creator with update rights can update an indexed field and the index stays correct. A reader-only delete is denied with `document not found or not authorized to access` and leaves index entries intact. Truncating a soft-deleted uniquely-indexed document by an identity without read access frees the unique value so the owner can re-add it.

# `go.acp.dac.query.shared-cid`: Queries by a block CID shared by documents return only readable owners

- **Parents:** ["go.acp.dac.query"]
- **Go-Tests:** ["acp/dac/commits/with_cid_shared_field_test.go::TestACP_QueryCommitsWithSharedFieldCID_ReturnsOnlyAccessibleOwner", "acp/dac/query/with_cid_shared_field_test.go::TestACP_QuerySimpleWithSharedFieldCid_ReturnsOnlyAccessibleOwners"]

When two private documents of different owners share an identical field block CID, `Users(cid: ...)` time-travel and `_commits(cid: ...)` return only the document/commit the requester can read, never the other owner's.

# `go.acp.dac.query.subscription`: Subscriptions deliver only readable events

- **Parents:** ["go.acp.dac.query"]
- **Go-Tests:** ["acp/dac/subscription_test.go::TestACP_Subscription_AuthenticatedOwner_ReceivesEvents", "acp/dac/subscription_test.go::TestACP_Subscription_AuthenticatedReader_ReceivesEvents", "acp/dac/subscription_test.go::TestACP_Subscription_AnonymousSubscriber_ReceivesNothing"]

A GraphQL subscription on a policy-linked collection delivers create and update events to the owner and to an actor granted `reader`; an anonymous subscriber receives no events (an empty stream, not an error).

# `go.acp.dac.commits`: DAC on commit queries

- **Parents:** ["go.acp.dac"]

`_commits` and `_version` return commit blocks only for documents the requester can read.

# `go.acp.dac.commits.listing`: Unfiltered _commits shows only readable documents' commits

- **Parents:** ["go.acp.dac.commits"]
- **Go-Tests:** ["acp/dac/commits/basic_test.go::TestACP_QueryCommitsOnPublicDocWithoutIdentity_CanSeeCommits", "acp/dac/commits/basic_test.go::TestACP_QueryCommitsOnPublicDocWithIdentity_CanSeeCommits", "acp/dac/commits/basic_test.go::TestACP_QueryCommitsOnPrivateDocWithOwnerIdentity_CanSeeCommits", "acp/dac/commits/basic_test.go::TestACP_QueryCommitsOnPrivateDocWithoutIdentity_CanNotSeeCommits", "acp/dac/commits/basic_test.go::TestACP_QueryCommitsOnPrivateDocWithWrongIdentity_CanNotSeeCommits", "acp/dac/commits/basic_test.go::TestACP_QueryCommitsOnMixedDocsWithoutIdentity_CanOnlySeePublicCommits", "acp/dac/commits/basic_test.go::TestACP_QueryCommitsOnMixedDocsWithOwnerIdentity_CanSeeAllCommits", "acp/dac/commits/basic_test.go::TestACP_QueryCommitsOnMixedDocsWithWrongIdentity_CanOnlySeePublicCommits", "acp/dac/commits/multi_collection_test.go::TestACP_QueryCommitsMultiCollectionWithOwnerIdentity_CanSeeAllCommits", "acp/dac/commits/multi_collection_test.go::TestACP_QueryCommitsMultiCollectionWithWrongIdentity_CanNotSeeCommits"]

`_commits` returns all 3 commits (two field blocks and the composite) of a public document to anyone; a private document's commits only to its owner. With one public and one private document, anonymous and wrong identities see 3 commits and the owner 6. Across two private collections the owner sees all commits and a wrong identity none.

# `go.acp.dac.commits.filtered`: Filtered _commits is DAC-gated

- **Parents:** ["go.acp.dac.commits"]
- **Go-Tests:** ["acp/dac/commits/with_cid_test.go::TestACP_QueryCommitsWithCIDOnPrivateDocWithoutIdentity_CanNotSeeCommit", "acp/dac/commits/with_cid_test.go::TestACP_QueryCommitsWithCIDOnPrivateDocWithOwnerIdentity_CanSeeCommit", "acp/dac/commits/with_doc_id_test.go::TestACP_QueryCommitsWithDocIDOnPublicDocWithoutIdentity_CanSeeCommits", "acp/dac/commits/with_doc_id_test.go::TestACP_QueryCommitsWithDocIDOnPrivateDocWithOwnerIdentity_CanSeeCommits", "acp/dac/commits/with_doc_id_test.go::TestACP_QueryCommitsWithDocIDOnPrivateDocWithoutIdentity_CanNotSeeCommits", "acp/dac/commits/with_doc_id_test.go::TestACP_QueryCommitsWithDocIDOnPrivateDocWithWrongIdentity_CanNotSeeCommits", "acp/dac/commits/with_filters_test.go::TestACP_QueryCommitsWithFieldNameFilterOnPrivateDocWithoutIdentity_CanNotSeeCommits", "acp/dac/commits/with_filters_test.go::TestACP_QueryCommitsWithFieldNameFilterOnPrivateDocWithOwnerIdentity_CanSeeCommits", "acp/dac/commits/with_filters_test.go::TestACP_QueryCommitsAfterUpdateOnPrivateDocWithOwnerIdentity_CanSeeAllCommits", "acp/dac/commits/with_filters_test.go::TestACP_QueryCommitsAfterUpdateOnPrivateDocWithoutIdentity_CanNotSeeCommits", "acp/dac/commits/with_filters_test.go::TestACP_QueryCommitsWithDepthOnPrivateDocWithoutIdentity_CanNotSeeCommits"]

`_commits(docID:)`, `_commits(cid:)`, `_commits(fieldName:)` and `_commits(depth: 1)` return a private document's commits only to the owner; after an update the owner sees all 5 commits while anonymous and wrong identities see none. A public document's commits are visible without identity.

# `go.acp.dac.commits.version-and-grants`: _version sub-select and reader grants on commits

- **Parents:** ["go.acp.dac.commits"]
- **Go-Tests:** ["acp/dac/commits/version_subselect_test.go::TestACP_QueryVersionSubSelectOnPrivateDocWithOwnerIdentity_CanSeeCommits", "acp/dac/commits/version_subselect_test.go::TestACP_QueryVersionSubSelectOnPrivateDocWithWrongIdentity_CanNotSeeCommits", "acp/dac/commits/with_relationship_test.go::TestACP_QueryCommitsOnPrivateDocWithReaderRelationship_CanSeeCommits"]

The `_version` sub-select on a private document returns commits to the owner and nothing to a wrong identity (the parent is not returned). An identity granted `reader` via a document actor relationship can see the document's commits.

# `go.acp.dac.branchable`: DAC on branchable collections

- **Parents:** ["go.acp.dac"]

A branchable collection created with an identity is itself a private ACP object gating its collection-level commit DAG.

# `go.acp.dac.branchable.commits`: Branchable collection commit DAG visibility

- **Parents:** ["go.acp.dac.branchable"]
- **Go-Tests:** ["acp/dac/branchable/collection_commits_test.go::TestACP_BranchableCollectionCommits_OwnerCanSeeAllCommits", "acp/dac/branchable/collection_commits_test.go::TestACP_BranchableCollectionCommits_StrangerCannotSeeAnyCommit", "acp/dac/branchable/collection_commits_test.go::TestACP_BranchableCollectionCommits_NoIdentityCannotSeeAnyCommit", "acp/dac/branchable/collection_commits_test.go::TestACP_BranchableCollectionCommits_SharedCollectionReaderCanSeeAllCommits", "acp/dac/branchable/collection_commits_test.go::TestACP_BranchableCollectionCommits_CollectionReaderStillGatedByPrivateDoc", "acp/dac/branchable/collection_commits_test.go::TestACP_BranchableCollectionCommits_PublicCollectionIsVisibleToAll"]

For a private branchable collection the owner sees all 4 commits (2 field, composite, collection); strangers and anonymous requests see none even for a public document. Granting `reader` on the collection object reveals all commits of public documents, but a private document's commits stay hidden (only the collection commit shows). A branchable collection created without identity is public.

# `go.acp.dac.branchable.mutation`: Branchable collections keep document-level gating

- **Parents:** ["go.acp.dac.branchable"]
- **Go-Tests:** ["acp/dac/branchable/collection_mutation_test.go::TestACP_BranchableCollection_StrangerCannotUpdateDoc", "acp/dac/branchable/collection_mutation_test.go::TestACP_BranchableCollection_StrangerCannotDeleteDoc", "acp/dac/branchable/collection_mutation_test.go::TestACP_BranchableCollection_NonOwnerCannotShareCollection"]
- **Scope:** "the update test runs with collection-named and collection-save mutation types only"

A stranger cannot update or delete a private document in a branchable policy-linked collection (`document not found or not authorized to access`), and a non-owner adding a collection actor relationship fails with `failed to add document actor relationship with acp`.

# `go.acp.dac.branchable.p2p`: Branchable collection sync needs node access to the collection object

- **Parents:** ["go.acp.dac.branchable"]
- **Go-Tests:** ["acp/dac/branchable/peer_test.go::TestACP_P2PBranchableCollectionNotSyncedWithoutNodeCollectionAccess_LocalACP", "acp/dac/branchable/peer_test.go::TestACP_P2PBranchableCollectionSyncedWithNodeCollectionAccess_LocalACP", "acp/dac/branchable/peer_test.go::TestACP_P2PBranchableCollectionSharedReaderCanReadOnPeer_LocalACP"]
- **Scope:** "Local DAC only"

A private branchable collection's commits are not synced to a subscribing peer unless that peer's node identity is granted `reader` on the collection object; once granted, they sync, the owner reads every commit on the peer, strangers see nothing, and a stranger granted collection read on all nodes can read the synced DAG.

# `go.acp.dac.relationship`: Document actor relationships

- **Parents:** ["go.acp.dac"]

`AddDACActorRelationship` / `DeleteDACActorRelationship` grant and revoke a relation on a document to a target actor, requested by the owner or a manager.

# `go.acp.dac.relationship.missing-args`: Relationship operations reject missing arguments

- **Parents:** ["go.acp.dac.relationship"]
- **Go-Tests:** ["acp/dac/relationship/doc_actor/add/invalid_test.go::TestACP_AddDocActorRelationshipMissingDocID_Error", "acp/dac/relationship/doc_actor/add/invalid_test.go::TestACP_AddDocActorRelationshipMissingCollection_Error", "acp/dac/relationship/doc_actor/add/invalid_test.go::TestACP_AddDocActorRelationshipMissingRelationName_Error", "acp/dac/relationship/doc_actor/add/invalid_test.go::TestACP_AddDocActorRelationshipMissingTargetActorName_Error", "acp/dac/relationship/doc_actor/add/invalid_test.go::TestACP_AddDocActorRelationshipMissingReqestingIdentityName_Error", "acp/dac/relationship/doc_actor/delete/invalid_test.go::TestACP_DeleteDocActorRelationshipMissingDocID_Error", "acp/dac/relationship/doc_actor/delete/invalid_test.go::TestACP_DeleteDocActorRelationshipMissingCollection_Error", "acp/dac/relationship/doc_actor/delete/invalid_test.go::TestACP_DeleteDocActorRelationshipMissingRelationName_Error", "acp/dac/relationship/doc_actor/delete/invalid_test.go::TestACP_DeleteDocActorRelationshipMissingTargetActorName_Error", "acp/dac/relationship/doc_actor/delete/invalid_test.go::TestACP_DeleteDocActorRelationshipMissingReqestingIdentityName_Error"]

Adding or deleting a document actor relationship without a docID, relation name, target actor or requesting identity fails with `missing a required argument needed to add doc actor relationship.` (or `... delete doc actor relationship.`); an empty collection name fails with `collection name can't be empty`.

# `go.acp.dac.relationship.not-applicable`: Relationships on public documents or unpolicied collections are refused

- **Parents:** ["go.acp.dac.relationship"]
- **Go-Tests:** ["acp/dac/relationship/doc_actor/add/with_no_policy_on_collection_test.go::TestACP_AddDocActorRelationshipWithCollectionThatHasNoPolicy_NotAllowedError", "acp/dac/relationship/doc_actor/add/with_public_document_test.go::TestACP_AddDocActorRelationshipWithPublicDocument_CanAlreadyAccess_Error", "acp/dac/relationship/doc_actor/delete/with_no_policy_on_collection_test.go::TestACP_DeleteDocActorRelationshipWithCollectionThatHasNoPolicy_NotAllowedError", "acp/dac/relationship/doc_actor/delete/with_public_document_test.go::TestACP_DeleteDocActorRelationshipWithPublicDocument_CanAlreadyAccess_Error"]

Adding or deleting a relationship on a collection without a policy fails with `operation requires ACP, but collection has no policy`; on a public document (created without identity) it fails with `failed to add document actor relationship with acp` / `failed to delete document actor relationship with acp`.

# `go.acp.dac.relationship.dummy-relation`: Relations unknown to DRI change nothing; relations not on the policy error

- **Parents:** ["go.acp.dac.relationship"]
- **Go-Tests:** ["acp/dac/relationship/doc_actor/add/with_dummy_relation_test.go::TestACP_AddDocActorRelationshipWithDummyRelationDefinedOnPolicy_NothingChanges", "acp/dac/relationship/doc_actor/add/with_dummy_relation_test.go::TestACP_AddDocActorRelationshipWithDummyRelationNotDefinedOnPolicy_Error", "acp/dac/relationship/doc_actor/delete/with_dummy_relation_test.go::TestACP_DeleteDocActorRelationshipWithDummyRelationDefinedOnPolicy_NothingChanges", "acp/dac/relationship/doc_actor/delete/with_dummy_relation_test.go::TestACP_DeleteDocActorRelationshipWithDummyRelationNotDefinedOnPolicy_Error"]

Adding or deleting a relation that exists on the policy but grants no permission succeeds and leaves access unchanged; a relation not defined on the policy fails with `failed to add document actor relationship with acp` / `failed to delete document actor relationship with acp`.

# `go.acp.dac.relationship.reader`: Granting reader allows reading only

- **Parents:** ["go.acp.dac.relationship"]
- **Go-Tests:** ["acp/dac/relationship/doc_actor/add/with_reader_gql_test.go::TestACP_OwnerGivesOnlyReadAccessToAnotherActor_GQL_OtherActorCanReadButNotUpdate", "acp/dac/relationship/doc_actor/add/with_reader_test.go::TestACP_OwnerGivesReadAccessToAnotherActorTwice_ShowThatTheRelationshipAlreadyExists", "acp/dac/relationship/doc_actor/add/with_reader_test.go::TestACP_OwnerGivesReadAccessToAnotherActor_OtherActorCanRead", "acp/dac/relationship/doc_actor/add/with_reader_test.go::TestACP_OwnerGivesReadAccessToAnotherActor_OtherActorCanReadSoCanTheOwner", "acp/dac/relationship/doc_actor/add/with_reader_test.go::TestACP_OwnerGivesOnlyReadAccessToAnotherActor_OtherActorCanReadButNotUpdate", "acp/dac/relationship/doc_actor/add/with_reader_test.go::TestACP_OwnerGivesOnlyReadAccessToAnotherActor_OtherActorCanReadButNotDelete"]
- **Scope:** "the read-but-not-update tests are split by mutation type: collection-named/save, and GraphQL"

After the owner grants identity 2 `reader`, identity 2 reads the document and the owner still does (no ownership transfer); identity 2's update and delete fail with `document not found or not authorized to access`. Granting the same relation twice reports that it already exists (`ExpectedExistence: true`) and is a no-op.

# `go.acp.dac.relationship.updater`: Granting update allows updating

- **Parents:** ["go.acp.dac.relationship"]
- **Go-Tests:** ["acp/dac/relationship/doc_actor/add/with_only_update_gql_test.go::TestACP_OwnerGivesUpdateAccessToAnotherActorWithoutExplicitReadPerm_GQL_OtherActorCanUpdate", "acp/dac/relationship/doc_actor/add/with_only_update_test.go::TestACP_OwnerGivesUpdateAccessToAnotherActorWithoutExplicitReadPerm_OtherActorCanUpdate", "acp/dac/relationship/doc_actor/add/with_update_gql_test.go::TestACP_OwnerGivesUpdateAccessToAnotherActorTwice_GQL_ShowThatTheRelationshipAlreadyExists", "acp/dac/relationship/doc_actor/add/with_update_gql_test.go::TestACP_OwnerGivesUpdateAccessToAnotherActor_GQL_OtherActorCanUpdate", "acp/dac/relationship/doc_actor/add/with_update_test.go::TestACP_OwnerGivesUpdateAccessToAnotherActorTwice_ShowThatTheRelationshipAlreadyExists", "acp/dac/relationship/doc_actor/add/with_update_test.go::TestACP_OwnerGivesUpdateAccessToAnotherActor_OtherActorCanUpdate", "acp/dac/relationship/doc_actor/add/with_update_test.go::TestACP_OwnerGivesUpdateAccessToAnotherActor_OtherActorCanUpdateSoCanTheOwner"]
- **Scope:** "tests are split by mutation type: collection API types versus GraphQL requests"

After the owner grants `updater`, the actor can update the document (and read it, as the policy's read expression includes updater, or even without explicit read permission) and the owner still can. Granting twice reports existence. Before the grant the update fails with `document not found or not authorized to access` on the collection API.

# `go.acp.dac.relationship.deleter`: Granting delete allows deleting

- **Parents:** ["go.acp.dac.relationship"]
- **Go-Tests:** ["acp/dac/relationship/doc_actor/add/with_delete_test.go::TestACP_OwnerGivesDeleteAccessToAnotherActorTwice_ShowThatTheRelationshipAlreadyExists", "acp/dac/relationship/doc_actor/add/with_delete_test.go::TestACP_OwnerGivesDeleteAccessToAnotherActor_OtherActorCanDelete", "acp/dac/relationship/doc_actor/add/with_delete_test.go::TestACP_OwnerGivesDeleteAccessToAnotherActor_OtherActorCanDeleteSoCanTheOwner", "acp/dac/relationship/doc_actor/add/with_only_delete_test.go::TestACP_OwnerGivesDeleteAccessToAnotherActorWithoutExplicitReadPerm_OtherActorCanDelete"]

After the owner grants `deleter`, the actor can delete the document, even without explicit read permission, and the owner can still delete it; before the grant the delete fails with `document not found or not authorized to access`. Granting twice reports existence.

# `go.acp.dac.relationship.manager`: Managers grant only the relations they manage

- **Parents:** ["go.acp.dac.relationship"]
- **Go-Tests:** ["acp/dac/relationship/doc_actor/add/with_manager_gql_test.go::TestACP_OwnerMakesAManagerThatGivesItSelfReadAndWriteAccess_GQL_ManagerCanReadAndWrite", "acp/dac/relationship/doc_actor/add/with_manager_gql_test.go::TestACP_OwnerMakesManagerButManagerCanNotPerformOperations_GQL_ManagerCantReadOrWrite", "acp/dac/relationship/doc_actor/add/with_manager_gql_test.go::TestACP_ManagerAddsRelationshipWithRelationItDoesNotManageAccordingToPolicy_GQL_Error", "acp/dac/relationship/doc_actor/add/with_manager_test.go::TestACP_ManagerGivesReadAccessToAnotherActor_OtherActorCanRead", "acp/dac/relationship/doc_actor/add/with_manager_test.go::TestACP_ManagerGivesWriteAccessToAnotherActor_OtherActorCanWrite", "acp/dac/relationship/doc_actor/add/with_manager_test.go::TestACP_OwnerMakesAManagerThatGivesItSelfReadAccess_ManagerCanRead", "acp/dac/relationship/doc_actor/add/with_manager_test.go::TestACP_OwnerMakesAManagerThatGivesItSelfReadAndWriteAccess_ManagerCanReadAndWrite", "acp/dac/relationship/doc_actor/add/with_manager_test.go::TestACP_ManagerAddsRelationshipWithRelationItDoesNotManageAccordingToPolicy_Error", "acp/dac/relationship/doc_actor/add/with_manager_test.go::TestACP_OwnerMakesManagerButManagerCanNotPerformOperations_ManagerCantReadOrWrite", "acp/dac/relationship/doc_actor/add/with_manager_test.go::TestACP_CantMakeRelationshipIfNotOwnerOrManager_Error"]
- **Scope:** "tests are split by mutation type: collection API types versus GraphQL requests"

An actor made `admin` (which `manages` reader/updater) can grant those relations to others or itself, after which the grantee can read or write; being a manager alone gives no read, update or delete rights (`document not found or not authorized to access`). Granting a relation the manager does not manage fails with `UNAUTHORIZED`, and an actor that is neither owner nor manager fails with `failed to add document actor relationship with acp`.

# `go.acp.dac.relationship.all-actors`: Granting and revoking the all-actors wildcard

- **Parents:** ["go.acp.dac.relationship"]
- **Go-Tests:** ["acp/dac/relationship/doc_actor/add/with_target_all_actors_gql_test.go::TestACP_OwnerGivesOnlyReadAccessToAllActors_GQL_AllActorsCanReadButNotUpdateOrDelete", "acp/dac/relationship/doc_actor/add/with_target_all_actors_gql_test.go::TestACP_OwnerGivesOnlyReadAccessToAllActors_GQL_CanReadEvenWithoutIdentityButNotUpdateOrDelete", "acp/dac/relationship/doc_actor/add/with_target_all_actors_test.go::TestACP_OwnerGivesOnlyReadAccessToAllActors_AllActorsCanReadButNotUpdateOrDelete", "acp/dac/relationship/doc_actor/add/with_target_all_actors_test.go::TestACP_OwnerGivesOnlyReadAccessToAllActors_CanReadEvenWithoutIdentityButNotUpdateOrDelete", "acp/dac/relationship/doc_actor/delete/with_target_all_actors_test.go::TestACP_OwnerRevokesAccessFromAllNonExplicitActors_ActorsCanNotReadAnymore", "acp/dac/relationship/doc_actor/delete/with_target_all_actors_test.go::TestACP_OwnerRevokesAccessFromAllNonExplicitActors_ExplicitActorsCanStillRead", "acp/dac/relationship/doc_actor/delete/with_target_all_actors_test.go::TestACP_OwnerRevokesAccessFromAllNonExplicitActors_NonIdentityRequestsCanNotReadAnymore"]
- **Scope:** "the add tests are split by mutation type: collection API types versus GraphQL requests"

Granting `reader` to all actors (`*`) lets every identity and anonymous requests read but not update or delete (`document not found or not authorized to access`). Revoking the wildcard removes that implicit access for identities and anonymous requests, while actors granted explicitly (before or after) keep reading.

# `go.acp.dac.relationship.revoke`: Revoking a relation removes the access it gave

- **Parents:** ["go.acp.dac.relationship"]
- **Go-Tests:** ["acp/dac/relationship/doc_actor/delete/with_delete_test.go::TestACP_OwnerRevokesDeleteAccess_OtherActorCanNoLongerDelete", "acp/dac/relationship/doc_actor/delete/with_manager_test.go::TestACP_ManagerRevokesReadAccess_OtherActorCanNoLongerRead", "acp/dac/relationship/doc_actor/delete/with_manager_test.go::TestACP_OwnerRevokesManagersAccess_ManagerCanNoLongerManageOthers", "acp/dac/relationship/doc_actor/delete/with_reader_test.go::TestACP_OwnerRevokesReadAccessTwice_ShowThatTheRecordWasNotFoundSecondTime", "acp/dac/relationship/doc_actor/delete/with_reader_test.go::TestACP_OwnerRevokesGivenReadAccess_OtherActorCanNoLongerRead", "acp/dac/relationship/doc_actor/delete/with_update_test.go::TestACP_OwnerRevokesUpdateAccess_OtherActorCanNoLongerUpdate", "acp/dac/relationship/doc_actor/delete/with_update_test.go::TestACP_OwnerRevokesUpdateAccess_GQL_OtherActorCanNoLongerUpdate"]
- **Scope:** "the update-revocation tests are split by mutation type: collection-named/save versus GraphQL"

After the owner or a managing admin revokes `reader`, `updater` or `deleter`, the actor can no longer read, update (`document not found or not authorized to access` on the collection API, silent no-op over GraphQL) or delete. Revoking an admin stops it granting (`failed to add document actor relationship with acp`). Revoking a relation twice reports `ExpectedRecordFound: false` the second time.

# `go.acp.dac.relationship.revoke-forbidden`: Owner relations cannot be revoked

- **Parents:** ["go.acp.dac.relationship"]
- **Go-Tests:** ["acp/dac/relationship/doc_actor/delete/with_manager_test.go::TestACP_AdminTriesToRevokeOwnersAccess_NotAllowedError", "acp/dac/relationship/doc_actor/delete/with_self_test.go::TestACP_AdminTriesToRevokeItsOwnAccess_NotAllowedError", "acp/dac/relationship/doc_actor/delete/with_self_test.go::TestACP_OwnerTriesToRevokeItsOwnAccess_NotAllowedError"]

An admin revoking the owner's `owner` relation fails with `OPERATION_FORBIDDEN`; the owner revoking its own ownership, or an admin revoking its own `admin`, fails with `failed to delete document actor relationship with acp`.

# `go.acp.dac.p2p`: DAC across P2P replication

- **Parents:** ["go.acp.dac"]

Private documents only replicate to, and are only readable on, peers whose node identity has access.

# `go.acp.dac.p2p.remote-dac`: Private documents replicate under a shared Remote DAC

- **Parents:** ["go.acp.dac.p2p"]
- **Go-Tests:** ["acp/dac/p2p/add_test.go::TestACP_P2PAddPrivateDocumentsOnDifferentNodes_RemoteDAC", "acp/dac/p2p/add_test.go::TestACP_P2PAddPrivateDocumentAndSyncAfterAddingRelationship_RemoteDAC", "acp/dac/p2p/delete_test.go::TestACP_P2PDeletePrivateDocumentsOnDifferentNodes_RemoteDAC", "acp/dac/p2p/replicator_test.go::TestACP_P2POneToOneReplicatorWithPermissionedCollection_RemoteDAC", "acp/dac/p2p/subscribe_test.go::TestACP_P2PSubscribeAddGetSingleWithPermissionedCollection_RemoteDAC", "acp/dac/p2p/update_test.go::TestACP_P2PUpdatePrivateDocumentsOnDifferentNodes_RemoteDAC"]
- **Scope:** "Remote DAC only"

With a Remote DAC shared by all nodes, owners can create, update and delete private documents on different nodes and replicate them via replicators. A replicated document is readable on every node by the owner and hidden from anonymous and wrong identities. Over a collection subscription, a node whose identity lacks access receives nothing; after the owner grants the node identity `reader`, the document syncs.

# `go.acp.dac.p2p.relationship-sync`: Relationships are shared across nodes under Remote DAC

- **Parents:** ["go.acp.dac.p2p"]
- **Go-Tests:** ["acp/dac/p2p/replicator_with_doc_actor_relationship_test.go::TestACP_P2PReplicatorWithPermissionedCollectionAddDocActorRelationship_RemoteDAC", "acp/dac/p2p/subscribe_grant_race_test.go::TestACP_P2PSubscribe_MultiBlockDoc_NonCreatorReadsAfterGrantsLand_RemoteDAC", "acp/dac/p2p/subscribe_with_doc_actor_relationship_test.go::TestACP_P2PSubscribeAddGetSingleWithPermissionedCollectionAddDocActorRelationship_RemoteDAC"]
- **Scope:** "Remote DAC only"

With Remote DAC, a relationship added on one node is visible from another (adding it again there reports existence, deleting it again reports not found), and the grantee reads the document on both nodes. A non-creator granted read before the grants propagate can read a multi-block document on the receiving node once they land.

# `go.acp.dac.p2p.local-dac`: P2P setup and sync with Local DAC

- **Parents:** ["go.acp.dac.p2p"]
- **Go-Tests:** ["acp/dac/p2p/replicator_test.go::TestACP_P2POneToOneReplicatorWithPermissionedCollection_LocalACP", "acp/dac/p2p/subscribe_test.go::TestACP_P2PSubscribeAddGetSingleWithPermissionedCollection_LocalACP", "acp/dac/p2p/subscribe_with_patched_collection_test.go::TestACP_P2PSubscribeUpdateAuthoredOnInactiveCollectionVersion_SyncsUpdate"]
- **Scope:** "Local DAC only"

With Local DAC a replicator can be added for a policy-linked collection and a subscription to it is listed by `ListP2PCollections`. An update authored against a collection version that is later neither root nor active still syncs to a subscribed peer whose node identity was granted `reader`.

# `go.acp.dac.p2p.shared-block`: Shared field blocks sync when any owner is readable

- **Parents:** ["go.acp.dac.p2p"]
- **Go-Tests:** ["acp/dac/p2p/sync_shared_field_test.go::TestACP_P2PSyncSharedFieldBlock_AccessToFirstDoc", "acp/dac/p2p/sync_shared_field_test.go::TestACP_P2PSyncSharedFieldBlock_AccessToSecondDoc"]
- **Scope:** "Local and Remote DAC only"

When two documents share an identical field block and the peer is granted only one of them, the peer syncs that document in full, including the shared block, regardless of the owners' lexicographic order, and does not receive the other document.

# `go.acp.dac.p2p.non-creator-update`: Non-creators update replicated documents on a peer

- **Parents:** ["go.acp.dac.p2p"]
- **Go-Tests:** ["acp/dac/p2p/update_by_non_creator_on_peer_test.go::TestACP_P2P_NonCreatorUpdatesIndexedFieldOnPeerNode_Succeeds", "acp/dac/p2p/update_by_non_creator_on_peer_test.go::TestACP_P2P_NonCreatorUpdatesNonIndexedFieldOnPeerNode_Succeeds"]
- **Scope:** "Remote DAC only"

A non-creator granted update rights can update a document on the receiving node after it replicated, for both indexed and non-indexed fields, and the indexed query returns the new value (no `corrupted index`).

# `go.acp.nac`: Node access control (NAC)

- **Parents:** ["go.acp"]

When NAC is enabled, every node operation requires an identity with the matching node permission; unauthorized calls fail with `not authorized to perform operation. Permission: <perm>`.

# `go.acp.nac.lifecycle`: NAC configuration lifecycle

- **Parents:** ["go.acp.nac"]

NAC is configured at start, persisted, and can be temporarily disabled and re-enabled.

# `go.acp.nac.lifecycle.start`: Starting a node with or without NAC

- **Parents:** ["go.acp.nac.lifecycle"]
- **Go-Tests:** ["acp/nac/nac_start_test.go::TestNAC_StartWithDefaultConfig_NACStatusIsDisabled", "acp/nac/nac_start_test.go::TestNAC_StartWithDefaultConfigWithIdentity_NACStatusIsDisabled", "acp/nac/nac_start_test.go::TestNAC_StartNodeWithIdentityAndWithNACEnableTrue_NACEnabledSuccessfully", "acp/nac/nac_start_test.go::TestNAC_StartNodeNoIdentityWithNACEnableTrue_ErrorAsIdentityIsNeeded", "acp/nac/nac_start_test.go::TestNAC_StartNodeWithIdentityAndWithNACEnableFalse_NACNotEnabled"]

A default start reports NAC status `NACNotConfigured`, with or without identity, and also when started with an identity and NAC disabled. Starting with an identity and NAC enabled reports `NACEnabled` to that identity and denies the status call without identity (`not authorized to perform operation`). Enabling NAC without an identity fails with `can not start nac without identity`.

# `go.acp.nac.lifecycle.restart`: NAC state survives restart

- **Parents:** ["go.acp.nac.lifecycle"]
- **Go-Tests:** ["acp/nac/nac_restart_test.go::TestNAC_RestartNodeWithNACEnabledWithoutNACArgs_RestartsAndNACIsStillEnabled", "acp/nac/nac_restart_test.go::TestNAC_RestartNodeWithNACEnabledWithExplicitlySpecifyingSameArgs_RestartsAndNACIsStillEnabled", "acp/nac/nac_restart_test.go::TestNAC_RestartNodeWithNACEnabledWithAnotherIdentity_IgnoreNewIdentityAndRestartWithExistingNACState"]
- **Scope:** "file-backed database types only"

After NAC is enabled, restarting with no NAC arguments, with the same identity, or with a different identity keeps NAC enabled with the original owner (identity 1 still sees `NACEnabled`; the new identity is ignored).

# `go.acp.nac.lifecycle.disable`: Temporarily disabling NAC

- **Parents:** ["go.acp.nac.lifecycle"]
- **Go-Tests:** ["acp/nac/disable_nac_test.go::TestNAC_DisableNotConfiguredBefore_Error", "acp/nac/disable_nac_test.go::TestNAC_DisableNotConfiguredBeforeWithIdentity_Error", "acp/nac/disable_nac_test.go::TestNAC_DisableWithoutIdentityOnNodeThatHasConfigured_Error", "acp/nac/disable_nac_test.go::TestNAC_DisableWithWrongIdentityOnNodeThatHasConfigured_Error", "acp/nac/disable_nac_test.go::TestNAC_DisableWithIdentityOnNodeThatHasNACConfiguredAndEnabled_Successful", "acp/nac/disable_nac_test.go::TestNAC_DisableNoIdentityWhenConfiguredAndAlreadyDisabledBefore_Error", "acp/nac/disable_nac_test.go::TestNAC_DisableWithIdentityWhenConfiguredAndAlreadyDisabledBefore_Error", "acp/nac/disable_nac_test.go::TestNAC_DisableSuccessfullyThenRestartWithNoArgs_RemainsDisabled", "acp/nac/disable_nac_test.go::TestNAC_DisableSuccessfullyThenRestartWithStartArgs_RemainsDisabled"]
- **Scope:** "the restart tests run on file-backed database types only"

Disabling on a node where NAC was never configured fails with `node acp is not configured`. With NAC enabled, disabling without identity or with a wrong identity fails with permission `disable-nac`; the owner succeeds and status becomes disabled. Disabling again fails with `node acp is already disabled`. A restart, with or without start args, keeps it disabled.

# `go.acp.nac.lifecycle.re-enable`: Re-enabling NAC

- **Parents:** ["go.acp.nac.lifecycle"]
- **Go-Tests:** ["acp/nac/re_enable_nac_test.go::TestNAC_ReEnableNotConfiguredBefore_Error", "acp/nac/re_enable_nac_test.go::TestNAC_ReEnableNotConfiguredBeforeWithIdentity_Error", "acp/nac/re_enable_nac_test.go::TestNAC_ReEnableWithNoIdentityWhenTemporarilyDisabled_Error", "acp/nac/re_enable_nac_test.go::TestNAC_ReEnableWithWrongIdentityWhenTemporarilyDisabled_Error", "acp/nac/re_enable_nac_test.go::TestNAC_ReEnableWithValidIdentityWhenTemporarilyDisabled_NACReEnabled", "acp/nac/re_enable_nac_test.go::TestNAC_ReEnableWithNoIdentityWhenAlreadyEnabled_Error", "acp/nac/re_enable_nac_test.go::TestNAC_ReEnableWithWrongIdentityWhenAlreadyEnabled_Error", "acp/nac/re_enable_nac_test.go::TestNAC_ReEnableWithValidIdentityWhenAlreadyEnabled_Error", "acp/nac/re_enable_nac_test.go::TestNAC_ReEnableSuccessfullyThenRestartWithNoArgs_RemainsReEnabled", "acp/nac/re_enable_nac_test.go::TestNAC_ReEnableSuccessfullyThenRestartWithStartArgs_RemainsReEnabled", "acp/nac/re_enable_nac_test.go::TestNAC_ReEnableTemporarilyDisabledNACAfterRestart_ReEnabledSuccessfully"]
- **Scope:** "the restart tests run on file-backed database types only"

Re-enabling where NAC was never configured fails with `node acp is not configured`. When temporarily disabled, re-enabling without identity or with a wrong identity fails with permission `re-enable-nac`; the owner succeeds and gating resumes. When already enabled, non-owners get `not authorized to perform operation. Permission: re-enable-nac` and the owner gets `node acp is already enabled`. Re-enabled state survives restart, and a disabled state can be re-enabled after a restart.

# `go.acp.nac.relationship`: NAC actor relationships

- **Parents:** ["go.acp.nac"]

`AddNACActorRelationship` / `DeleteNACActorRelationship` grant or revoke node relations such as `admin`.

# `go.acp.nac.relationship.add`: Adding NAC relationships

- **Parents:** ["go.acp.nac.relationship"]
- **Go-Tests:** ["acp/nac/add_nac_relationship_test.go::TestNAC_AddRelationshipWhenNACNotConfiguredBefore_Error", "acp/nac/add_nac_relationship_test.go::TestNAC_AddRelationshipWhenNACIsEnabledWithInvalidIdentities_Error", "acp/nac/add_nac_relationship_test.go::TestNAC_AddRelationshipWhenNACIsDisabledWithInvalidIdentities_Error", "acp/nac/add_nac_relationship_test.go::TestNAC_AddRelationshipWithInvalidRelationName_Error", "acp/nac/add_nac_relationship_test.go::TestNAC_AddRelationshipWithValidIdentity_RelationshipAdded", "acp/nac/add_nac_relationship_test.go::TestNAC_AddRelationshipForAllIdentities_AllIdentitiesCanAccess", "acp/nac/add_nac_relationship_test.go::TestNAC_AddRelationshipStillRequiresIdentityEvenIfAllIdentitiesGivenAccess_StillNeedIdentity"]
- **Go-Bug-Suspect:** "an authorized requestor with no target identity gets 'actor must be a valid did' instead of a clearer missing-target error (TODO-ACP, Go issue #3796)"

Without configured NAC the call fails with `node acp is not configured`; with NAC disabled it fails with `operation requires ACP, but ACP not available`. With NAC enabled, an unauthorized requestor or missing target fails with permission `add-nac-relation`, an unknown relation with `relation not in resource`. The owner grants `admin` to an identity or to all identities (`*`), after which those identities can operate; requests without identity are still denied (`node acp relationship operation requires identity`).

# `go.acp.nac.relationship.delete`: Deleting NAC relationships

- **Parents:** ["go.acp.nac.relationship"]
- **Go-Tests:** ["acp/nac/delete_nac_relationship_test.go::TestNAC_DeleteRelationshipWhenNACNotConfiguredBefore_Error", "acp/nac/delete_nac_relationship_test.go::TestNAC_DeleteRelationshipWhenNACIsEnabledWithInvalidIdentities_Error", "acp/nac/delete_nac_relationship_test.go::TestNAC_DeleteRelationshipWhenNACIsDisabledWithInvalidIdentities_Error", "acp/nac/delete_nac_relationship_test.go::TestNAC_DeleteRelationshipWithInvalidRelationName_Error", "acp/nac/delete_nac_relationship_test.go::TestNAC_DeleteRelationshipWithValidIdentity_RelationshipDeleted", "acp/nac/delete_nac_relationship_test.go::TestNAC_DeleteRelationshipForAllIdentities_AllImplicitIdentitiesAccessRevoked", "acp/nac/delete_nac_relationship_test.go::TestNAC_DeleteRelationshipStillRequiresIdentityEvenIfAllIdentitiesGivenAccess_StillNeedIdentity"]
- **Go-Bug-Suspect:** "same missing-target case as add asserts 'actor must be a valid did' under a TODO-ACP (Go issue #3796)"

Without configured NAC the call fails with `node acp is not configured`; with NAC disabled with `operation requires ACP, but ACP not available`. With NAC enabled, an unauthorized requestor fails with permission `delete-nac-relation`, an unknown relation with `relation not in resource`. Revoking `admin` from an identity or from all identities removes the access they had (subsequent calls fail with the operation's permission error); identity-less requests are rejected with `node acp relationship operation requires identity`.

# `go.acp.nac.gates`: NAC gating of node operations

- **Parents:** ["go.acp.nac"]

With NAC enabled by identity 1, each operation succeeds for identity 1 and fails without identity or with identity 2 with `not authorized to perform operation. Permission: <perm>`.

# `go.acp.nac.gates.collection-schema`: Collection definition changes are gated

- **Parents:** ["go.acp.nac.gates"]
- **Go-Tests:** ["acp/nac/add_collection_test.go::TestNAC_GatesAddCollection_AllowIfAuthorizedElseError", "acp/nac/delete_collection_test.go::TestNAC_GatesDeleteCollection_AuthorizedIdentity_AllowAccess", "acp/nac/delete_collection_test.go::TestNAC_GatesDeleteCollection_NoIdentity_NotAuthorizedError", "acp/nac/delete_collection_test.go::TestNAC_GatesDeleteCollection_WrongIdentity_NotAuthorizedError", "acp/nac/patch_collection_test.go::TestNAC_GatesPatchCollection_AuthorizedIdentity_AllowAccess", "acp/nac/patch_collection_test.go::TestNAC_GatesPatchCollection_NoIdentity_NotAuthorizedError", "acp/nac/patch_collection_test.go::TestNAC_GatesPatchCollection_WrongIdentity_NotAuthorizedError", "acp/nac/set_active_collection_version_test.go::TestNAC_GatesSetActiveCollectionVersion_AuthorizedIdentity_AllowAccess", "acp/nac/set_active_collection_version_test.go::TestNAC_GatesSetActiveCollectionVersion_NoIdentity_NotAuthorizedError", "acp/nac/set_active_collection_version_test.go::TestNAC_GatesSetActiveCollectionVersion_WrongIdentity_NotAuthorizedError", "acp/nac/truncate_collection_test.go::TestNAC_GatesTruncateCollection_AuthorizedIdentity_AllowAccess", "acp/nac/truncate_collection_test.go::TestNAC_GatesTruncateCollection_NoIdentity_NotAuthorizedError", "acp/nac/truncate_collection_test.go::TestNAC_GatesTruncateCollection_WrongIdentity_NotAuthorizedError"]

With NAC enabled, adding a collection, deleting a collection, patching a collection and setting the active collection version succeed for the NAC owner and fail for anyone else with `not authorized to perform operation. Permission: patch-collection`; truncating a collection fails with permission `get-collection`.

# `go.acp.nac.gates.collection-read`: Collection lookups are gated

- **Parents:** ["go.acp.nac.gates"]
- **Go-Tests:** ["acp/nac/get_collection_by_id_test.go::TestNAC_GatesGetCollectionByID_AuthorizedIdentity_AllowAccess", "acp/nac/get_collection_by_id_test.go::TestNAC_GatesGetCollectionByID_NoIdentity_NotAuthorizedError", "acp/nac/get_collection_by_id_test.go::TestNAC_GatesGetCollectionByID_WrongIdentity_NotAuthorizedError", "acp/nac/get_collection_by_name_test.go::TestNAC_GatesGetCollectionByName_AuthorizedIdentity_AllowAccess", "acp/nac/get_collection_by_name_test.go::TestNAC_GatesGetCollectionByName_NoIdentity_NotAuthorizedError", "acp/nac/get_collection_by_name_test.go::TestNAC_GatesGetCollectionByName_WrongIdentity_NotAuthorizedError", "acp/nac/get_collection_by_version_test.go::TestNAC_GatesGetCollectionByVersion_AuthorizedIdentity_AllowAccess", "acp/nac/get_collection_by_version_test.go::TestNAC_GatesGetCollectionByVersion_NoIdentity_NotAuthorizedError", "acp/nac/get_collection_by_version_test.go::TestNAC_GatesGetCollectionByVersion_WrongIdentity_NotAuthorizedError", "acp/nac/list_collection_test.go::TestNAC_GatesListCollection_AuthorizedIdentity_AllowAccess", "acp/nac/list_collection_test.go::TestNAC_GatesListCollection_NoIdentity_NotAuthorizedError", "acp/nac/list_collection_test.go::TestNAC_GatesListCollection_WrongIdentity_NotAuthorizedError"]

With NAC enabled, getting a collection by ID, by name, by version and listing collections succeed for the NAC owner and fail without identity or with a wrong identity with `not authorized to perform operation. Permission: get-collection`. Getting by an unknown version as the owner fails with `collection not found`, showing authorization passed.

# `go.acp.nac.gates.documents`: Document operations are gated

- **Parents:** ["go.acp.nac.gates"]
- **Go-Tests:** ["acp/nac/add_document_test.go::TestNAC_GatesAddDocument_AuthorizedIdentity_AllowAccess", "acp/nac/add_document_test.go::TestNAC_GatesAddDocument_NoIdentity_NotAuthorizedError", "acp/nac/add_document_test.go::TestNAC_GatesAddDocument_WrongIdentity_NotAuthorizedError", "acp/nac/delete_document_test.go::TestNAC_GatesDeleteDocument_AuthorizedIdentity_AllowAccess", "acp/nac/delete_document_test.go::TestNAC_GatesDeleteDocument_NoIdentity_NotAuthorizedError", "acp/nac/delete_document_test.go::TestNAC_GatesDeleteDocument_WrongIdentity_NotAuthorizedError", "acp/nac/read_document_test.go::TestNAC_GatesReadDocument_AuthorizedIdentity_AllowAccess", "acp/nac/read_document_test.go::TestNAC_GatesReadDocument_NoIdentity_NotAuthorizedError", "acp/nac/read_document_test.go::TestNAC_GatesReadDocument_WrongIdentity_NotAuthorizedError", "acp/nac/update_document_test.go::TestNAC_GatesUpdateDocument_AuthorizedIdentity_AllowAccess", "acp/nac/update_document_test.go::TestNAC_GatesUpdateDocument_NoIdentity_NotAuthorizedError", "acp/nac/update_document_test.go::TestNAC_GatesUpdateDocument_WrongIdentity_NotAuthorizedError", "acp/nac/update_document_with_filter_test.go::TestNAC_GatesUpdateDocumentWithFilter_AuthorizedIdentity_AllowAccess", "acp/nac/update_document_with_filter_test.go::TestNAC_GatesUpdateDocumentWithFilter_NoIdentity_NotAuthorizedError", "acp/nac/update_document_with_filter_test.go::TestNAC_GatesUpdateDocumentWithFilter_WrongIdentity_NotAuthorizedError"]

With NAC enabled, adding, reading (GraphQL query), updating (by ID or with a filter) and deleting documents succeed for the NAC owner; without identity or with a wrong identity they fail with `not authorized to perform operation. Permission: get-collection`.

# `go.acp.nac.gates.signature`: Block signature verification is gated

- **Parents:** ["go.acp.nac.gates"]
- **Go-Tests:** ["acp/nac/verify_signature_test.go::TestNAC_GatesVerifySignature_AuthorizedIdentity_AllowAccess", "acp/nac/verify_signature_test.go::TestNAC_GatesVerifySignature_GoClient_AuthorizedIdentity_AllowAccess", "acp/nac/verify_signature_test.go::TestNAC_GatesVerifySignature_NoIdentity_NotAuthorizedError", "acp/nac/verify_signature_test.go::TestNAC_GatesVerifySignature_WrongIdentity_NotAuthorizedError"]
- **Scope:** "authorized-identity tests run on HTTP and CLI clients (default signer) or Go and C clients (signed documents), one test each"

With NAC enabled, verifying a block signature succeed for the NAC owner and fail without identity or with a wrong identity with `not authorized to perform operation. Permission: verify-signature`.

# `go.acp.nac.gates.indexes`: Index operations are gated

- **Parents:** ["go.acp.nac.gates"]
- **Go-Tests:** ["acp/nac/delete_index_test.go::TestNAC_GatesDeleteIndex_AuthorizedIdentity_AllowAccess", "acp/nac/delete_index_test.go::TestNAC_GatesDeleteIndex_NoIdentity_NotAuthorizedError", "acp/nac/delete_index_test.go::TestNAC_GatesDeleteIndex_WrongIdentity_NotAuthorizedError", "acp/nac/list_index_test.go::TestNAC_GatesListIndex_AuthorizedIdentity_AllowAccess", "acp/nac/list_index_test.go::TestNAC_GatesListIndex_NoIdentity_NotAuthorizedError", "acp/nac/list_index_test.go::TestNAC_GatesListIndex_WrongIdentity_NotAuthorizedError", "acp/nac/new_index_test.go::TestNAC_GatesNewIndex_AuthorizedIdentity_AllowAccess", "acp/nac/new_index_test.go::TestNAC_GatesNewIndex_NoIdentity_NotAuthorizedError", "acp/nac/new_index_test.go::TestNAC_GatesNewIndex_WrongIdentity_NotAuthorizedError"]
- **Scope:** "authorized-identity tests run on Go, HTTP, CLI and C clients (not JS)"

With NAC enabled, creating, deleting and listing indexes succeed for the NAC owner and fail for anyone else with permission `get-collection`.

# `go.acp.nac.gates.encrypted-indexes`: Encrypted index operations are gated

- **Parents:** ["go.acp.nac.gates"]
- **Go-Tests:** ["acp/nac/delete_encrypted_index_test.go::TestNAC_GatesDeleteEncryptedIndex_AuthorizedIdentity_AllowAccess", "acp/nac/delete_encrypted_index_test.go::TestNAC_GatesDeleteEncryptedIndex_NoIdentity_NotAuthorizedError", "acp/nac/delete_encrypted_index_test.go::TestNAC_GatesDeleteEncryptedIndex_WrongIdentity_NotAuthorizedError", "acp/nac/list_all_encrypted_index_test.go::TestNAC_GatesListAllEncryptedIndex_AuthorizedIdentity_AllowAccess", "acp/nac/list_all_encrypted_index_test.go::TestNAC_GatesListAllEncryptedIndex_NoIdentity_NotAuthorizedError", "acp/nac/list_all_encrypted_index_test.go::TestNAC_GatesListAllEncryptedIndex_WrongIdentity_NotAuthorizedError", "acp/nac/list_encrypted_index_test.go::TestNAC_GatesListEncryptedIndex_AuthorizedIdentity_AllowAccess", "acp/nac/list_encrypted_index_test.go::TestNAC_GatesListEncryptedIndex_NoIdentity_NotAuthorizedError", "acp/nac/list_encrypted_index_test.go::TestNAC_GatesListEncryptedIndex_WrongIdentity_NotAuthorizedError", "acp/nac/new_encrypted_index_test.go::TestNAC_GatesNewEncryptedIndex_AuthorizedIdentity_AllowAccess", "acp/nac/new_encrypted_index_test.go::TestNAC_GatesNewEncryptedIndex_NoIdentity_NotAuthorizedError", "acp/nac/new_encrypted_index_test.go::TestNAC_GatesNewEncryptedIndex_WrongIdentity_NotAuthorizedError"]
- **Scope:** "authorized-identity tests run on Go, HTTP, CLI and C clients (not JS)"

With NAC enabled, creating, deleting and listing encrypted indexes on a collection succeed for the NAC owner and fail for others with permission `get-collection`; listing all encrypted indexes fails with permission `list-all-encrypted-index`.

# `go.acp.nac.gates.dac-admin`: DAC policy and relationship administration is gated

- **Parents:** ["go.acp.nac.gates"]
- **Go-Tests:** ["acp/nac/add_dac_policy_test.go::TestNAC_GatesAddDACPolicy_AuthorizedIdentity_AllowAccess", "acp/nac/add_dac_policy_test.go::TestNAC_GatesAddDACPolicy_NoIdentity_NotAuthorizedError", "acp/nac/add_dac_policy_test.go::TestNAC_GatesAddDACPolicy_WrongIdentity_NotAuthorizedError", "acp/nac/add_dac_relationship_test.go::TestNAC_GatesAddDACRelationship_AuthorizedIdentity_AllowAccess", "acp/nac/add_dac_relationship_test.go::TestNAC_GatesAddDACRelationship_NoIdentity_NotAuthorizedError", "acp/nac/add_dac_relationship_test.go::TestNAC_GatesAddDACRelationship_WrongIdentity_NotAuthorizedError", "acp/nac/delete_dac_relationship_test.go::TestNAC_GatesDeleteDACRelationship_AuthorizedIdentity_AllowAccess", "acp/nac/delete_dac_relationship_test.go::TestNAC_GatesDeleteDACRelationship_NoIdentity_NotAuthorizedError", "acp/nac/delete_dac_relationship_test.go::TestNAC_GatesDeleteDACRelationship_WrongIdentity_NotAuthorizedError"]

With NAC enabled, adding a DAC policy, adding a document actor relationship and deleting one succeed for the NAC owner and fail for others with permissions `add-dac-policy`, `add-dac-relation` and `delete-dac-relation`.

# `go.acp.nac.gates.lens`: Lens and migration operations are gated

- **Parents:** ["go.acp.nac.gates"]
- **Go-Tests:** ["acp/nac/add_lens_test.go::TestNAC_GatesAddLens_AuthorizedIdentity_AllowAccess", "acp/nac/add_lens_test.go::TestNAC_GatesAddLens_NoIdentity_NotAuthorizedError", "acp/nac/add_lens_test.go::TestNAC_GatesAddLens_WrongIdentity_NotAuthorizedError", "acp/nac/list_lens_test.go::TestNAC_GatesListLens_AuthorizedIdentity_AllowAccess", "acp/nac/list_lens_test.go::TestNAC_GatesListLens_NoIdentity_NotAuthorizedError", "acp/nac/list_lens_test.go::TestNAC_GatesListLens_WrongIdentity_NotAuthorizedError", "acp/nac/set_migration_test.go::TestNAC_GatesSetMigration_AuthorizedIdentity_AllowAccess", "acp/nac/set_migration_test.go::TestNAC_GatesSetMigration_NoIdentity_NotAuthorizedError", "acp/nac/set_migration_test.go::TestNAC_GatesSetMigration_WrongIdentity_NotAuthorizedError"]
- **Scope:** "the authorized set-migration test runs on HTTP, CLI, Go and C clients (not JS)"

With NAC enabled, adding a lens, listing lenses and setting a migration succeed for the NAC owner and fail for others with permissions `add-lens`, `list-lens` and `set-migration`.

# `go.acp.nac.gates.views`: View operations are gated

- **Parents:** ["go.acp.nac.gates"]
- **Go-Tests:** ["acp/nac/add_view_test.go::TestNAC_GatesAddView_AuthorizedIdentity_AllowAccess", "acp/nac/add_view_test.go::TestNAC_GatesAddView_NoIdentity_NotAuthorizedError", "acp/nac/add_view_test.go::TestNAC_GatesAddView_WrongIdentity_NotAuthorizedError", "acp/nac/refresh_view_test.go::TestNAC_GatesRefreshView_AuthorizedIdentity_AllowAccess", "acp/nac/refresh_view_test.go::TestNAC_GatesRefreshView_NoIdentity_NotAuthorizedError", "acp/nac/refresh_view_test.go::TestNAC_GatesRefreshView_WrongIdentity_NotAuthorizedError"]

With NAC enabled, adding a view and refreshing views succeed for the NAC owner and fail for others with permissions `add-view` and `refresh-view`.

# `go.acp.nac.gates.p2p-peers`: P2P peer operations are gated

- **Parents:** ["go.acp.nac.gates"]
- **Go-Tests:** ["acp/nac/connect_p2p_peer_test.go::TestNAC_GatesConnectP2PPeer_AuthorizedIdentity_AllowAccess", "acp/nac/connect_p2p_peer_test.go::TestNAC_GatesConnectP2PPeer_NoIdentity_NotAuthorizedError", "acp/nac/connect_p2p_peer_test.go::TestNAC_GatesConnectP2PPeer_WrongIdentity_NotAuthorizedError", "acp/nac/disconnect_p2p_peer_test.go::TestNAC_GatesDisconnectP2PPeer_AuthorizedIdentity_AllowAccess", "acp/nac/disconnect_p2p_peer_test.go::TestNAC_GatesDisconnectP2PPeer_NoIdentity_NotAuthorizedError", "acp/nac/disconnect_p2p_peer_test.go::TestNAC_GatesDisconnectP2PPeer_WrongIdentity_NotAuthorizedError", "acp/nac/get_p2p_active_peers_test.go::TestNAC_GatesGetActivePeers_AuthorizedIdentity_AllowAccess", "acp/nac/get_p2p_active_peers_test.go::TestNAC_GatesGetActivePeers_NoIdentity_NotAuthorizedError", "acp/nac/get_p2p_active_peers_test.go::TestNAC_GatesGetActivePeers_WrongIdentity_NotAuthorizedError", "acp/nac/get_p2p_peer_info_test.go::TestNAC_GatesGetP2PPeerInfo_AuthorizedIdentity_AllowAccess", "acp/nac/get_p2p_peer_info_test.go::TestNAC_GatesGetP2PPeerInfo_NoIdentity_NotAuthorizedError", "acp/nac/get_p2p_peer_info_test.go::TestNAC_GatesGetP2PPeerInfo_WrongIdentity_NotAuthorizedError"]
- **Scope:** "authorized-identity tests run on HTTP, CLI, Go and C clients (not JS)"

With NAC enabled, connecting and disconnecting peers, getting peer info and listing active peers succeed for the NAC owner and fail for others with permissions `connect-p2p-peer`, `disconnect-p2p-peer`, `get-p2p-peer-info` and `get-p2p-active-peers`.

# `go.acp.nac.gates.p2p-replicators`: P2P replicator operations are gated

- **Parents:** ["go.acp.nac.gates"]
- **Go-Tests:** ["acp/nac/add_p2p_replicator_test.go::TestNAC_GatesAddP2PReplicator_AuthorizedIdentity_AllowAccess", "acp/nac/add_p2p_replicator_test.go::TestNAC_GatesAddP2PReplicator_NoIdentity_NotAuthorizedError", "acp/nac/add_p2p_replicator_test.go::TestNAC_GatesAddP2PReplicator_WrongIdentity_NotAuthorizedError", "acp/nac/delete_p2p_replicator_test.go::TestNAC_GatesDeleteP2PReplicator_AuthorizedIdentity_AllowAccess", "acp/nac/delete_p2p_replicator_test.go::TestNAC_GatesDeleteP2PReplicator_NoIdentity_NotAuthorizedError", "acp/nac/delete_p2p_replicator_test.go::TestNAC_GatesDeleteP2PReplicator_WrongIdentity_NotAuthorizedError", "acp/nac/list_p2p_replicator_test.go::TestNAC_GatesListP2PReplicator_AuthorizedIdentity_AllowAccess", "acp/nac/list_p2p_replicator_test.go::TestNAC_GatesListP2PReplicator_NoIdentity_NotAuthorizedError", "acp/nac/list_p2p_replicator_test.go::TestNAC_GatesListP2PReplicator_WrongIdentity_NotAuthorizedError"]
- **Scope:** "the authorized add test and all delete/list tests run on HTTP, CLI, Go and C clients (not JS)"

With NAC enabled, adding, deleting and listing replicators succeed for the NAC owner and fail for others with permissions `add-p2p-replicator`, `delete-p2p-replicator` and `list-p2p-replicator`.

# `go.acp.nac.gates.p2p-collections`: P2P collection subscriptions are gated

- **Parents:** ["go.acp.nac.gates"]
- **Go-Tests:** ["acp/nac/add_p2p_collection_test.go::TestNAC_GatesAddP2PCollection_AuthorizedIdentity_AllowAccess", "acp/nac/add_p2p_collection_test.go::TestNAC_GatesAddP2PCollection_NoIdentity_NotAuthorizedError", "acp/nac/add_p2p_collection_test.go::TestNAC_GatesAddP2PCollection_WrongIdentity_NotAuthorizedError", "acp/nac/delete_p2p_collection_test.go::TestNAC_GatesDeleteP2PCollection_AuthorizedIdentity_AllowAccess", "acp/nac/delete_p2p_collection_test.go::TestNAC_GatesDeleteP2PCollection_NoIdentity_NotAuthorizedError", "acp/nac/delete_p2p_collection_test.go::TestNAC_GatesDeleteP2PCollection_WrongIdentity_NotAuthorizedError", "acp/nac/list_p2p_collection_test.go::TestNAC_GatesListP2PCollection_AuthorizedIdentity_AllowAccess", "acp/nac/list_p2p_collection_test.go::TestNAC_GatesListP2PCollection_NoIdentity_NotAuthorizedError", "acp/nac/list_p2p_collection_test.go::TestNAC_GatesListP2PCollection_WrongIdentity_NotAuthorizedError"]
- **Scope:** "HTTP, CLI, Go and C clients (not JS)"

With NAC enabled, adding, deleting and listing P2P collection subscriptions succeed for the NAC owner and fail without identity or with a wrong identity with `not authorized to perform operation. Permission: add-p2p-collection` / `delete-p2p-collection` / `list-p2p-collection`.

# `go.acp.nac.gates.p2p-documents`: P2P document subscriptions are gated

- **Parents:** ["go.acp.nac.gates"]
- **Go-Tests:** ["acp/nac/add_p2p_document_test.go::TestNAC_GatesAddP2PDocument_AuthorizedIdentity_AllowAccess", "acp/nac/add_p2p_document_test.go::TestNAC_GatesAddP2PDocument_NoIdentity_NotAuthorizedError", "acp/nac/add_p2p_document_test.go::TestNAC_GatesAddP2PDocument_WrongIdentity_NotAuthorizedError", "acp/nac/delete_p2p_document_test.go::TestNAC_GatesDeleteP2PDocument_AuthorizedIdentity_AllowAccess", "acp/nac/delete_p2p_document_test.go::TestNAC_GatesDeleteP2PDocument_NoIdentity_NotAuthorizedError", "acp/nac/delete_p2p_document_test.go::TestNAC_GatesDeleteP2PDocument_WrongIdentity_NotAuthorizedError", "acp/nac/list_p2p_document_test.go::TestNAC_GatesListP2PDocument_AuthorizedIdentity_AllowAccess", "acp/nac/list_p2p_document_test.go::TestNAC_GatesListP2PDocument_NoIdentity_NotAuthorizedError", "acp/nac/list_p2p_document_test.go::TestNAC_GatesListP2PDocument_WrongIdentity_NotAuthorizedError"]
- **Scope:** "HTTP, CLI, Go and C clients (not JS)"

With NAC enabled, adding, deleting and listing P2P document subscriptions succeed for the NAC owner and fail without identity or with a wrong identity with `not authorized to perform operation. Permission: add-p2p-document` / `delete-p2p-document` / `list-p2p-document`.

# `go.acp.nac.gates.p2p-sync`: P2P on-demand sync is gated

- **Parents:** ["go.acp.nac.gates"]
- **Go-Tests:** ["acp/nac/sync_p2p_branchable_collection_test.go::TestNAC_GatesSyncP2PBranchableCollection_AuthorizedIdentity_AllowAccess", "acp/nac/sync_p2p_branchable_collection_test.go::TestNAC_GatesSyncP2PBranchableCollection_NoIdentity_NotAuthorizedError", "acp/nac/sync_p2p_branchable_collection_test.go::TestNAC_GatesSyncP2PBranchableCollection_WrongIdentity_NotAuthorizedError", "acp/nac/sync_p2p_collection_versions_test.go::TestNAC_GatesSyncP2PCollectionVersions_AuthorizedIdentity_AllowAccess", "acp/nac/sync_p2p_collection_versions_test.go::TestNAC_GatesSyncP2PCollectionVersions_NoIdentity_NotAuthorizedError", "acp/nac/sync_p2p_collection_versions_test.go::TestNAC_GatesSyncP2PCollectionVersions_WrongIdentity_NotAuthorizedError", "acp/nac/sync_p2p_documents_test.go::TestNAC_GatesSyncP2PDocuments_AuthorizedIdentity_AllowAccess", "acp/nac/sync_p2p_documents_test.go::TestNAC_GatesSyncP2PDocuments_NoIdentity_NotAuthorizedError", "acp/nac/sync_p2p_documents_test.go::TestNAC_GatesSyncP2PDocuments_WrongIdentity_NotAuthorizedError"]
- **Scope:** "authorized-identity tests run on HTTP, CLI, Go and C clients (not JS)"

With NAC enabled, syncing documents, collection versions and branchable collections from peers succeed for the NAC owner and fail without identity or with a wrong identity with `not authorized to perform operation. Permission: sync-p2p-documents` / `sync-p2p-collection-versions` / `sync-p2p-branchable-collection`.

# `go.acp.nac.dac`: Interplay of NAC and DAC

- **Parents:** ["go.acp.nac"]

NAC is checked before DAC; when NAC is off, DAC alone decides document visibility.

# `go.acp.nac.dac.nac-on`: With NAC on, only authorized identities reach DAC

- **Parents:** ["go.acp.nac.dac"]
- **Go-Tests:** ["acp/nac/dac_access_by_empty_user_nac_on_test.go::TestNAC_WithDACEnabled_AccessByEmptyUser_PrivateDocumentOwnedByNodeOwner_NotAuthorizedError", "acp/nac/dac_access_by_empty_user_nac_on_test.go::TestNAC_WithDACEnabled_AccessByEmptyUser_PrivateDocumentOwnedByNonNodeOwner_NotAuthorizedError", "acp/nac/dac_access_by_empty_user_nac_on_test.go::TestNAC_WithDACEnabled_AccessEmptyUser_PublicDocument_NotAuthorizedError", "acp/nac/dac_access_by_node_owner_nac_on_test.go::TestNAC_WithDACEnabled_AccessByNodeOwner_DoesNotOwnTheDocument_AllowAccess", "acp/nac/dac_access_by_node_owner_nac_on_test.go::TestNAC_WithDACEnabled_AccessByNodeOwner_OwnsTheDocument_AllowAccess", "acp/nac/dac_access_by_node_owner_nac_on_test.go::TestNAC_WithDACEnabled_AccessByNodeOwner_PublicDocument_AllowAccess", "acp/nac/dac_access_by_wrong_user_nac_on_test.go::TestNAC_WithDACEnabled_AccessByNonNodeOwner_OwnsTheDocument_NotAuthorizedError", "acp/nac/dac_access_by_wrong_user_nac_on_test.go::TestNAC_WithDACEnabled_AccessByNonNodeOwner_DoesNotOwnTheDocument_NotAuthorizedError", "acp/nac/dac_access_by_wrong_user_nac_on_test.go::TestNAC_WithDACEnabled_AccessByNonNodeOwner_PublicDocument_NotAuthorizedError"]

With NAC and DAC enabled, the NAC owner reads public documents, its own private documents and private documents owned by others (node ownership bypasses DAC). An anonymous user or a non-owner identity is refused before DAC with `not authorized to perform operation. Permission: get-collection`, even for public documents or documents it owns.

# `go.acp.nac.dac.nac-off`: With NAC disabled, DAC alone decides

- **Parents:** ["go.acp.nac.dac"]
- **Go-Tests:** ["acp/nac/dac_access_by_empty_user_nac_off_test.go::TestNAC_Disabled_WithDACEnabled_AccessByEmptyUser_PrivateDocumentOwnedByNodeOwner_CanNotAccess", "acp/nac/dac_access_by_empty_user_nac_off_test.go::TestNAC_Disabled_WithDACEnabled_AccessByEmptyUser_PrivateDocumentOwnedByNonNodeOwner_CanNotAccess", "acp/nac/dac_access_by_empty_user_nac_off_test.go::TestNAC_Disabled_WithDACEnabled_AccessEmptyUser_PublicDocument_CanAccess", "acp/nac/dac_access_by_owner_nac_off_test.go::TestNAC_Disabled_WithDACEnabled_AccessByNodeOwner_DoesNotOwnTheDocument_CanNotAccess", "acp/nac/dac_access_by_owner_nac_off_test.go::TestNAC_Disabled_WithDACEnabled_AccessByNodeOwner_OwnsTheDocument_CanAccess", "acp/nac/dac_access_by_owner_nac_off_test.go::TestNAC_Disabled_WithDACEnabled_AccessByNodeOwner_PublicDocument_CanAccess", "acp/nac/dac_access_by_wrong_user_nac_off_test.go::TestNAC_Disabled_WithDACEnabled_AccessByNonNodeOwner_OwnsTheDocument_CanAccess", "acp/nac/dac_access_by_wrong_user_nac_off_test.go::TestNAC_Disabled_WithDACEnabled_AccessByNonNodeOwner_DoesNotOwnTheDocument_CanNotAccess", "acp/nac/dac_access_by_wrong_user_nac_off_test.go::TestNAC_Disabled_WithDACEnabled_AccessByNonNodeOwner_PublicDocument_CanAccess"]

With NAC temporarily disabled, anonymous users, the node owner and other identities read public documents and private documents they own, and get an empty result (no error) for private documents owned by someone else; the node owner gets no DAC bypass.

# `go.acp.nac.admin`: The NAC admin relation

- **Parents:** ["go.acp.nac"]

An identity granted the NAC `admin` relation by the owner gains every node permission; without it the same call fails with the operation's permission error.

# `go.acp.nac.admin.dac-bypass`: Admins bypass DAC only while NAC is enabled

- **Parents:** ["go.acp.nac.admin"]
- **Go-Tests:** ["acp/nac/relation_admin/dac_bypass_with_nac_off_test.go::TestNAC_Disabled_AdminRelation_DoesNotOwnTheDocument_CanNotAccessAndCanNotDACBypass", "acp/nac/relation_admin/dac_bypass_with_nac_off_test.go::TestNAC_Disabled_AdminRelation_OwnThePrivateDocument_CanAccessButNotDACBypass", "acp/nac/relation_admin/dac_bypass_with_nac_off_test.go::TestNAC_Disabled_AdminRelation_PublicDocument_CanAccessButNotDACBypass", "acp/nac/relation_admin/dac_bypass_with_nac_on_test.go::TestNAC_AdminRelation_DoesNotOwnTheDocument_CanBypassDAC", "acp/nac/relation_admin/dac_bypass_with_nac_on_test.go::TestNAC_AdminRelation_OwnThePrivateDocument_CanBypassDAC", "acp/nac/relation_admin/dac_bypass_with_nac_on_test.go::TestNAC_AdminRelation_PublicDocument_CanAccessPublicDocument", "acp/nac/relation_admin/dac_bypass_with_nac_on_test.go::TestNAC_AdminRelation_DACByPassRevokation_CanNotDACBypass"]

With NAC enabled, an identity is denied (permission `get-collection`) until granted `admin`, then reads public documents, its own private documents and others' private documents; after the admin relation is revoked it no longer bypasses DAC. With NAC disabled, an admin reads only public and its own documents and gets an empty result for others' private documents.

# `go.acp.nac.admin.nac-admin`: Admins manage NAC itself

- **Parents:** ["go.acp.nac.admin"]
- **Go-Tests:** ["acp/nac/relation_admin/add_nac_relationship_test.go::TestNAC_AdminRelation_CanAddNACRelationship", "acp/nac/relation_admin/delete_nac_relationship_test.go::TestNAC_AdminRelation_CanDeleteNACRelationship", "acp/nac/relation_admin/disable_nac_test.go::TestNAC_AdminRelation_CanDisableNAC", "acp/nac/relation_admin/get_nac_status_test.go::TestNAC_AdminRelation_CanGetNACStatus", "acp/nac/relation_admin/re_enable_nac_test.go::TestNAC_AdminRelation_CanReEnableNAC"]

An identity granted `admin` can get NAC status, add and delete NAC relationships, disable NAC and re-enable it; before the grant each call fails with permissions `get-nac-status`, `add-nac-relation`, `delete-nac-relation`, `disable-nac` and `re-enable-nac`.

# `go.acp.nac.admin.dac-relationships`: Admins still need DAC manager rights for document relationships

- **Parents:** ["go.acp.nac.admin"]
- **Go-Tests:** ["acp/nac/relation_admin/add_dac_relationship_test.go::TestNAC_AdminRelation_WithDACManagerRelation_CanAddDACActorRelationship", "acp/nac/relation_admin/add_dac_relationship_test.go::TestNAC_AdminRelation_WithoutManagerDACRelation_CanNotAddDACActorRelationship", "acp/nac/relation_admin/delete_dac_relationship_test.go::TestNAC_AdminRelation_WithDACManagerRelation_CanDeleteDACActorRelationship", "acp/nac/relation_admin/delete_dac_relationship_test.go::TestNAC_AdminRelation_WithoutDACManagerRelation_CanNotDeleteDACActorRelationship"]

A NAC admin passes the `add-dac-relation` / `delete-dac-relation` gate, but adding or deleting a document actor relationship succeeds only if it also manages the relation in the DAC policy; otherwise it fails with `cannot create relationship: actor is not a manager of relation` / `cannot delete relationship: actor is not a manager of relation`.

# `go.acp.nac.admin.collections`: Admins operate on collections

- **Parents:** ["go.acp.nac.admin"]
- **Go-Tests:** ["acp/nac/relation_admin/add_collection_test.go::TestNAC_AdminRelation_CanAddCollection", "acp/nac/relation_admin/delete_collection_test.go::TestNAC_AdminRelation_CanDeleteCollection", "acp/nac/relation_admin/get_collection_by_id_test.go::TestNAC_AdminRelation_CanGetCollectionByID", "acp/nac/relation_admin/get_collection_by_name_test.go::TestNAC_AdminRelation_CanGetCollectionByName", "acp/nac/relation_admin/get_collection_by_version_test.go::TestNAC_AdminRelation_CanGetCollectionByVersion", "acp/nac/relation_admin/list_collection_test.go::TestNAC_AdminRelation_CanListCollection", "acp/nac/relation_admin/patch_collection_test.go::TestNAC_AdminRelation_CanPatchCollection", "acp/nac/relation_admin/set_active_collection_version_test.go::TestNAC_AdminRelation_CanSetActiveCollectionVersion", "acp/nac/relation_admin/truncate_collection_test.go::TestNAC_AdminRelation_CanTruncateCollection"]

After being granted `admin`, an identity can add, delete, patch, get (by ID, name, version), list and truncate collections and set the active collection version; before the grant these fail with permissions `patch-collection` or `get-collection`. Getting an unknown version returns `collection not found`.

# `go.acp.nac.admin.documents`: Admins operate on documents and signatures

- **Parents:** ["go.acp.nac.admin"]
- **Go-Tests:** ["acp/nac/relation_admin/add_document_test.go::TestNAC_AdminRelation_CanAddDocument", "acp/nac/relation_admin/delete_document_test.go::TestNAC_AdminRelation_CanDeleteDocument", "acp/nac/relation_admin/read_document_test.go::TestNAC_AdminRelation_CanReadDocument", "acp/nac/relation_admin/update_document_test.go::TestNAC_AdminRelation_CanUpdateDocument", "acp/nac/relation_admin/verify_signature_test.go::TestNAC_AdminRelation_CanVerifySignature", "acp/nac/relation_admin/verify_signature_test.go::TestNAC_AdminRelation_GoClient_CanVerifySignature"]
- **Scope:** "the signature tests run on HTTP and CLI clients (default signer) or Go and C clients (signed documents), one test each"

After being granted `admin`, an identity can add, read, update and delete documents and verify block signatures; before the grant these fail with permissions `get-collection` and `verify-signature`.

# `go.acp.nac.admin.indexes`: Admins operate on indexes

- **Parents:** ["go.acp.nac.admin"]
- **Go-Tests:** ["acp/nac/relation_admin/delete_encrypted_index_test.go::TestNAC_AdminRelation_CanDeleteEncryptedIndex", "acp/nac/relation_admin/delete_index_test.go::TestNAC_AdminRelation_CanDeleteIndex", "acp/nac/relation_admin/list_all_encrypted_index_test.go::TestNAC_AdminRelation_CanListAllEncryptedIndex", "acp/nac/relation_admin/list_encrypted_index_test.go::TestNAC_AdminRelation_CanListEncryptedIndex", "acp/nac/relation_admin/list_index_test.go::TestNAC_AdminRelation_CanListIndex", "acp/nac/relation_admin/new_encrypted_index_test.go::TestNAC_AdminRelation_CanNewEncryptedIndex", "acp/nac/relation_admin/new_index_test.go::TestNAC_AdminRelation_CanMakeNewIndex"]

After being granted `admin`, an identity can create, delete and list indexes and encrypted indexes and list all encrypted indexes; before the grant these fail with permissions `get-collection` or `list-all-encrypted-index`.

# `go.acp.nac.admin.schema-tools`: Admins manage policies, lenses, migrations and views

- **Parents:** ["go.acp.nac.admin"]
- **Go-Tests:** ["acp/nac/relation_admin/add_dac_policy_test.go::TestNAC_AdminRelation_CanAddDACPolicy", "acp/nac/relation_admin/add_lens_test.go::TestNAC_AdminRelation_CanAddLens", "acp/nac/relation_admin/add_view_test.go::TestNAC_AdminRelation_CanAddView", "acp/nac/relation_admin/list_lens_test.go::TestNAC_AdminRelation_CanListLenses", "acp/nac/relation_admin/refresh_view_test.go::TestNAC_AdminRelation_CanRefreshViews", "acp/nac/relation_admin/set_migration_test.go::TestNAC_AdminRelation_CanSetMigration"]

After being granted `admin`, an identity can add DAC policies, add and list lenses, set migrations, add views and refresh views; before the grant these fail with permissions `add-dac-policy`, `add-lens`, `list-lens`, `set-migration`, `add-view` and `refresh-view`.

# `go.acp.nac.admin.p2p`: Admins operate P2P

- **Parents:** ["go.acp.nac.admin"]
- **Go-Tests:** ["acp/nac/relation_admin/add_p2p_collection_test.go::TestNAC_AdminRelation_CanAddP2PCollection", "acp/nac/relation_admin/add_p2p_document_test.go::TestNAC_AdminRelation_CanAddP2PDocument", "acp/nac/relation_admin/add_p2p_replicator_test.go::TestNAC_AdminRelation_CanAddP2PReplicator", "acp/nac/relation_admin/connect_p2p_peer_test.go::TestNAC_AdminRelation_CanConnectP2PPeer", "acp/nac/relation_admin/delete_p2p_collection_test.go::TestNAC_AdminRelation_CanDeleteP2PCollection", "acp/nac/relation_admin/delete_p2p_document_test.go::TestNAC_AdminRelation_CanDeleteP2PDocument", "acp/nac/relation_admin/delete_p2p_replicator_test.go::TestNAC_AdminRelation_CanDeleteP2PReplicator", "acp/nac/relation_admin/get_p2p_active_peers_test.go::TestNAC_AdminRelation_CanGetActivePeers", "acp/nac/relation_admin/get_p2p_peer_info_test.go::TestNAC_AdminRelation_CanGetP2PPeerInfo", "acp/nac/relation_admin/list_p2p_collection_test.go::TestNAC_AdminRelation_CanListP2PCollection", "acp/nac/relation_admin/list_p2p_document_test.go::TestNAC_AdminRelation_CanListP2PDocument", "acp/nac/relation_admin/list_p2p_replicator_test.go::TestNAC_AdminRelation_CanListP2PReplicator", "acp/nac/relation_admin/sync_p2p_branchable_collection_test.go::TestNAC_AdminRelation_CanSyncP2PBranchableCollection", "acp/nac/relation_admin/sync_p2p_collection_versions_test.go::TestNAC_AdminRelation_CanSyncP2PCollectionVersions", "acp/nac/relation_admin/sync_p2p_documents_test.go::TestNAC_AdminRelation_CanSyncP2PDocuments"]
- **Scope:** "HTTP, CLI, Go and C clients (not JS)"

After being granted `admin`, an identity can connect peers, get peer info and active peers, add/delete/list replicators, P2P collections and P2P documents, and sync documents, collection versions and branchable collections; before the grant each fails with its P2P permission error.
