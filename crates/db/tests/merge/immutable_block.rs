use std::collections::BTreeMap;
use std::sync::Arc;

use blockstore::{Blockstore as _, DefraBlockstore};
use db::collection::Collection;
use db::database::DB;
use db::merge::merge_handler::DbMergeHandler;
use defra_core::block::{
    Block, CompositeDeltaPayload, CrdtDelta, DAGLink, ImmutableDeltaPayload, IMMUTABLE_LINK_NAME,
};
use defra_core::merge::{BlockMetadata, MergeBlock, MergeHandler};
use document::{DocID, Document, NormalValue};
use schema::{CollectionVersion, FieldDescription, FieldKind};
use storage::RegolithStore;

type Handler = DbMergeHandler<RegolithStore, DefraBlockstore<RegolithStore>>;

const COLLECTION_ID: &str = "col-records";

async fn make_handler() -> (Handler, Arc<DefraBlockstore<RegolithStore>>, Collection) {
    make_handler_with_flags(true).await
}

/// `immutable_flags: false` is how a peer that synced the definition of an
/// ungoverned collection sees it: same version, no `@immutable` flags.
async fn make_handler_with_flags(
    immutable_flags: bool,
) -> (Handler, Arc<DefraBlockstore<RegolithStore>>, Collection) {
    let field = |id: &str, name: &str| {
        let field =
            FieldDescription::new(id, name, FieldKind::string(), schema::CType::LwwRegister);
        if immutable_flags {
            field.as_immutable()
        } else {
            field
        }
    };
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(DB::from_arc(store.clone()).unwrap());
    db.create_collection(CollectionVersion::new(
        "Records",
        "v1",
        COLLECTION_ID,
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            field("2", "author"),
            FieldDescription::new("3", "body", FieldKind::string(), schema::CType::LwwRegister),
            field("4", "kind"),
        ],
    ))
    .await
    .unwrap();
    let collection = db
        .find_collection_by_id(COLLECTION_ID)
        .unwrap()
        .expect("collection exists");
    let blockstore = Arc::new(DefraBlockstore::new(store, false));
    (
        DbMergeHandler::new(db, blockstore.clone()),
        blockstore,
        collection,
    )
}

fn record(collection: &Collection) -> Document {
    let mut doc = Document::new(collection.schema_arc());
    doc.set("author", NormalValue::String("did:key:alice".into()))
        .unwrap();
    doc.set("body", NormalValue::String("hello".into()))
        .unwrap();
    doc.set("kind", NormalValue::String("note".into())).unwrap();
    doc.set_collection_version_id("v1");
    doc
}

async fn load(blockstore: &DefraBlockstore<RegolithStore>, cid: &cid::Cid) -> Block {
    Block::from_dag_cbor(&blockstore.get(cid).await.unwrap().unwrap()).unwrap()
}

async fn read_doc(handler: &Handler, collection: &Collection, doc_id: &DocID) -> Option<Document> {
    let txn = handler.db().new_txn(true).await.unwrap();
    let doc = {
        let datastore = txn.datastore().unwrap();
        let systemstore = txn.systemstore().unwrap();
        collection
            .get_by_doc_id(&datastore, &systemstore, doc_id)
            .await
            .unwrap()
    };
    txn.force_discard().unwrap();
    doc
}

fn merge_block(cid: cid::Cid, block_data: bytes::Bytes, doc_id: &str) -> MergeBlock {
    MergeBlock {
        cid,
        block_data,
        doc_id: doc_id.to_string(),
        collection_id: COLLECTION_ID.to_string(),
        creator: "did:key:z6MkrImmutableBlock".to_string(),
        sender_peer: Some("peer1".to_string()),
        is_explicit_replicator: false,
        explicit_replay_authorization: None,
        verified_creator: None,
    }
}

fn cbor(value: &str) -> serde_bytes::ByteBuf {
    let mut data = Vec::new();
    ciborium::into_writer(&NormalValue::String(value.into()), &mut data).unwrap();
    serde_bytes::ByteBuf::from(data)
}

/// Store a genesis composite linking `immutable`, returning the composite CID
/// and bytes. `encryption` is the composite's doc-level encryption link.
async fn put_genesis(
    blockstore: &DefraBlockstore<RegolithStore>,
    immutable: ImmutableDeltaPayload,
    encryption: Option<cid::Cid>,
) -> (cid::Cid, bytes::Bytes) {
    let immutable_block = Block::new(CrdtDelta::Immutable(immutable), vec![], vec![]);
    let immutable_cid = immutable_block.generate_cid().unwrap();
    blockstore
        .put(&immutable_cid, &immutable_block.to_dag_cbor().unwrap())
        .await
        .unwrap();
    let mut composite = Block::new(
        CrdtDelta::Composite(CompositeDeltaPayload {
            collection_version_id: "v1".into(),
            priority: 1,
            status: 1,
        }),
        vec![],
        vec![DAGLink::new(IMMUTABLE_LINK_NAME, immutable_cid)],
    );
    composite.encryption = encryption;
    let cid = composite.generate_cid().unwrap();
    let bytes = composite.to_dag_cbor().unwrap();
    blockstore.put(&cid, &bytes).await.unwrap();
    (cid, bytes.into())
}

#[test]
fn create_writes_immutable_fields_in_one_block() {
    workers::block_on(async {
        let (handler, blockstore, collection) = make_handler().await;
        let mut doc = record(&collection);

        let (_, _, result) =
            super::merge_handler_tests::create_doc_locally(&handler, &collection, &mut doc).await;

        let composite = load(&blockstore, &result.cid).await;
        let mut links: Vec<&str> = composite
            .links
            .iter()
            .flatten()
            .map(|link| link.name.as_str())
            .collect();
        links.sort();
        assert_eq!(links, [IMMUTABLE_LINK_NAME, "body"]);
        assert_eq!(result.field_cids.len(), 2);

        let immutable_cid = composite
            .links
            .iter()
            .flatten()
            .find(|link| link.name == IMMUTABLE_LINK_NAME)
            .unwrap()
            .link;
        let CrdtDelta::Immutable(payload) = load(&blockstore, &immutable_cid).await.delta else {
            panic!("expected an Immutable block");
        };
        assert_eq!(
            payload
                .fields
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["author", "kind"]
        );
    })
    .unwrap()
}

#[test]
fn update_writes_no_block_for_unchanged_immutable_field() {
    workers::block_on(async {
        let (handler, blockstore, collection) = make_handler().await;
        let mut doc = record(&collection);
        let (_, short_id, created) =
            super::merge_handler_tests::create_doc_locally(&handler, &collection, &mut doc).await;

        doc.set("body", NormalValue::String("edited".into()))
            .unwrap();
        let modified: rapidhash::RapidHashSet<String> =
            ["author", "body"].into_iter().map(String::from).collect();
        let txn = handler.db().new_txn(false).await.unwrap();
        let result = db::block::builder::write_document_blocks(
            &txn.blockstore().unwrap(),
            &txn.headstore().unwrap(),
            &doc,
            collection.schema(),
            db::block::builder::DocRef::new(collection.resolved_root_id(), short_id),
            Some(&modified),
            None,
            None,
            None,
        )
        .await
        .unwrap();
        txn.force_commit().await.unwrap();

        let composite = load(&blockstore, &result.cid).await;
        let links: Vec<&str> = composite
            .links
            .iter()
            .flatten()
            .map(|link| link.name.as_str())
            .collect();
        assert_eq!(links, ["body"]);
        assert_eq!(composite.heads, Some(vec![created.cid]));
    })
    .unwrap()
}

#[test]
fn remote_genesis_applies_immutable_block_fields() {
    workers::block_on(async {
        let (local, local_blocks, local_collection) = make_handler().await;
        let doc = record(&local_collection);
        let built = db::block::builder::build_blocks_from_document(
            &doc,
            local_collection.schema(),
            &local_blocks,
        )
        .await
        .unwrap();

        for batch in [false, true] {
            let (remote, remote_blocks, remote_collection) = make_handler().await;
            let composite = load(&local_blocks, &built.cid).await;
            for link in composite.links.iter().flatten() {
                let data = local_blocks.get(&link.link).await.unwrap().unwrap();
                remote_blocks.put(&link.link, &data).await.unwrap();
            }
            remote_blocks.put(&built.cid, &built.block).await.unwrap();

            let outcome = if batch {
                remote
                    .handle_block_batch(&[merge_block(
                        built.cid,
                        built.block.clone(),
                        &built.doc_id,
                    )])
                    .await
                    .remove(0)
            } else {
                remote
                    .handle_block(
                        &built.cid,
                        &built.block,
                        BlockMetadata::normal(
                            &built.doc_id,
                            COLLECTION_ID,
                            "did:key:z6MkrImmutableBlock",
                            Some("peer1"),
                            false,
                        ),
                    )
                    .await
            };
            assert!(outcome.unwrap().is_merged(), "batch={batch}");

            let merged = read_doc(
                &remote,
                &remote_collection,
                &DocID::from_string(&built.doc_id).unwrap(),
            )
            .await
            .expect("merged document exists");
            for (field, value) in [
                ("author", "did:key:alice"),
                ("body", "hello"),
                ("kind", "note"),
            ] {
                assert_eq!(
                    merged.get(field),
                    Some(&NormalValue::String(value.into())),
                    "batch={batch} field={field}"
                );
            }
        }
        drop(local);
    })
    .unwrap()
}

#[test]
fn peer_without_immutable_flags_merges_and_chains_its_updates() {
    workers::block_on(async {
        let (handler, blockstore, collection) = make_handler_with_flags(false).await;
        let (cid, bytes) = put_genesis(
            &blockstore,
            ImmutableDeltaPayload {
                priority: 1,
                collection_version_id: "v1".into(),
                fields: BTreeMap::from([
                    ("author".to_string(), cbor("did:key:alice")),
                    ("kind".to_string(), cbor("note")),
                ]),
            },
            None,
        )
        .await;
        let doc_id = DocID::new_v0(cid);
        let doc_id_str = doc_id.to_string();
        let outcome = handler
            .handle_block(
                &cid,
                &bytes,
                BlockMetadata::normal(
                    &doc_id_str,
                    COLLECTION_ID,
                    "did:key:z6MkrImmutableBlock",
                    None,
                    false,
                ),
            )
            .await
            .unwrap();
        assert!(outcome.is_merged(), "got {outcome:?}");
        let merged = read_doc(&handler, &collection, &doc_id).await.unwrap();
        assert_eq!(
            merged.get("author"),
            Some(&NormalValue::String("did:key:alice".into()))
        );

        // Here `author` is mutable: its update must chain to the Immutable block
        // at priority 2, not start a second root that ties with it at priority 1.
        let immutable_cid = load(&blockstore, &cid).await.links.unwrap()[0].link;
        let mut author_cids = Vec::new();
        for author in ["did:key:bob", "did:key:carol"] {
            let txn = handler.db().new_txn(false).await.unwrap();
            let short_id = db::docid::map::get_doc_ref(&txn.systemstore().unwrap(), &doc_id_str)
                .await
                .unwrap()
                .expect("merged document has a short id")
                .doc_short_id;
            let mut update = Document::new(collection.schema_arc());
            update.set_id(doc_id.clone());
            update
                .set("author", NormalValue::String(author.into()))
                .unwrap();
            let modified: rapidhash::RapidHashSet<String> =
                ["author".to_string()].into_iter().collect();
            let result = db::block::builder::write_document_blocks(
                &txn.blockstore().unwrap(),
                &txn.headstore().unwrap(),
                &update,
                collection.schema(),
                db::block::builder::DocRef::new(collection.resolved_root_id(), short_id),
                Some(&modified),
                None,
                None,
                None,
            )
            .await
            .unwrap();
            db::docid::map::set_block_doc_id_mapping(
                &txn.systemstore().unwrap(),
                &result.field_cids[0].to_string(),
                &doc_id_str,
            )
            .await
            .unwrap();
            txn.force_commit().await.unwrap();
            author_cids.push(result.field_cids[0]);
        }

        let author_block = load(&blockstore, &author_cids[0]).await;
        assert_eq!(author_block.heads, Some(vec![immutable_cid]));
        assert_eq!(author_block.delta.priority(), 2);

        // A walk filtered to `kind` passes through the `author` updates to reach
        // the Immutable block that carries `kind`. The starting block is always
        // reported, whatever the filter.
        let rows = commits(
            &handler,
            db::read::commits::CommitsQueryOptions {
                doc_id: Some(doc_id_str.clone()),
                cid: Some(author_cids[1].to_string()),
                depth: Some(3),
                field_name: Some("kind".into()),
                ..Default::default()
            },
        )
        .await;
        let kind_rows: Vec<_> = rows
            .iter()
            .map(commit_row)
            .filter(|(_, field, _)| field == "kind")
            .collect();
        assert_eq!(
            kind_rows,
            [(
                immutable_cid.to_string(),
                "kind".to_string(),
                Some(base64_cbor("note"))
            )]
        );
    })
    .unwrap()
}

#[test]
fn later_immutable_block_cannot_change_a_value() {
    workers::block_on(async {
        let (handler, blockstore, collection) = make_handler().await;
        let mut doc = record(&collection);
        let (doc_id, _, created) =
            super::merge_handler_tests::create_doc_locally(&handler, &collection, &mut doc).await;

        let forged = Block::new(
            CrdtDelta::Immutable(ImmutableDeltaPayload {
                priority: 2,
                collection_version_id: "v1".into(),
                fields: BTreeMap::from([("author".to_string(), cbor("did:key:mallory"))]),
            }),
            vec![],
            vec![],
        );
        let forged_cid = forged.generate_cid().unwrap();
        blockstore
            .put(&forged_cid, &forged.to_dag_cbor().unwrap())
            .await
            .unwrap();
        let update = Block::new(
            CrdtDelta::Composite(CompositeDeltaPayload {
                collection_version_id: "v1".into(),
                priority: 2,
                status: 1,
            }),
            vec![created.cid],
            vec![DAGLink::new(IMMUTABLE_LINK_NAME, forged_cid)],
        );
        let update_cid = update.generate_cid().unwrap();
        let update_bytes = update.to_dag_cbor().unwrap();
        blockstore.put(&update_cid, &update_bytes).await.unwrap();

        let doc_id_str = doc_id.to_string();
        let outcome = handler
            .handle_block(
                &update_cid,
                &update_bytes,
                BlockMetadata::normal(
                    &doc_id_str,
                    COLLECTION_ID,
                    "did:key:z6MkrImmutableBlock",
                    None,
                    false,
                ),
            )
            .await
            .unwrap();

        assert!(outcome.is_terminal_skip(), "got {outcome:?}");
        let stored = read_doc(&handler, &collection, &doc_id).await.unwrap();
        assert_eq!(
            stored.get("author"),
            Some(&NormalValue::String("did:key:alice".into()))
        );
    })
    .unwrap()
}

#[test]
fn batch_and_single_create_build_the_same_genesis() {
    workers::block_on(async {
        let (handler, _, collection) = make_handler().await;
        let doc = record(&collection);
        let identity =
            db::block::builder::DocRef::new(collection.resolved_root_id(), 7);

        let computed = db::block::builder::compute_document_blocks(
            &doc,
            collection.schema(),
            identity,
            None,
            None,
        )
        .unwrap();
        let txn = handler.db().new_txn(false).await.unwrap();
        let written = db::block::builder::write_document_blocks(
            &txn.blockstore().unwrap(),
            &txn.headstore().unwrap(),
            &doc,
            collection.schema(),
            identity,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
        txn.force_discard().unwrap();

        assert_eq!(computed.block_result.cid, written.cid);
        assert_eq!(computed.block_result.field_cids.len(), 2);
    })
    .unwrap()
}

#[test]
fn a_field_named_like_the_immutable_link_is_rejected() {
    workers::block_on(async {
        let db = DB::from_arc(Arc::new(RegolithStore::in_memory().unwrap())).unwrap();
        let created = db
            .create_collection(CollectionVersion::new(
                "Clash",
                "v1",
                "col-clash",
                vec![
                    FieldDescription::new(
                        "1",
                        "_docID",
                        FieldKind::doc_id(),
                        schema::CType::LwwRegister,
                    ),
                    FieldDescription::new(
                        "2",
                        IMMUTABLE_LINK_NAME,
                        FieldKind::string(),
                        schema::CType::LwwRegister,
                    ),
                ],
            ))
            .await;
        let error = created.expect_err("a field cannot take the immutable link name");
        assert!(error.to_string().contains("reserved"), "{error}");
    })
    .unwrap()
}

async fn commits(
    handler: &Handler,
    options: db::read::commits::CommitsQueryOptions,
) -> Vec<document::CommitRow> {
    let txn = handler.db().new_txn(true).await.unwrap();
    let fetcher = db::read::commits::CommitsFetcher::new(Arc::new(txn));
    fetcher.fetch_commits(&options).await.unwrap()
}

fn commit_row(commit: &document::CommitRow) -> (String, String, Option<String>) {
    let text = |key: &str| commit.get(key).and_then(|v| v.as_str()).map(str::to_string);
    (
        text("cid").unwrap(),
        text("fieldName").unwrap(),
        text("delta"),
    )
}

fn base64_cbor(value: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(cbor(value))
}

#[test]
fn commits_for_an_immutable_field_return_the_immutable_block() {
    workers::block_on(async {
        let (handler, blockstore, collection) = make_handler().await;
        let mut doc = record(&collection);
        let (doc_id, _, created) =
            super::merge_handler_tests::create_doc_locally(&handler, &collection, &mut doc).await;
        let immutable_cid = load(&blockstore, &created.cid)
            .await
            .links
            .unwrap()
            .into_iter()
            .find(|link| link.name == IMMUTABLE_LINK_NAME)
            .unwrap()
            .link;

        let rows = commits(
            &handler,
            db::read::commits::CommitsQueryOptions {
                doc_id: Some(doc_id.to_string()),
                field_name: Some("author".into()),
                ..Default::default()
            },
        )
        .await;

        assert_eq!(
            rows.iter().map(commit_row).collect::<Vec<_>>(),
            [(
                immutable_cid.to_string(),
                "author".to_string(),
                Some(base64_cbor("did:key:alice"))
            )]
        );
    })
    .unwrap()
}

/// Downsampling reads a document's values from its commits by height.
#[test]
fn height_one_commits_carry_every_immutable_field_value() {
    workers::block_on(async {
        let (handler, _, collection) = make_handler().await;
        let mut doc = record(&collection);
        let (doc_id, _, _) =
            super::merge_handler_tests::create_doc_locally(&handler, &collection, &mut doc).await;

        let rows = commits(
            &handler,
            db::read::commits::CommitsQueryOptions {
                doc_id: Some(doc_id.to_string()),
                height_start: Some(1),
                height_end: Some(2),
                ..Default::default()
            },
        )
        .await;

        let mut values: Vec<_> = rows
            .iter()
            .map(commit_row)
            .filter_map(|(_, field, delta)| Some((field, delta?)))
            .collect();
        values.sort();
        assert_eq!(
            values,
            [
                ("author".to_string(), base64_cbor("did:key:alice")),
                ("body".to_string(), base64_cbor("hello")),
                ("kind".to_string(), base64_cbor("note")),
            ]
        );
    })
    .unwrap()
}

/// A doc-level encrypted document encrypts every field, so its builders never
/// write an Immutable block. One arriving under an encrypted genesis would put
/// plaintext values beside the encrypted ones.
#[test]
fn an_encrypted_genesis_cannot_carry_an_immutable_block() {
    workers::block_on(async {
        let (handler, blockstore, collection) = make_handler().await;
        let encryption = Block::new(
            CrdtDelta::Immutable(ImmutableDeltaPayload {
                priority: 1,
                collection_version_id: "v1".into(),
                fields: BTreeMap::new(),
            }),
            vec![],
            vec![],
        )
        .generate_cid()
        .unwrap();
        let (cid, bytes) = put_genesis(
            &blockstore,
            ImmutableDeltaPayload {
                priority: 1,
                collection_version_id: "v1".into(),
                fields: BTreeMap::from([("author".to_string(), cbor("did:key:alice"))]),
            },
            Some(encryption),
        )
        .await;
        let doc_id = DocID::new_v0(cid);
        let doc_id_str = doc_id.to_string();

        let single = handler
            .handle_block(
                &cid,
                &bytes,
                BlockMetadata::normal(
                    &doc_id_str,
                    COLLECTION_ID,
                    "did:key:z6MkrImmutableBlock",
                    None,
                    false,
                ),
            )
            .await;
        let batch = handler
            .handle_block_batch(&[merge_block(cid, bytes.clone(), &doc_id_str)])
            .await
            .remove(0);

        for outcome in [single, batch] {
            assert!(
                !outcome.as_ref().is_ok_and(|outcome| outcome.is_merged()),
                "got {outcome:?}"
            );
        }
        assert!(read_doc(&handler, &collection, &doc_id).await.is_none());
    })
    .unwrap()
}
