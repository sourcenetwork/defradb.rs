use super::*;

#[tokio::test]
async fn revocation_during_remote_check_cannot_restore_the_grant() {
    let started = Arc::new(tokio::sync::Notify::new());
    let resume = Arc::new(tokio::sync::Semaphore::new(0));
    let mut provider = MockProvider::new(vec![true, false]);
    provider.verify_gate = Some((started.clone(), resume.clone()));
    let provider = Arc::new(provider);
    let acp = VeraDocumentACP::new(provider.clone(), Duration::from_secs(300));
    let identity = Identity::from(requestor());
    let check = acp.check_doc_access(
        &identity,
        DocumentPermission::Read,
        "policy",
        "files",
        "doc",
    );
    tokio::pin!(check);
    tokio::select! {
        _ = started.notified() => {}
        result = &mut check => panic!("check completed before release: {result:?}"),
    }
    acp.delete_actor_relationship(
        &requestor(),
        &requestor(),
        "policy",
        "files",
        "doc",
        "reader",
        &[],
    )
    .await
    .unwrap();
    resume.add_permits(1);
    assert!(check.await.unwrap());
    assert!(!acp
        .check_doc_access(
            &identity,
            DocumentPermission::Read,
            "policy",
            "files",
            "doc"
        )
        .await
        .unwrap());
    assert_eq!(provider.verify_calls(), 2);
}
