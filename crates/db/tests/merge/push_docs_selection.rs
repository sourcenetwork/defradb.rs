use super::push_docs_replay::ConnectedPeerTransport;
use db::merge::{
    document_heads_acknowledged, push_existing_docs, push_existing_docs_by_id,
    PushExistingDocsSeOptions,
};
use db::{AutoCommitMutator, DB};
use document::Document;
use p2p::transport::PeerId;
use query::mutator::DocMutator;
use schema::{CollectionVersion, FieldDescription, FieldKind};
use std::process::Command;
use std::sync::Arc;

#[test]
#[ignore = "requires Lean; executed explicitly by the Lean Conformance CI job"]
fn selected_document_transfers_match_executed_model() {
    let output = Command::new("lake")
        .args(["env", "lean", "--run", "DocumentPush.lean"])
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../proofs/lean"))
        .output()
        .expect("execute document push model");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases = String::from_utf8(output.stdout).unwrap();
    let mut count = 0;
    for line in cases.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        let [explicit, present, acknowledged, owner, expected, retained] = fields.as_slice() else {
            panic!("invalid model case: {line}")
        };
        workers::block_on(exercise(
            explicit.parse().unwrap(),
            present.parse().unwrap(),
            acknowledged.parse().unwrap(),
            owner,
            expected,
            retained.parse().unwrap(),
            false,
        ))
        .unwrap();
        count += 1;
    }
    assert!(count > 0, "model emitted no transfer cases");
}

async fn exercise(
    explicit: bool,
    present: bool,
    acknowledged: bool,
    owner: &str,
    expected: &str,
    retained: bool,
    remove_on_reply: bool,
) {
    let store = Arc::new(storage::RegolithStore::in_memory().unwrap());
    let db = Arc::new(DB::from_arc(store).unwrap());
    db.set_network(edge::testing::edge());
    db.create_collection(CollectionVersion::new(
        "PushNote",
        "v1",
        "col-push-note",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id()),
            FieldDescription::new("2", "body", FieldKind::string()),
        ],
    ))
    .await
    .unwrap();
    let mutator = AutoCommitMutator::new(db.clone());
    let mut ids = Vec::new();
    for body in ["selected", "unselected"] {
        let mut doc = Document::new();
        doc.set("body", body.to_owned());
        ids.push(
            mutator
                .create("PushNote", doc)
                .await
                .unwrap()
                .doc_id
                .to_string(),
        );
    }
    let peer = PeerId::new("receiver".into());
    let peerstore = db.peerstore();
    if present {
        let info = p2p::ReplicatorInfo::from_raw(
            peer.as_str().to_owned(),
            vec!["col-push-note".into()],
            vec![],
        );
        peerstore
            .create_replicator(peer.as_str(), &info.to_bytes().unwrap())
            .await
            .unwrap();
    }
    let (requests, received) = regolith::sync::bounded(16);
    let mut transport =
        ConnectedPeerTransport::new("sender", peer.as_str()).with_replies(acknowledged, requests);
    if remove_on_reply {
        transport = transport.removing_subscription_on_reply(Arc::new(db.peerstore()));
    }
    if explicit {
        transport = transport.requiring_selected_authority(Arc::new(db.peerstore()));
    }
    let filters = p2p::ReplicationFilters::new();
    let authority = p2p::sync::HeadHintCarAuthority::default();
    let result = if explicit {
        push_existing_docs_by_id(
            &transport,
            &db,
            None,
            &peer,
            &[("PushNote".into(), ids[0].clone())],
            &filters,
            PushExistingDocsSeOptions::default(),
            Default::default(),
            &p2p::EqOnlyFilterMatcher,
            &authority,
        )
        .await
    } else {
        push_existing_docs(
            &transport,
            &db,
            None,
            &peer,
            &["PushNote".into()],
            &filters,
            PushExistingDocsSeOptions::default(),
            &p2p::EqOnlyFilterMatcher,
            &authority,
        )
        .await
    };
    let mut sent = Vec::new();
    while let Some(request) = received.try_recv() {
        assert!(ids.contains(&request.doc_id));
        if explicit {
            assert_eq!(request.doc_id, ids[0]);
        }
        sent.push(request);
    }
    let retries = peerstore.get_retry_documents(peer.as_str()).await.unwrap();
    assert_eq!(
        peerstore.has_replicator(peer.as_str()).await.unwrap(),
        present && !remove_on_reply
    );
    if owner != "replicator" {
        assert!(
            retries.is_empty(),
            "explicit={explicit} present={present} acknowledged={acknowledged} owner={owner}"
        );
    }
    let observed = document_heads_acknowledged(&db, peer.as_str(), "PushNote", &ids)
        .await
        .unwrap();
    assert_eq!(
        observed,
        vec![retained, retained && !explicit],
        "retained heads: explicit={explicit} present={present} acknowledged={acknowledged}"
    );
    match expected {
        "refused" => {
            assert!(result.is_err());
            assert!(sent.is_empty());
        }
        "failed" => {
            assert!(result.is_err(), "{result:?}");
            assert!(!sent.is_empty());
        }
        "acknowledged" => {
            assert!(result.is_ok(), "{result:?}");
            assert!(!sent.is_empty());
            assert!(retries.is_empty());
        }
        "retainedRetry" => {
            assert!(result.is_ok(), "{result:?}");
            assert!(!sent.is_empty());
            assert!(!retries.is_empty());
        }
        _ => panic!("unknown model outcome: {expected}"),
    }
}

#[test]
fn selected_delivery_records_unchanged_acknowledged_heads() {
    workers::block_on(exercise(
        true,
        true,
        true,
        "selected",
        "acknowledged",
        true,
        false,
    ))
    .unwrap();
}

#[test]
fn selected_delivery_survives_subscription_removal_without_retaining_state() {
    workers::block_on(exercise(
        true,
        true,
        true,
        "selected",
        "acknowledged",
        false,
        true,
    ))
    .unwrap();
}
