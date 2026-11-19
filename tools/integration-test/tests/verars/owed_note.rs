//! The outcome of an owed request whose caller knows it by its digest alone,
//! when another command resolves it from the journal.

use vera::{
    owed_request_digest, OwedCall, OwedRequestOutcome, PolicyAction, PolicyCommand, PolicySubject,
    SubjectRef, VeraProvider,
};

use super::embedded::{self, Embedded};

const POLICY: &str = "\
name: owed note
resources:
  - name: file
    relations:
      - name: reader
        types: [actor]
    permissions:
      - name: read
        expr: reader
";
const READER: &str = "did:key:z6MkfXG2FkNy3u7Eg3jm8e2YQpGz7Z1JqWgHDAP1hLk9r2bR";

/// The drainer journals a request and stops before storing its hash. A
/// command with the same bytes (the same delegation, the same grant) then
/// finds it in the journal and sends it as its own. The request's outcome
/// is noted under its digest, so the drainer learns the grant applied
/// instead of sending it again.
#[nuclei::test]
#[serial_test::serial]
async fn a_command_that_resolves_a_journaled_request_with_its_bytes_notes_the_outcome() {
    edge::compat(async {
        let node = Embedded::start().await;
        let root = tempfile::tempdir().unwrap();
        let provider = node
            .in_process(embedded::worker(root.path()))
            .await
            .expect("provider");
        let owner = provider.self_did().unwrap();
        let policy = provider.create_policy(POLICY).await.expect("policy");
        let bearer = provider.create_bearer_token(&owner).await.unwrap();
        provider
            .register_object(&bearer, &policy, "file", "report")
            .await
            .expect("register object");

        let reader = SubjectRef::Actor(READER.into());
        let request = provider
            .owed_request(&[OwedCall {
                bearer_token: &bearer,
                command: PolicyCommand {
                    policy_id: &policy,
                    resource: "file",
                    object_id: "report",
                    action: PolicyAction::SetRelationship {
                        relation: "reader",
                        subject: PolicySubject::Actor(&reader),
                    },
                },
            }])
            .expect("owed request");
        let floor = provider.owed_request_floor().await.expect("floor");
        provider
            .journal_owed_request(&request)
            .await
            .expect("journaled");

        provider
            .set_relationship(&bearer, &policy, "file", "report", "reader", &reader)
            .await
            .expect("the same grant, sent directly");
        assert!(provider
            .verify_access(&policy, "file", "report", "read", READER)
            .await
            .expect("proven read"));

        let outcome = provider
            .owed_request_outcome(&owed_request_digest(&request), None, Some(floor))
            .await
            .expect("outcome");
        assert_eq!(outcome, OwedRequestOutcome::Applied);
        node.stop().await;
    })
    .await;
}
