//! A collection that declares a governance root is judged by a node's merge
//! validator with no claim by name, so a node that has a validator judges one
//! defined after it started. A node with no merge validator, or with no merge
//! governance at all, handles such a collection as it handles any other.

use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

const LEDGER_SDL: &str = r#"type Ledger @governed(root: "root-a") { entry: String }"#;
const PLAIN_SDL: &str = "type Plain { entry: String }";

const NO_VALIDATOR: &str = "collection Ledger is governed but no merge validator is installed";

/// Gives one verdict for every composite it is asked about, and counts the
/// asking.
struct Counting {
    verdict: MergeVerdict,
    asked: AtomicUsize,
}

impl Counting {
    fn giving(verdict: MergeVerdict) -> Arc<Self> {
        Arc::new(Self {
            verdict,
            asked: AtomicUsize::new(0),
        })
    }

    fn asked(&self) -> usize {
        self.asked.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl MergeValidator for Counting {
    async fn validate(
        &self,
        _candidate: &MergeCandidate<'_>,
        _view: &dyn MergeView,
    ) -> Result<MergeVerdict, String> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        Ok(self.verdict.clone())
    }
}

/// Governance that claims no collection by name.
fn claiming_nothing() -> MergeGovernance {
    MergeGovernance::new(Vec::<String>::new())
}

/// A running node that claimed nothing, judging with `validator`.
async fn node_judging_with(validator: &Arc<Counting>) -> Node {
    Node::open(
        RegolithStore::in_memory().unwrap(),
        claiming_nothing().with_validator(validator.clone()),
        true,
    )
    .await
}

/// A running node on which no merge governance was ever installed.
async fn node_without_governance(store: RegolithStore) -> Node {
    let store = Arc::new(store);
    let db = Arc::new(
        DB::open_from_arc_with_options(store.clone(), DbOptions::default())
            .await
            .unwrap(),
    );
    Node::assemble(db, store)
}

/// Define a collection on a node that is already running.
async fn define(node: &Node, sdl: &str) {
    node.db
        .add_collection(&defra_core::RequestContext::ambient(), sdl)
        .await
        .unwrap();
}

/// Offer `authored` to the node as a peer would, keeping an error as an
/// error.
async fn offer(authored: &Genesis, node: &Node, creator: &str) -> Result<MergeOutcome, String> {
    authored.store(node).await;
    node.handler
        .handle_block(
            &authored.cid,
            authored.bytes(),
            BlockMetadata::normal(
                &authored.doc_id,
                &authored.collection_id,
                creator,
                Some("peer"),
                false,
            ),
        )
        .await
        .map_err(|error| error.to_string())
}

#[test]
fn a_rooted_collection_defined_after_start_is_judged_with_no_claim() {
    workers::block_on(async {
        let validator = Counting::giving(MergeVerdict::reject("no"));
        let node = node_judging_with(&validator).await;
        let writer = signer();

        define(&node, LEDGER_SDL).await;
        let entry = genesis(&target(LEDGER_SDL), "entry", "x", &writer);

        assert_eq!(
            entry.merge(&node, &writer.did).await,
            MergeOutcome::rejected("no")
        );
        assert_eq!(validator.asked(), 1);
        assert!(node.doc_ids("Ledger").await.is_empty());
    })
    .unwrap()
}

#[test]
fn a_rooted_collection_merges_what_the_validator_accepts() {
    workers::block_on(async {
        let validator = Counting::giving(MergeVerdict::Accept);
        let node = node_judging_with(&validator).await;
        let writer = signer();

        define(&node, LEDGER_SDL).await;
        let entry = genesis(&target(LEDGER_SDL), "entry", "x", &writer);

        assert_eq!(entry.merge(&node, &writer.did).await, MergeOutcome::Merged);
        assert_eq!(validator.asked(), 1);
        assert_eq!(node.doc_ids("Ledger").await, vec![entry.doc_id.clone()]);
    })
    .unwrap()
}

#[test]
fn a_collection_with_no_root_and_no_claim_merges_unjudged() {
    workers::block_on(async {
        let validator = Counting::giving(MergeVerdict::reject("no"));
        let node = node_judging_with(&validator).await;
        let writer = signer();

        define(&node, PLAIN_SDL).await;
        let entry = genesis(&target(PLAIN_SDL), "entry", "x", &writer);

        assert_eq!(entry.merge(&node, &writer.did).await, MergeOutcome::Merged);
        assert_eq!(validator.asked(), 0);
        assert_eq!(node.doc_ids("Plain").await, vec![entry.doc_id.clone()]);
    })
    .unwrap()
}

/// Governance with no merge validator, as an application that installs only
/// a read or write validator has: the root alone governs nothing there.
#[test]
fn a_rooted_collection_on_a_node_with_no_validator_merges_as_before() {
    workers::block_on(async {
        let node = Node::open(
            RegolithStore::in_memory().unwrap(),
            claiming_nothing(),
            true,
        )
        .await;
        let writer = signer();

        define(&node, LEDGER_SDL).await;
        let entry = genesis(&target(LEDGER_SDL), "entry", "x", &writer);

        assert_eq!(entry.merge(&node, &writer.did).await, MergeOutcome::Merged);
        assert_eq!(node.handler.deferred_composites(), 0);
        assert_eq!(node.doc_ids("Ledger").await, vec![entry.doc_id.clone()]);
    })
    .unwrap()
}

#[test]
fn a_rooted_collection_on_a_node_without_governance_merges_as_before() {
    workers::block_on(async {
        let node = node_without_governance(RegolithStore::in_memory().unwrap()).await;
        let writer = signer();
        define(&node, LEDGER_SDL).await;
        define(&node, PLAIN_SDL).await;

        let entry = genesis(&target(LEDGER_SDL), "entry", "from a peer", &writer);
        assert_eq!(entry.merge(&node, &writer.did).await, MergeOutcome::Merged);
        assert_eq!(node.handler.deferred_composites(), 0);

        node.create_locally("Ledger", r#"{"entry": "written here"}"#)
            .await;
        assert_eq!(node.doc_ids("Ledger").await.len(), 2);

        let plain = genesis(&target(PLAIN_SDL), "entry", "from a peer", &writer);
        assert_eq!(plain.merge(&node, &writer.did).await, MergeOutcome::Merged);
    })
    .unwrap()
}

/// A name claim with no validator holds every composite unmerged, as it did
/// before a root governed anything, whether or not the collection declares
/// a root.
#[test]
fn a_claimed_rooted_collection_with_no_validator_is_held_as_before() {
    workers::block_on(async {
        let node = Node::open(
            RegolithStore::in_memory().unwrap(),
            MergeGovernance::new(["Ledger"]),
            true,
        )
        .await;
        let writer = signer();

        define(&node, LEDGER_SDL).await;
        let entry = genesis(&target(LEDGER_SDL), "entry", "x", &writer);

        assert_eq!(
            entry.merge(&node, &writer.did).await,
            MergeOutcome::retryable_skip(NO_VALIDATOR)
        );
        assert_eq!(node.handler.deferred_composites(), 0);
        assert!(node.doc_ids("Ledger").await.is_empty());
        assert_eq!(node.handler.sweep_unmerged_governed().await, 0);
    })
    .unwrap()
}

/// A rooted collection is resolved from the block alone on a node that judges
/// it: were the carrier's collection id honoured, a sender could file a
/// composite of a version this node does not hold under the governed
/// collection.
#[test]
fn a_rooted_collection_is_not_resolved_from_the_carrier() {
    workers::block_on(async {
        let validator = Counting::giving(MergeVerdict::Accept);
        let node = node_judging_with(&validator).await;
        let writer = signer();
        define(&node, LEDGER_SDL).await;

        let stray = genesis(
            &Target {
                version_id: "unknown-version".to_string(),
                collection_id: target(LEDGER_SDL).collection_id,
            },
            "entry",
            "x",
            &writer,
        );

        assert_eq!(
            stray.merge(&node, &writer.did).await,
            MergeOutcome::retryable_skip(
                "schema version unknown-version is not held; a governed collection is resolved only from the block"
            )
        );
        assert_eq!(validator.asked(), 0);
        assert!(node.doc_ids("Ledger").await.is_empty());
    })
    .unwrap()
}

#[test]
fn an_update_ahead_of_its_genesis_in_a_rooted_collection_waits_for_it() {
    workers::block_on(async {
        let validator = Counting::giving(MergeVerdict::Accept);
        let node = node_judging_with(&validator).await;
        let writer = signer();
        define(&node, LEDGER_SDL).await;

        let ledger = target(LEDGER_SDL);
        let created = genesis(&ledger, "entry", "first", &writer);
        let updated = authored(&ledger, Some(&created), "entry", "second", &writer);

        assert_eq!(
            updated.merge(&node, &writer.did).await,
            MergeOutcome::retryable_skip("document genesis not held")
        );
        assert_eq!(node.handler.deferred_composites(), 1);

        assert_eq!(
            created.merge(&node, &writer.did).await,
            MergeOutcome::Merged
        );
        assert_eq!(
            field_value(&node, "Ledger", &created.doc_id, "entry").await,
            Some("second".to_string())
        );
        assert_eq!(node.handler.deferred_composites(), 0);
    })
    .unwrap()
}

/// Without a validator an update ahead of its genesis fares in a rooted
/// collection as in one with no root: it fails, and nothing is deferred for
/// it.
#[test]
fn an_unresolved_update_in_a_rooted_collection_without_a_validator_fares_as_before() {
    workers::block_on(async {
        let node = node_without_governance(RegolithStore::in_memory().unwrap()).await;
        let writer = signer();
        define(&node, LEDGER_SDL).await;
        define(&node, PLAIN_SDL).await;

        let unresolved = |sdl: &str| {
            let target = target(sdl);
            let created = genesis(&target, "entry", "first", &writer);
            authored(&target, Some(&created), "entry", "second", &writer)
        };
        let rooted = offer(&unresolved(LEDGER_SDL), &node, &writer.did).await;
        let plain = offer(&unresolved(PLAIN_SDL), &node, &writer.did).await;

        assert!(plain.is_err(), "{plain:?}");
        assert!(rooted.is_err(), "{rooted:?}");
        assert_eq!(node.handler.deferred_composites(), 0);
        assert!(node.forwarded().is_empty());
        assert!(node.doc_ids("Ledger").await.is_empty());
    })
    .unwrap()
}

/// A composite a node held under a name claim, before it had a validator, is
/// judged once the node runs with one that claims nothing: the root governs
/// it there, and the sweep finds it unmerged after the restart.
#[test]
fn a_held_rooted_composite_is_judged_once_the_node_has_a_validator() {
    workers::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("node");
        let writer = signer();
        let entry = genesis(&target(LEDGER_SDL), "entry", "x", &writer);

        {
            let node = Node::open(
                RegolithStore::open(&path).unwrap(),
                MergeGovernance::new(["Ledger"]),
                true,
            )
            .await;
            define(&node, LEDGER_SDL).await;
            assert_eq!(
                entry.merge(&node, &writer.did).await,
                MergeOutcome::retryable_skip(NO_VALIDATOR)
            );
            node.db.close().await.unwrap();
        }

        let validator = Counting::giving(MergeVerdict::Accept);
        let node = Node::open(
            RegolithStore::open(&path).unwrap(),
            claiming_nothing().with_validator(validator.clone()),
            false,
        )
        .await;
        assert!(node.doc_ids("Ledger").await.is_empty());

        assert_eq!(node.handler.sweep_unmerged_governed().await, 1);

        assert_eq!(validator.asked(), 1);
        assert_eq!(node.doc_ids("Ledger").await, vec![entry.doc_id.clone()]);
        assert_eq!(node.forwarded(), vec![entry.cid]);
    })
    .unwrap()
}
