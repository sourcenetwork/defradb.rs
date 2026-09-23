//! The view's document lookups read what they need, not the collection.

use schema::{IndexKind, IndexedFieldDescription};

use super::*;

impl Node {
    /// Overwrite a document's stored blob so that any read of it fails, which
    /// makes a collection scan fail and leaves reads of other documents alone.
    async fn corrupt(&self, collection: &str, doc_id: &str) {
        let collection = self.db.get_collection(collection).unwrap().unwrap();
        let txn = self.db.new_txn(false).await.unwrap();
        {
            let short_id = collection
                .resolve_doc_short_id(&txn.systemstore().unwrap(), &doc_id.parse().unwrap())
                .await
                .unwrap()
                .unwrap();
            txn.datastore()
                .unwrap()
                .set(&collection.doc_key(short_id), b"not a document")
                .await
                .unwrap();
        }
        txn.commit().await.unwrap();
    }
}

#[tokio::test]
async fn immutable_fields_reads_one_document_not_the_collection() {
    let validator = Arc::new(ReadImmutable {
        collection: "Grants",
        doc_id: Mutex::new(String::new()),
        read: Mutex::new(None),
    });
    let node = Node::with_immutable_grants(validator.clone()).await;
    let writer = signer();
    let grant = genesis("col-grants", "writer", &writer.did, &writer);
    assert_eq!(grant.merge(&node, &writer.did).await, MergeOutcome::Merged);
    let bystander = genesis("col-grants", "writer", "someone else", &writer);
    assert_eq!(
        bystander.merge(&node, &writer.did).await,
        MergeOutcome::Merged
    );
    node.corrupt("Grants", &bystander.doc_id).await;
    assert!(AutoCommitFetcher::new(node.db.clone())
        .get_all("Grants")
        .await
        .is_err());

    *validator.doc_id.lock().unwrap() = grant.doc_id.clone();
    let note = genesis("col-notes", "grant", "anything", &writer);
    assert_eq!(note.merge(&node, &writer.did).await, MergeOutcome::Merged);
    assert_eq!(
        validator.read.lock().unwrap().clone().unwrap(),
        Some(vec![(
            "writer".to_string(),
            NormalValue::String(writer.did.clone())
        )])
    );
}

type Lookups = Vec<(String, Result<Vec<String>, String>)>;

/// Defers a grant held by "gated" until the composite `gate` is held, and
/// on a note looks every probe value up on each of `fields`.
struct Probe {
    gate: Mutex<Cid>,
    fields: &'static [&'static str],
    values: Mutex<Vec<String>>,
    found: Mutex<Lookups>,
}

impl Probe {
    fn new(fields: &'static [&'static str]) -> Arc<Self> {
        Arc::new(Self {
            gate: Mutex::new(Cid::default()),
            fields,
            values: Mutex::new(Vec::new()),
            found: Mutex::new(Vec::new()),
        })
    }

    /// What a note's verdict finds for `values`, by `field=value`.
    async fn lookups(&self, node: &Node, values: &[&str]) -> Lookups {
        *self.values.lock().unwrap() = values.iter().map(|value| value.to_string()).collect();
        self.found.lock().unwrap().clear();
        let writer = signer();
        let note = genesis("col-notes", "grant", &values.join(","), &writer);
        assert_eq!(note.merge(node, &writer.did).await, MergeOutcome::Merged);
        self.found.lock().unwrap().clone()
    }
}

#[async_trait]
impl MergeValidator for Probe {
    async fn validate(
        &self,
        candidate: &MergeCandidate<'_>,
        view: &dyn MergeView,
    ) -> Result<MergeVerdict, String> {
        if candidate.collection.name == "Grants" {
            let fields = view
                .composite_fields(candidate.cid)
                .await?
                .unwrap_or_default();
            let gated = fields.iter().any(|(name, value)| {
                name == "holder" && *value == FieldValue::Value(NormalValue::String("gated".into()))
            });
            let gate = *self.gate.lock().unwrap();
            if gated && view.composite_fields(&gate).await?.is_none() {
                return Ok(MergeVerdict::defer("gate not held", [gate]));
            }
            return Ok(MergeVerdict::Accept);
        }
        let values = self.values.lock().unwrap().clone();
        for value in values {
            for field in self.fields {
                let mut found = view
                    .find_documents("Grants", field, &NormalValue::String(value.clone()))
                    .await;
                if let Ok(ids) = &mut found {
                    ids.sort();
                }
                self.found
                    .lock()
                    .unwrap()
                    .push((format!("{field}={value}"), found));
            }
        }
        Ok(MergeVerdict::Accept)
    }
}

impl Node {
    /// Governed Grants whose `@immutable` `writer` carries an `@index` and
    /// whose `@immutable` `twin` does not, plus a mutable `holder`.
    async fn with_indexed_grants(validator: Arc<dyn MergeValidator>) -> Self {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let db = Arc::new(
            DB::open_from_arc_with_options(store.clone(), DbOptions::default())
                .await
                .unwrap(),
        );
        let immutable = |id: &str, name: &str| {
            let mut field = FieldDescription::new(id, name, FieldKind::string());
            field.immutable = true;
            field
        };
        db.create_collection(CollectionVersion::new(
            "Grants",
            "col-grants",
            "col-grants",
            vec![
                FieldDescription::new("1", "_docID", FieldKind::doc_id()),
                immutable("2", "writer"),
                immutable("3", "twin"),
                FieldDescription::new("4", "holder", FieldKind::string()),
            ],
        ))
        .await
        .unwrap();
        db.create_collection(CollectionVersion::new(
            "Notes",
            "col-notes",
            "col-notes",
            vec![
                FieldDescription::new("1", "_docID", FieldKind::doc_id()),
                FieldDescription::new("2", "grant", FieldKind::string()),
            ],
        ))
        .await
        .unwrap();
        db.create_index(
            "Grants",
            None,
            vec![IndexedFieldDescription {
                name: "writer".to_string(),
                descending: false,
            }],
            IndexKind::default(),
        )
        .await
        .unwrap();
        db.set_merge_governance(
            MergeGovernance::new(["Grants", "Notes"]).with_validator(validator),
        );
        Self::assemble(db, store)
    }
}

fn grant(value: &str, holder: &str, by: &Signer) -> Genesis {
    authored_fields(
        "col-grants",
        None,
        &[("writer", value), ("twin", value), ("holder", holder)],
        by,
    )
}

#[tokio::test]
async fn find_documents_through_an_index_matches_the_scan() {
    let probe = Probe::new(&["writer", "twin"]);
    let node = Node::with_indexed_grants(probe.clone()).await;
    let peer = signer();

    // Both halves of a fork share a value; one arrives from a peer, one is
    // written here.
    let forked = grant("shared", "a", &peer);
    assert_eq!(forked.merge(&node, &peer.did).await, MergeOutcome::Merged);
    node.create_locally(
        "Grants",
        r#"{"writer": "shared", "twin": "shared", "holder": "b"}"#,
    )
    .await;
    // "share" is a prefix of "shared" and must not match it.
    let lone = grant("share", "c", &peer);
    assert_eq!(lone.merge(&node, &peer.did).await, MergeOutcome::Merged);

    let found = probe.lookups(&node, &["shared", "share", "absent"]).await;
    let ids = |key: &str| {
        let (_, ids) = found.iter().find(|(found, _)| found == key).unwrap();
        ids.clone().unwrap()
    };
    assert_eq!(ids("writer=shared").len(), 2);
    assert!(ids("writer=shared").contains(&forked.doc_id));
    assert_eq!(ids("writer=share"), vec![lone.doc_id.clone()]);
    assert!(ids("writer=absent").is_empty());
    for value in ["shared", "share", "absent"] {
        assert_eq!(
            ids(&format!("writer={value}")),
            ids(&format!("twin={value}"))
        );
    }
}

#[tokio::test]
async fn find_documents_through_an_index_finds_redriven_and_local_documents() {
    let probe = Probe::new(&["writer", "twin"]);
    let node = Node::with_indexed_grants(probe.clone()).await;
    let peer = signer();
    let gate = grant("gate", "open", &peer);
    *probe.gate.lock().unwrap() = gate.cid;

    let redriven = grant("shared", "gated", &peer);
    assert_eq!(
        redriven.merge(&node, &peer.did).await,
        MergeOutcome::retryable_skip("gate not held")
    );
    assert_eq!(gate.merge(&node, &peer.did).await, MergeOutcome::Merged);
    assert_eq!(node.forwarded(), vec![redriven.cid]);
    node.create_locally(
        "Grants",
        r#"{"writer": "shared", "twin": "shared", "holder": "local"}"#,
    )
    .await;

    let found = probe.lookups(&node, &["shared"]).await;
    let by_index = found[0].1.clone().unwrap();
    assert_eq!(found[0].0, "writer=shared");
    assert_eq!(by_index.len(), 2);
    assert!(by_index.contains(&redriven.doc_id));
    assert_eq!(by_index, found[1].1.clone().unwrap());
}

#[tokio::test]
async fn find_documents_through_an_index_reads_only_the_matches() {
    let probe = Probe::new(&["writer", "twin"]);
    let node = Node::with_indexed_grants(probe.clone()).await;
    let peer = signer();
    let wanted = grant("wanted", "a", &peer);
    assert_eq!(wanted.merge(&node, &peer.did).await, MergeOutcome::Merged);
    let bystander = grant("bystander", "b", &peer);
    assert_eq!(
        bystander.merge(&node, &peer.did).await,
        MergeOutcome::Merged
    );
    node.corrupt("Grants", &bystander.doc_id).await;

    let found = probe.lookups(&node, &["wanted"]).await;
    assert_eq!(found[0].1, Ok(vec![wanted.doc_id.clone()]));
    // The unindexed twin still scans, and the scan trips on the bystander.
    assert!(found[1].1.is_err(), "{:?}", found[1].1);
}

#[tokio::test]
async fn find_documents_orders_ids_the_same_on_both_paths() {
    let probe = Probe::new(&["writer", "twin"]);
    let node = Node::with_indexed_grants(probe.clone()).await;
    let peer = signer();
    let mut grants = vec![grant("shared", "a", &peer), grant("shared", "b", &peer)];
    // Merge in descending id order, so short-id order is the reverse of id order.
    grants.sort_by(|x, y| y.doc_id.cmp(&x.doc_id));
    for g in &grants {
        assert_eq!(g.merge(&node, &peer.did).await, MergeOutcome::Merged);
    }
    let mut expected: Vec<String> = grants.iter().map(|g| g.doc_id.clone()).collect();
    expected.sort();

    let found = probe.lookups(&node, &["shared"]).await;
    // writer is indexed, twin is scanned: both sorted by id string.
    assert_eq!(found[0].1, Ok(expected.clone()));
    assert_eq!(found[1].1, Ok(expected));
}

#[tokio::test]
async fn find_documents_still_finds_a_deleted_document_on_both_paths() {
    let probe = Probe::new(&["writer", "twin"]);
    let node = Node::with_indexed_grants(probe.clone()).await;
    let peer = signer();
    let gone = grant("gone", "a", &peer);
    assert_eq!(gone.merge(&node, &peer.did).await, MergeOutcome::Merged);
    let kept = grant("kept", "b", &peer);
    assert_eq!(kept.merge(&node, &peer.did).await, MergeOutcome::Merged);
    node.delete_locally("Grants", &gone.doc_id).await;
    assert_eq!(node.doc_ids("Grants").await, vec![kept.doc_id.clone()]);

    // The delete removed the index entry; the governance read adds the
    // deleted rows back, so the index path and the scan path agree.
    let found = probe.lookups(&node, &["gone", "kept", "absent"]).await;
    let ids = |key: &str| {
        let (_, ids) = found.iter().find(|(found, _)| found == key).unwrap();
        ids.clone().unwrap()
    };
    assert_eq!(ids("writer=gone"), vec![gone.doc_id.clone()]);
    assert_eq!(ids("twin=gone"), vec![gone.doc_id.clone()]);
    assert_eq!(ids("writer=kept"), vec![kept.doc_id.clone()]);
    assert!(ids("writer=absent").is_empty());
}
