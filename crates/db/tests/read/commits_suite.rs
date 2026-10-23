mod unit_tests {
    use db::*;
    use document::NormalValue;
    use storage::RegolithStore;

    fn commit(doc_id: &str, field_name: &str) -> Document {
        let mut commit = Document::new();
        commit.set("docID", NormalValue::String(doc_id.to_string()));
        commit.set("fieldName", NormalValue::String(field_name.to_string()));
        commit
    }

    #[test]
    fn test_commits_query_options_default() {
        let opts = CommitsQueryOptions::default();
        assert!(opts.doc_id.is_none());
        assert!(opts.cid.is_none());
        assert!(opts.depth.is_none());
        assert!(opts.height_start.is_none());
        assert!(opts.height_end.is_none());
        assert!(opts.field_name.is_none());
    }

    #[test]
    fn sort_commits_preserves_document_discovery_order() {
        let fetcher = CommitsFetcher::<RegolithStore>::detached();
        let mut commits = vec![
            commit("z-first", "_C"),
            commit("z-first", "name"),
            commit("a-second", "_C"),
            commit("a-second", "age"),
        ];

        fetcher.sort_commits_go_order(&mut commits);

        let actual: Vec<_> = commits
            .iter()
            .map(|commit| {
                (
                    commit.get("docID").and_then(|value| value.as_str()),
                    commit.get("fieldName").and_then(|value| value.as_str()),
                )
            })
            .collect();
        assert_eq!(
            actual,
            vec![
                (Some("z-first"), Some("name")),
                (Some("z-first"), Some("_C")),
                (Some("a-second"), Some("age")),
                (Some("a-second"), Some("_C")),
            ]
        );
    }
}

mod additional_tests {

    #[test]
    fn test_looks_like_cidv1() {
        use db::read::commits::CommitsFetcher;
        use storage::RegolithStore;

        assert!(CommitsFetcher::<RegolithStore>::looks_like_cidv1(
            "bafybeid57gpbwi4i6bg7g35hhhhhhhhhhhhhhhhhhhhhhhdoesnotexist"
        ));
        assert!(CommitsFetcher::<RegolithStore>::looks_like_cidv1(
            "bafyreiajq6jmyblg2b6vupjdapzkaodbt7kkwqp4fijekdvydnyxvr4y7q"
        ));

        assert!(!CommitsFetcher::<RegolithStore>::looks_like_cidv1(
            "fhbnjfahfhfhanfhga"
        ));
        assert!(!CommitsFetcher::<RegolithStore>::looks_like_cidv1("short"));
        assert!(!CommitsFetcher::<RegolithStore>::looks_like_cidv1(
            "randomtext"
        ));
    }
}

mod shared_owner_tests {

    use rapidhash::RapidHashSet;
    use std::sync::Arc;
    use storage::RegolithStore;

    use defra_core::{
        Block, CrdtDelta, LwwDeltaPayload, Signature, SignatureHeader, SignatureType,
    };
    use document::{DocID, NormalValue};

    use db::read::commits::{CommitsFetcher, CommitsQueryOptions};
    use db::{VersionedFetcher, DB};

    fn lww_block(value: &str, priority: u64, heads: Vec<cid::Cid>) -> Block {
        let mut data = Vec::new();
        ciborium::into_writer(&NormalValue::String(value.to_string()), &mut data).unwrap();
        Block::new(
            CrdtDelta::Lww(LwwDeltaPayload {
                field_name: "name".to_string(),
                collection_version_id: "v1".to_string(),
                priority,
                data,
            }),
            heads,
            vec![],
        )
    }

    #[test]
    fn collection_commit_depth_starts_from_live_heads() {
        workers::block_on(async {
            let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
            let parent = lww_block("parent", 1, vec![]);
            let parent_cid = parent.generate_cid().unwrap();
            let child = lww_block("child", 2, vec![parent_cid]);
            let child_cid = child.generate_cid().unwrap();

            let txn = db.new_txn(false).await.unwrap();
            {
                let blockstore = txn.blockstore().unwrap();
                let headstore = txn.headstore().unwrap();
                blockstore
                    .set(&parent_cid.to_bytes(), &parent.to_dag_cbor().unwrap())
                    .await
                    .unwrap();
                blockstore
                    .set(&child_cid.to_bytes(), &child.to_dag_cbor().unwrap())
                    .await
                    .unwrap();
                for (cid, priority, parents) in
                    [(parent_cid, 1, vec![]), (child_cid, 2, vec![parent_cid])]
                {
                    let set = db::block::heads::collection_set(&headstore, 1)
                        .await
                        .unwrap();
                    db::block::heads::publish_collection_block(
                        &headstore, 1, &set, cid, priority, &parents,
                    )
                    .await
                    .unwrap();
                }
            }
            txn.commit().await.unwrap();

            let commits_txn = db.new_txn(true).await.unwrap();
            let commits = CommitsFetcher::new(Arc::new(commits_txn))
                .fetch_commits(&CommitsQueryOptions {
                    depth: Some(1),
                    ..Default::default()
                })
                .await
                .unwrap();

            assert_eq!(commits.len(), 1);
            assert_eq!(
                commits[0].get("cid").and_then(NormalValue::as_str),
                Some(child_cid.to_string().as_str())
            );
        })
        .unwrap()
    }

    #[test]
    fn shared_field_cid_fans_out_to_every_owner() {
        workers::block_on(async {
            let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
            let mut data = Vec::new();
            ciborium::into_writer(&NormalValue::String("shared".to_string()), &mut data).unwrap();
            let block = Block::new(
                CrdtDelta::Lww(LwwDeltaPayload {
                    field_name: "name".to_string(),
                    collection_version_id: "v1".to_string(),
                    priority: 1,
                    data,
                }),
                vec![],
                vec![],
            );
            let cid = block.generate_cid().unwrap();
            let owners = [
                DocID::new_v0(defra_core::block::generate_cid_from_bytes(b"owner-a").unwrap())
                    .to_string(),
                DocID::new_v0(defra_core::block::generate_cid_from_bytes(b"owner-b").unwrap())
                    .to_string(),
            ];

            let txn = db.new_txn(false).await.unwrap();
            {
                let blockstore = txn.blockstore().unwrap();
                let systemstore = txn.systemstore().unwrap();
                blockstore
                    .set(&cid.to_bytes(), &block.to_dag_cbor().unwrap())
                    .await
                    .unwrap();
                for owner in &owners {
                    db::docid::map::set_block_doc_id_mapping(&systemstore, &cid.to_string(), owner)
                        .await
                        .unwrap();
                }
            }
            txn.commit().await.unwrap();

            let commits_txn = db.new_txn(true).await.unwrap();
            let commits = CommitsFetcher::new(Arc::new(commits_txn))
                .fetch_commits(&CommitsQueryOptions {
                    cid: Some(cid.to_string()),
                    ..Default::default()
                })
                .await
                .unwrap();
            let commit_owners: RapidHashSet<_> = commits
                .iter()
                .filter_map(|commit| commit.get("docID").and_then(|value| value.as_str()))
                .collect();
            assert_eq!(commit_owners, owners.iter().map(String::as_str).collect());

            let version_txn = db.new_txn(true).await.unwrap();
            let documents = VersionedFetcher::new(Arc::new(version_txn))
                .get_documents_at_cid(&cid.to_string(), None, None)
                .await
                .unwrap();
            let document_owners: RapidHashSet<_> = documents
                .iter()
                .filter_map(|document| document.id().map(ToString::to_string))
                .collect();
            assert_eq!(document_owners, owners.into_iter().collect());
        })
        .unwrap()
    }

    #[test]
    fn signature_value_uses_graphql_base64_encoding() {
        workers::block_on(async {
            let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
            let signature = Signature::new(
                SignatureHeader::new(SignatureType::ES256K, b"identity".to_vec()),
                vec![1, 2, 3, 4],
            );
            let signature_cid = signature.generate_cid().unwrap();
            let mut data = Vec::new();
            ciborium::into_writer(&NormalValue::String("value".to_string()), &mut data).unwrap();
            let block = Block::new_with_options(
                CrdtDelta::Lww(LwwDeltaPayload {
                    field_name: "name".to_string(),
                    collection_version_id: "v1".to_string(),
                    priority: 1,
                    data,
                }),
                vec![],
                vec![],
                None,
                Some(signature_cid),
            );
            let cid = block.generate_cid().unwrap();
            let doc_id =
                DocID::new_v0(defra_core::block::generate_cid_from_bytes(b"owner").unwrap())
                    .to_string();

            let txn = db.new_txn(false).await.unwrap();
            {
                let blockstore = txn.blockstore().unwrap();
                let systemstore = txn.systemstore().unwrap();
                blockstore
                    .set(&signature_cid.to_bytes(), &signature.to_dag_cbor().unwrap())
                    .await
                    .unwrap();
                blockstore
                    .set(&cid.to_bytes(), &block.to_dag_cbor().unwrap())
                    .await
                    .unwrap();
                db::docid::map::set_block_doc_id_mapping(&systemstore, &cid.to_string(), &doc_id)
                    .await
                    .unwrap();
            }
            txn.commit().await.unwrap();

            let commits_txn = db.new_txn(true).await.unwrap();
            let commits = CommitsFetcher::new(Arc::new(commits_txn))
                .fetch_commits(&CommitsQueryOptions {
                    cid: Some(cid.to_string()),
                    ..Default::default()
                })
                .await
                .unwrap();
            let signature = commits[0]
                .get("signature")
                .and_then(NormalValue::as_json)
                .unwrap();

            assert_eq!(signature["value"], "AQIDBA==");
        })
        .unwrap()
    }
}
