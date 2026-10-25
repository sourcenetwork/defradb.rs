use super::super::hook::CompositeMergeHook;
use super::*;
use blockstore::{Blockstore as _, DefraBlockstore};
use defra_core::block::{CollectionDeltaPayload, CompositeDeltaPayload, DAGLink};
use schema::{CollectionVersion, FieldDescription, FieldKind};
use storage::RegolithStore;

struct DriverFailure {
    kind: u8,
    root: Cid,
}

#[async_trait::async_trait]
impl CompositeMergeHook for DriverFailure {
    async fn on_protected_composite(
        &self,
        _: &str,
        _: &CollectionVersion,
        _: &BlockMetadata<'_>,
    ) -> Result<Option<MergeOutcome>, MergeError> {
        Err(match self.kind {
            0 => MergeError::HistoryRequired,
            1 => MergeError::HistoryAborted(Box::new(HistoryRejection {
                root: self.root,
                context: [3; 32],
                generation: [4; 16],
                reason: "bad ancestor".into(),
            })),
            2 => MergeError::NeedsDecryptionKey(self.root),
            _ => MergeError::DepthExceeded {
                cid: self.root,
                depth: 99,
            },
        })
    }
}

#[test]
fn standalone_collection_preserves_linked_driver_errors() {
    workers::block_on(async {
        for kind in [1, 0, 2, 3] {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let db = Arc::new(DB::from_arc(store.clone()).unwrap());
            db.create_collection(CollectionVersion::new(
                "Users",
                "v1",
                "col-users",
                vec![FieldDescription::new("1", "_docID", FieldKind::doc_id())],
            ))
            .await
            .unwrap();
            let blocks = Arc::new(DefraBlockstore::new(store, true));
            let handler = DbMergeHandler::new(db, blocks.clone());
            let composite = Block::new(
                CrdtDelta::Composite(CompositeDeltaPayload {
                    schema_version_id: "v1".into(),
                    priority: 1,
                    status: 1,
                }),
                vec![],
                vec![],
            );
            let root = composite.generate_cid().unwrap();
            blocks
                .put(&root, &composite.to_dag_cbor().unwrap())
                .await
                .unwrap();
            handler.set_composite_merge_hook(Arc::new(DriverFailure { kind, root }));
            let payload = CollectionDeltaPayload {
                schema_version_id: "v1".into(),
                priority: 1,
            };
            let carrier = Block::new(
                CrdtDelta::Collection(payload.clone()),
                vec![],
                vec![DAGLink::new("_head", root)],
            );
            let cid = carrier.generate_cid().unwrap();
            let doc = crate::block::builder::derive_doc_id(&root);
            let metadata = BlockMetadata::normal(&doc, "col-users", "peer", Some("peer"), false);
            let error = handler
                .process_collection_delta(&cid, &carrier, &payload, &metadata, 0)
                .await
                .unwrap_err();
            match (kind, error) {
                (0, MergeError::HistoryRequired) => {}
                (1, MergeError::HistoryAborted(rejection)) => {
                    assert_eq!(rejection.root, root);
                    assert_eq!(rejection.context, [3; 32]);
                    assert_eq!(rejection.generation, [4; 16]);
                    assert_eq!(rejection.reason, "bad ancestor");
                }
                (2, MergeError::NeedsDecryptionKey(cid)) => assert_eq!(cid, root),
                (3, MergeError::DepthExceeded { cid, depth: 99 }) => assert_eq!(cid, root),
                (_, error) => panic!("driver error {kind} was replaced: {error}"),
            }
        }
    })
    .unwrap();
}
