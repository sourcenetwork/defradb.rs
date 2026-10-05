use super::*;

#[derive(Debug, Clone, Copy)]
enum Mutation {
    AddActor,
    DeleteActor,
    AddUserset,
    DeleteUserset,
    Register,
    Archive,
}

#[tokio::test]
async fn mutations_invalidate_inherited_grants_but_not_other_policies() {
    for mutation in [
        Mutation::AddActor,
        Mutation::DeleteActor,
        Mutation::AddUserset,
        Mutation::DeleteUserset,
        Mutation::Register,
        Mutation::Archive,
    ] {
        let provider = Arc::new(MockProvider::new(vec![true, true, false]));
        let acp = VeraDocumentACP::new(provider.clone(), Duration::from_secs(300));
        let identity = Identity::from(requestor());
        for policy in ["changed", "unrelated"] {
            assert!(acp
                .check_doc_access(
                    &identity,
                    DocumentPermission::Read,
                    policy,
                    "files",
                    "child"
                )
                .await
                .unwrap());
        }
        match mutation {
            Mutation::AddActor => {
                acp.add_actor_relationship(
                    &requestor(),
                    &requestor(),
                    "changed",
                    "folders",
                    "parent",
                    "reader",
                    &[],
                )
                .await
                .unwrap();
            }
            Mutation::DeleteActor => {
                acp.delete_actor_relationship(
                    &requestor(),
                    &requestor(),
                    "changed",
                    "folders",
                    "parent",
                    "reader",
                    &[],
                )
                .await
                .unwrap();
            }
            Mutation::AddUserset => {
                acp.add_relationship(
                    &requestor(),
                    Subject::entity_set("groups", "team", "member"),
                    "changed",
                    "folders",
                    "parent",
                    "reader",
                    &[],
                )
                .await
                .unwrap();
            }
            Mutation::DeleteUserset => {
                acp.delete_relationship(
                    &requestor(),
                    Subject::entity_set("groups", "team", "member"),
                    "changed",
                    "folders",
                    "parent",
                    "reader",
                    &[],
                )
                .await
                .unwrap();
            }
            Mutation::Register => {
                acp.register_doc_object(&requestor(), "changed", "folders", "parent")
                    .await
                    .unwrap();
            }
            Mutation::Archive => {
                acp.unregister_doc_object("changed", "folders", "parent")
                    .await
                    .unwrap();
            }
        }
        assert!(
            !acp.check_doc_access(
                &identity,
                DocumentPermission::Read,
                "changed",
                "files",
                "child"
            )
            .await
            .unwrap(),
            "stale inherited grant after {mutation:?}"
        );
        assert!(
            acp.check_doc_access(
                &identity,
                DocumentPermission::Read,
                "unrelated",
                "files",
                "child"
            )
            .await
            .unwrap(),
            "unrelated policy invalidated after {mutation:?}"
        );
        assert_eq!(provider.verify_calls(), 3, "{mutation:?}");
    }
}
