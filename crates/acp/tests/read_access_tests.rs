use kovan::Atom;
use std::sync::Arc;

use acp::read_access::{check_doc_read_access, DocAccess, ObjectAccessChecker};
use async_trait::async_trait;

struct FakeChecker {
    doc_access: DocAccess,
    collection_access: DocAccess,
    calls: Arc<Atom<Vec<String>>>,
}

impl FakeChecker {
    fn new(doc_access: DocAccess, collection_access: DocAccess) -> Self {
        Self {
            doc_access,
            collection_access,
            calls: Arc::new(Atom::new(Vec::new())),
        }
    }

    fn calls(&self) -> Vec<String> {
        self.calls.load_clone()
    }
}

#[async_trait]
impl ObjectAccessChecker for FakeChecker {
    async fn object_access(
        &self,
        _policy_id: &str,
        _resource_name: &str,
        object_id: &str,
    ) -> acp::Result<DocAccess> {
        self.calls.rcu(|calls| {
            let mut next = calls.clone();
            next.push(object_id.to_string());
            next
        });
        Ok(if object_id == "col1" {
            self.collection_access
        } else {
            self.doc_access
        })
    }
}

#[tokio::test]
async fn branchable_public_doc_requires_collection_read() {
    let checker = FakeChecker::new(
        DocAccess {
            has_access: true,
            explicit: false,
        },
        DocAccess {
            has_access: false,
            explicit: true,
        },
    );

    let allowed = check_doc_read_access(&checker, "policy1", "resource1", "col1", true, "doc1")
        .await
        .unwrap();

    assert!(!allowed);
    assert_eq!(checker.calls(), vec!["doc1", "col1"]);
}

#[tokio::test]
async fn branchable_explicit_doc_grant_wins_over_collection_denial() {
    let checker = FakeChecker::new(
        DocAccess {
            has_access: true,
            explicit: true,
        },
        DocAccess {
            has_access: false,
            explicit: true,
        },
    );

    let allowed = check_doc_read_access(&checker, "policy1", "resource1", "col1", true, "doc1")
        .await
        .unwrap();

    assert!(allowed);
    assert_eq!(checker.calls(), vec!["doc1"]);
}

#[tokio::test]
async fn non_branchable_public_doc_does_not_check_collection() {
    let checker = FakeChecker::new(
        DocAccess {
            has_access: true,
            explicit: false,
        },
        DocAccess {
            has_access: false,
            explicit: true,
        },
    );

    let allowed = check_doc_read_access(&checker, "policy1", "resource1", "col1", false, "doc1")
        .await
        .unwrap();

    assert!(allowed);
    assert_eq!(checker.calls(), vec!["doc1"]);
}

#[tokio::test]
async fn branchable_collection_level_commit_checks_collection_object() {
    let checker = FakeChecker::new(
        DocAccess {
            has_access: false,
            explicit: true,
        },
        DocAccess {
            has_access: true,
            explicit: true,
        },
    );

    let allowed = check_doc_read_access(&checker, "policy1", "resource1", "col1", true, "")
        .await
        .unwrap();

    assert!(allowed);
    assert_eq!(checker.calls(), vec!["col1"]);
}

#[tokio::test]
async fn unregistered_private_document_requires_a_branchable_collection_grant() {
    for branchable in [false, true] {
        for collection_allowed in [false, true] {
            let checker = FakeChecker::new(
                DocAccess {
                    has_access: false,
                    explicit: false,
                },
                DocAccess {
                    has_access: collection_allowed,
                    explicit: true,
                },
            );
            assert_eq!(
                check_doc_read_access(&checker, "policy1", "resource1", "col1", branchable, "doc1")
                    .await
                    .unwrap(),
                branchable && collection_allowed,
            );
            assert_eq!(
                checker.calls(),
                if branchable {
                    vec!["doc1", "col1"]
                } else {
                    vec!["doc1"]
                }
            );
        }
    }
}

#[tokio::test]
async fn local_unregistered_document_keeps_public_read_access() {
    use acp::{DirectChecker, DocumentACP, Identity, LocalDocumentACP, MemoryAcpStore};
    let acp = LocalDocumentACP::new(Arc::new(MemoryAcpStore::new()));
    assert!(acp.unregistered_documents_are_public());
    let checker = DirectChecker {
        acp: &acp,
        identity: &Identity::Anonymous,
    };
    let access = checker
        .object_access("policy", "resource", "doc")
        .await
        .unwrap();
    assert!(access.has_access);
    assert!(!access.explicit);
    assert!(
        check_doc_read_access(&checker, "policy", "resource", "collection", false, "doc")
            .await
            .unwrap()
    );
}
