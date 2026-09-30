//! Emission: what a verdict may write beside itself. An emitted record is the
//! same CID on every replica that finds the fact, is merged and forwarded like
//! any re-driven block, is judged if its collection is claimed, and comes out
//! of a defer as readily as out of an accept.

use super::*;
use db::merge::governance::{Emission, Judged, MAX_EMISSION_DEPTH};

fn record_of(cid: &Cid) -> Emission {
    Emission::new("Records").field("of", NormalValue::String(cid.to_string()))
}

fn is_record(candidate: &MergeCandidate<'_>) -> bool {
    candidate.collection.name == "Records"
}

/// A record is accepted only as the host emits it: unsigned.
fn judge_record(candidate: &MergeCandidate<'_>) -> MergeVerdict {
    if matches!(candidate.signature, SignatureStatus::Unsigned) {
        MergeVerdict::Accept
    } else {
        MergeVerdict::reject("a record is never signed")
    }
}

/// Accepts every note and emits a record of it.
struct RecordsNotes;

#[async_trait]
impl MergeValidator for RecordsNotes {
    async fn validate(
        &self,
        candidate: &MergeCandidate<'_>,
        view: &dyn MergeView,
    ) -> Result<MergeVerdict, String> {
        Ok(self.judge(candidate, view).await?.verdict)
    }

    async fn judge(
        &self,
        candidate: &MergeCandidate<'_>,
        _view: &dyn MergeView,
    ) -> Result<Judged, String> {
        if is_record(candidate) {
            return Ok(judge_record(candidate).into());
        }
        Ok(Judged::new(MergeVerdict::Accept).emitting(record_of(candidate.cid)))
    }
}

/// Defers every note naming nothing, the way a fork is found mid-verdict,
/// and emits a record of it each time it is judged.
struct RecordsThenHolds;

#[async_trait]
impl MergeValidator for RecordsThenHolds {
    async fn validate(
        &self,
        candidate: &MergeCandidate<'_>,
        view: &dyn MergeView,
    ) -> Result<MergeVerdict, String> {
        Ok(self.judge(candidate, view).await?.verdict)
    }

    async fn judge(
        &self,
        candidate: &MergeCandidate<'_>,
        _view: &dyn MergeView,
    ) -> Result<Judged, String> {
        if is_record(candidate) {
            return Ok(judge_record(candidate).into());
        }
        Ok(
            Judged::new(MergeVerdict::defer("holding", std::iter::empty::<Cid>()))
                .emitting(record_of(candidate.cid)),
        )
    }
}

/// Emits a record of everything it judges, records included.
struct RecordsEverything;

#[async_trait]
impl MergeValidator for RecordsEverything {
    async fn validate(
        &self,
        candidate: &MergeCandidate<'_>,
        view: &dyn MergeView,
    ) -> Result<MergeVerdict, String> {
        Ok(self.judge(candidate, view).await?.verdict)
    }

    async fn judge(
        &self,
        candidate: &MergeCandidate<'_>,
        _view: &dyn MergeView,
    ) -> Result<Judged, String> {
        let verdict = if is_record(candidate) {
            judge_record(candidate)
        } else {
            MergeVerdict::Accept
        };
        Ok(Judged::new(verdict).emitting(record_of(candidate.cid)))
    }
}

/// Emits into a collection the node does not have.
struct RecordsNowhere;

#[async_trait]
impl MergeValidator for RecordsNowhere {
    async fn validate(
        &self,
        candidate: &MergeCandidate<'_>,
        view: &dyn MergeView,
    ) -> Result<MergeVerdict, String> {
        Ok(self.judge(candidate, view).await?.verdict)
    }

    async fn judge(
        &self,
        candidate: &MergeCandidate<'_>,
        _view: &dyn MergeView,
    ) -> Result<Judged, String> {
        Ok(Judged::new(MergeVerdict::Accept)
            .emitting(Emission::new("Nowhere").field("of", candidate.cid.to_string())))
    }
}

/// Grants, Notes and a Records collection with one string field `of`.
async fn node_with_records(validator: Arc<dyn MergeValidator>, claimed: &[&str]) -> Node {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(
        DB::open_from_arc_with_options(store.clone(), DbOptions::default())
            .await
            .unwrap(),
    );
    for (name, id, field) in [
        ("Grants", "col-grants", "writer"),
        ("Notes", "col-notes", "grant"),
        ("Records", "col-records", "of"),
    ] {
        db.create_collection(CollectionVersion::new(
            name,
            id,
            id,
            vec![
                FieldDescription::new("1", "_docID", FieldKind::doc_id()),
                FieldDescription::new("2", field, FieldKind::string()),
            ],
        ))
        .await
        .unwrap();
    }
    db.set_merge_governance(
        MergeGovernance::new(claimed.iter().copied()).with_validator(validator),
    );
    Node::assemble(db, store)
}

/// E1. A record emitted with an accept is merged into its collection and
/// forwarded like a re-driven merge.
#[tokio::test]
async fn a_record_emitted_with_an_accept_merges_and_is_forwarded() {
    let writer = signer();
    let note = genesis("col-notes", "grant", "anything", &writer);
    let node = node_with_records(Arc::new(RecordsNotes), &["Notes"]).await;

    assert_eq!(note.merge(&node, &writer.did).await, MergeOutcome::Merged);

    let records = node.doc_ids("Records").await;
    assert_eq!(records.len(), 1);
    let forwarded = node.forwarded_with_ids();
    assert_eq!(forwarded.len(), 1);
    assert_eq!(
        db::block::builder::derive_doc_id(&forwarded[0].0),
        records[0]
    );
    // The sink pushes to replicators only under a collection id.
    assert_eq!(forwarded[0].1, "col-records");
    assert_eq!(
        field_value(&node, "Records", &records[0], "of").await,
        Some(note.cid.to_string())
    );
}

/// E2. A fact found during a defer is emitted, and re-judging the deferred
/// composite finds the same fact and adds nothing.
#[tokio::test]
async fn a_record_emitted_with_a_defer_is_written_once() {
    let writer = signer();
    let note = genesis("col-notes", "grant", "anything", &writer);
    let node = node_with_records(Arc::new(RecordsThenHolds), &["Notes"]).await;

    assert_eq!(
        note.merge(&node, &writer.did).await,
        MergeOutcome::retryable_skip("holding")
    );
    assert!(node.doc_ids("Notes").await.is_empty());
    assert_eq!(node.doc_ids("Records").await.len(), 1);

    // The defer named nothing, so only the sweep re-judges it; each pass
    // emits the record again and finds it held.
    assert_eq!(node.handler.sweep_unmerged_governed().await, 1);
    assert_eq!(node.handler.sweep_unmerged_governed().await, 1);
    assert!(node.doc_ids("Notes").await.is_empty());
    assert_eq!(node.doc_ids("Records").await.len(), 1);
    assert_eq!(node.forwarded().len(), 1);
}

/// E3. The same fact is the same record on two replicas: no signature, no
/// node-local value in the bytes, so the CID and the document id agree.
#[tokio::test]
async fn the_same_fact_is_the_same_record_on_two_nodes() {
    let writer = signer();
    let note = genesis("col-notes", "grant", "anything", &writer);
    let a = node_with_records(Arc::new(RecordsNotes), &["Notes"]).await;
    let b = node_with_records(Arc::new(RecordsNotes), &["Notes"]).await;

    assert_eq!(note.merge(&a, &writer.did).await, MergeOutcome::Merged);
    assert_eq!(note.merge(&b, &writer.did).await, MergeOutcome::Merged);

    let records = a.doc_ids("Records").await;
    assert_eq!(records.len(), 1);
    assert_eq!(b.doc_ids("Records").await, records);
    assert_eq!(a.forwarded(), b.forwarded());
}

/// E4. A record into a claimed collection is judged like any composite: the
/// emitted one is accepted unsigned, and a signed look-alike a peer pushes
/// is refused by the same validator.
#[tokio::test]
async fn a_record_into_a_claimed_collection_is_judged() {
    let writer = signer();
    let note = genesis("col-notes", "grant", "anything", &writer);
    let node = node_with_records(Arc::new(RecordsNotes), &["Notes", "Records"]).await;

    assert_eq!(note.merge(&node, &writer.did).await, MergeOutcome::Merged);
    assert_eq!(node.doc_ids("Records").await.len(), 1);

    let forged = genesis("col-records", "of", &note.cid.to_string(), &writer);
    assert_ne!(forged.merge(&node, &writer.did).await, MergeOutcome::Merged);
    assert_eq!(node.doc_ids("Records").await.len(), 1);
}

/// E5. A rule that records its own records stops at `MAX_EMISSION_DEPTH`.
#[tokio::test]
async fn a_chain_of_records_stops_at_the_depth_bound() {
    let writer = signer();
    let note = genesis("col-notes", "grant", "anything", &writer);
    let node = node_with_records(Arc::new(RecordsEverything), &["Notes", "Records"]).await;

    assert_eq!(note.merge(&node, &writer.did).await, MergeOutcome::Merged);

    assert_eq!(node.doc_ids("Records").await.len(), MAX_EMISSION_DEPTH);
    assert_eq!(node.forwarded().len(), MAX_EMISSION_DEPTH);
}

/// E6. An emission the node cannot write is dropped; the verdict it came
/// with stands.
#[tokio::test]
async fn an_emission_into_an_unknown_collection_never_fails_the_verdict() {
    let writer = signer();
    let note = genesis("col-notes", "grant", "anything", &writer);
    let node = node_with_records(Arc::new(RecordsNowhere), &["Notes"]).await;

    assert_eq!(note.merge(&node, &writer.did).await, MergeOutcome::Merged);
    assert_eq!(node.doc_ids("Notes").await, vec![note.doc_id.clone()]);
    assert!(node.forwarded().is_empty());
}

/// E7. The batch path judges through its own transaction and writes what
/// it emitted at the end of the batch.
#[tokio::test]
async fn a_record_emitted_during_a_batch_merge_is_written() {
    let writer = signer();
    let note = genesis("col-notes", "grant", "anything", &writer);
    let node = node_with_records(Arc::new(RecordsNotes), &["Notes"]).await;
    note.store(&node).await;

    let outcomes = node
        .handler
        .handle_block_batch(&[merge_block(&note, &writer.did)])
        .await;
    assert!(
        matches!(outcomes.as_slice(), [Ok(MergeOutcome::Merged)]),
        "{outcomes:?}"
    );
    assert_eq!(node.doc_ids("Records").await.len(), 1);
}

/// E8. A record naming a field its collection lacks is dropped; the verdict
/// it came with stands.
#[tokio::test]
async fn an_emission_with_a_field_the_schema_lacks_never_fails_the_verdict() {
    struct RecordsBadField;

    #[async_trait]
    impl MergeValidator for RecordsBadField {
        async fn validate(
            &self,
            candidate: &MergeCandidate<'_>,
            view: &dyn MergeView,
        ) -> Result<MergeVerdict, String> {
            Ok(self.judge(candidate, view).await?.verdict)
        }

        async fn judge(
            &self,
            candidate: &MergeCandidate<'_>,
            _view: &dyn MergeView,
        ) -> Result<Judged, String> {
            Ok(Judged::new(MergeVerdict::Accept)
                .emitting(Emission::new("Records").field("nope", candidate.cid.to_string())))
        }
    }

    let writer = signer();
    let note = genesis("col-notes", "grant", "anything", &writer);
    let node = node_with_records(Arc::new(RecordsBadField), &["Notes"]).await;

    assert_eq!(note.merge(&node, &writer.did).await, MergeOutcome::Merged);
    assert_eq!(node.doc_ids("Notes").await, vec![note.doc_id.clone()]);
    assert!(node.doc_ids("Records").await.is_empty());
}
