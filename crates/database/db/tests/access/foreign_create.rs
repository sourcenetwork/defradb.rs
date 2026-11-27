//! A create in a gated collection whose policy another deployment owns
//! (`reqs/defra/acp.md` `defra.acp.foreign_policy`): the owner's home
//! registers its documents, so the node refuses the create before it reads
//! the policy or decides `create` anywhere, and writes nothing.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use acp::policy_yaml::{build_policy, parse_policy_yaml};
use acp::{MemoryZanzibarStore, StorePolicyOptions, ZanzibarStore as _};
use db::DacPolicies;
use query::QueryExecutor as _;
use storage::RegolithStore;

use super::external_registration::{alice, gate, node, readable, request, RemoteAcp, POLICY};

/// The policy the node holds: `book` gates creates on `author->publish`.
const LIBRARY: &str = r#"
name: library
resources:
  - name: author
    relations:
      - name: publisher
        types: [actor]
    permissions:
      - name: read
        expr: publisher
      - name: update
        expr: publisher
      - name: delete
        expr: publisher
      - name: publish
        expr: publisher
  - name: book
    relations:
      - name: author
        types: [author]
    permissions:
      - name: read
        expr: author->read
      - name: update
        expr: author->update
      - name: delete
        expr: author->delete
    collection:
      create: author->publish
"#;

const SCHEMA: &str = r#"
    type Author @policy(id: "remote-policy", resource: "author") { name: String }
    type Book @governed(root: "library") @policy(id: "remote-policy", resource: "book") {
        title: String
        author: Author @immutable @policy(relation: "author")
    }
"#;

/// A gated create under a routed policy fails with the foreign-policy
/// refusal: no `create` decision is asked, nothing is registered, and no
/// book is written.
#[test]
#[mantra_macros::req_test("defra.acp.foreign_policy")]
fn a_gated_create_under_a_routed_policy_is_refused_before_anything_is_decided() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let remote = RemoteAcp::new();
        let node = node(store.clone(), gate(&remote, store), true).await;
        let held = Arc::new(MemoryZanzibarStore::new());
        let mut library = build_policy(&parse_policy_yaml(LIBRARY).unwrap(), 1).unwrap();
        library.id = POLICY.into();
        held.store_policy_with_options(&library, &StorePolicyOptions::new())
            .await
            .unwrap();
        node.db.set_dac_policies(Arc::new(DacPolicies::local(held)));
        node.db
            .add_collection(&defra_core::RequestContext::ambient(), SCHEMA)
            .await
            .unwrap();
        assert!(readable(&node.runner, Some(alice())).await.is_empty());
        assert!(
            node.db
                .dac_policies()
                .unwrap()
                .get_local(POLICY)
                .await
                .unwrap()
                .is_some_and(|policy| policy.resources.iter().any(|r| r.name == "book")),
            "the gated policy is held"
        );

        remote.foreign.store(true, Ordering::SeqCst);
        let response = node
            .runner
            .execute(request(
                r#"mutation { add_Book(input: {title: "Dune", author: "bae-0a24b4e4-c1f7-5d2b-9c0e-6c1e1b3c9f01"}) { _docID } }"#,
                Some(alice()),
            ))
            .await;
        let refused = format!(
            "policy {POLICY} is owned by another deployment: creating a document is made on its owner's home"
        );
        assert!(
            response.errors.iter().any(|error| error.message.contains(&refused)),
            "{:?}",
            response.errors
        );
        assert_eq!(remote.named_checks.load(Ordering::SeqCst), 0, "no create decision");
        assert_eq!(remote.submissions.load(Ordering::SeqCst), 0, "nothing registered");

        remote.foreign.store(false, Ordering::SeqCst);
        let books = node
            .runner
            .execute(request("{ Book { _docID } }", Some(alice())))
            .await;
        assert!(!books.has_errors(), "{:?}", books.errors);
        assert!(
            books.data.unwrap()["Book"].as_array().unwrap().is_empty(),
            "no book is written"
        );
    })
    .unwrap()
}
