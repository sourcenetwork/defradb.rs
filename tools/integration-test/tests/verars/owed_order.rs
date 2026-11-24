//! The order two owed changes of one relationship reach a Vera node in when
//! the first is still on its way as the second is submitted.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use acp::{DocumentACP as _, DocumentPermission, Identity, OwedScope, OwedSettlement};
use defra_core::thread_bounds::MaybeBoxFuture;
use defra_node::{DocumentAcpProvider, Node, QueryRequest, RelationshipChange};
use regolith::sync::{Event, SegQueue};
use vera::vera_rs::{ClientError, RpcTransport};
use vera::{AcpTuning, VeraDocumentACP, VeraProvider};

use super::embedded::{self, Embedded};

const SEND: &str = "vera_sendNativeTx";
const READER: &str = "did:key:z6MkfXG2FkNy3u7Eg3jm8e2YQpGz7Z1JqWgHDAP1hLk9r2bR";
const POLICY: &str = "\
name: owed order
resources:
  - name: users
    permissions:
      - name: read
        expr: writer + reader
      - name: update
        expr: writer
      - name: delete
        expr: writer
    relations:
      - name: writer
        types:
          - actor
      - name: reader
        types:
          - actor
";

/// The node's channel with one submission held on its way.
struct Stalling {
    node: Arc<dyn RpcTransport>,
    /// The next submission is held until `released`.
    stall: AtomicBool,
    released: Event,
    /// The held submission, as it was sent.
    held: SegQueue<serde_json::Value>,
    /// How many submissions reached the node.
    arrived: AtomicUsize,
}

impl RpcTransport for Stalling {
    fn call<'a>(
        &'a self,
        method: &'a str,
        params: serde_json::Value,
        max_bytes: usize,
    ) -> MaybeBoxFuture<'a, Result<serde_json::Value, ClientError>> {
        Box::pin(async move {
            if method == SEND {
                if self.stall.swap(false, Ordering::SeqCst) {
                    self.held.push(params.clone());
                    self.released.wait().await;
                }
                self.arrived.fetch_add(1, Ordering::SeqCst);
            }
            self.node.call(method, params, max_bytes).await
        })
    }
}

/// A grant whose submission is still on its way when its commit stops
/// waiting, and a revocation of the same relationship committed after it.
/// The revocation is not sent before the grant has executed, so the grant
/// cannot land last: once both are settled the Vera node does not hold the
/// relationship, and the grant's own request delivered once more changes
/// nothing.
#[nuclei::test]
#[serial_test::serial]
async fn a_grant_still_in_flight_cannot_land_after_the_revocation_that_follows_it() {
    edge::compat(async {
        let vera_node = Embedded::start().await;
        let root = tempfile::tempdir().unwrap();
        // The confirmation wait outlasts both commits' waits, so the held
        // submission is still in flight when it is released.
        let tuning = AcpTuning {
            receipt_timeout: Duration::from_secs(120),
            ..AcpTuning::default()
        };
        let mut channel = None;
        let provider = vera_node
            .in_process_behind(
                embedded::worker(&root.path().join("node")),
                &tuning,
                |node| {
                    let stalling = Arc::new(Stalling {
                        node,
                        stall: AtomicBool::new(false),
                        released: Event::new(),
                        held: SegQueue::new(),
                        arrived: AtomicUsize::new(0),
                    });
                    channel = Some(stalling.clone());
                    stalling
                },
            )
            .await
            .expect("provider");
        let channel = channel.expect("the channel");
        let provider: Arc<dyn VeraProvider> = Arc::new(provider);
        let node = Node::builder()
            .with_document_acp_provider(DocumentAcpProvider::new(provider))
            .build()
            .await
            .expect("node");
        // What the Vera node holds, read by a provider that keeps no answer.
        let chain = VeraDocumentACP::without_access_cache(Arc::new(
            vera_node
                .in_process(embedded::worker(&root.path().join("reader")))
                .await
                .expect("reading provider"),
        ));

        let vera = node.vera_acp().expect("vera.rs document ACP").clone();
        let owner = identity::Did::new(vera.self_did().expect("actor DID")).unwrap();
        let reader = identity::Did::new(READER).unwrap();
        let policy = vera
            .add_policy(owner.as_str(), POLICY, None)
            .await
            .expect("policy");
        drop(vera);
        node.add_collection(&format!(
            "type Users @policy(id: \"{policy}\", resource: \"users\") {{ name: String }}"
        ))
        .await
        .expect("collection");
        let created = node
            .execute_request(
                QueryRequest::new("mutation { add_Users(input: {name: \"shared\"}) { _docID } }")
                    .with_identity(Some(owner.clone())),
            )
            .await;
        assert!(!created.has_errors(), "{:?}", created.errors);
        let doc = created.data.expect("data")["add_Users"][0]["_docID"]
            .as_str()
            .expect("document ID")
            .to_owned();
        let reads = || async {
            chain
                .check_doc_access(
                    &Identity::Authenticated(reader.clone()),
                    DocumentPermission::Read,
                    &policy,
                    "users",
                    &doc,
                )
                .await
                .expect("access decision")
        };
        let acting = defra_core::RequestContext::default().with_identity(Some(owner.to_string()));

        // The grant: its submission is held, and the commit stops waiting.
        let before = channel.arrived.load(Ordering::SeqCst);
        channel.stall.store(true, Ordering::SeqCst);
        let transaction = node.begin_transaction_guard(false).await.unwrap();
        let staged = node
            .add_dac_actor_relationship_change_for_in_txn(
                transaction.handle(),
                &acting,
                "Users",
                &doc,
                "reader",
                reader.as_str(),
            )
            .await
            .expect("staged grant");
        assert!(
            matches!(staged, RelationshipChange::Staged { .. }),
            "{staged:?}"
        );
        transaction.commit().await.expect("commit");
        assert_eq!(channel.held.len(), 1, "the grant was sent and is held");
        assert!(!reads().await);

        // The revocation of the same relationship, committed while the grant
        // is in flight. Nothing reaches the Vera node for it.
        let transaction = node.begin_transaction_guard(false).await.unwrap();
        node.delete_dac_actor_relationship_change_for_in_txn(
            transaction.handle(),
            &acting,
            "Users",
            &doc,
            "reader",
            reader.as_str(),
        )
        .await
        .expect("staged revocation");
        transaction.commit().await.expect("commit");
        assert_eq!(
            channel.arrived.load(Ordering::SeqCst),
            before,
            "nothing is sent past a submission still in flight"
        );
        assert!(!reads().await);

        // The grant lands now, after the revocation was committed and its
        // pass ran. The node's drainer settles what it can by itself, and
        // the passes asked for here settle the rest: whichever pass takes
        // each, both end settled, with nothing owed or discarded left.
        channel.released.set();
        let owed = || async {
            node.owed_acp_as(&acting, owner.as_str())
                .await
                .expect("what is owed")
        };
        for _ in 0..60 {
            let pass = node
                .document_acp()
                .settle_owed_changes(OwedScope::All)
                .await
                .expect("pass");
            if pass == OwedSettlement::default() && owed().await.is_empty() {
                break;
            }
            nuclei::time::sleep(Duration::from_millis(500)).await;
        }
        assert_eq!(
            owed().await,
            Vec::new(),
            "the grant and the revocation are both settled"
        );
        assert!(
            channel.arrived.load(Ordering::SeqCst) >= before + 2,
            "the grant and then the revocation reached the node"
        );
        assert!(
            !reads().await,
            "the revocation is the last change the node holds"
        );

        // The grant's own request, delivered again after the revocation.
        let grant = channel.held.pop().expect("the held request");
        let _ = channel.node.call(SEND, grant, 1 << 20).await;
        nuclei::time::sleep(Duration::from_secs(2)).await;
        assert!(
            !reads().await,
            "a request that executed does not execute again"
        );

        node.shutdown().await.expect("shutdown");
        vera_node.stop().await;
    })
    .await;
}
