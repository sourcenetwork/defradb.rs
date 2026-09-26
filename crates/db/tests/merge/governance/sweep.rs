//! The sweep over unmerged governed composites: the fallback for a deferred
//! verdict the re-drive index cannot hold, or no longer holds.
//!
//! Three routes reach that state: a defer naming nothing, the index at
//! capacity, and a restart, which empties an index that lives in memory.

use super::*;

/// Defers until the composite `0` is held, naming it, like `AwaitComposite`
/// but reading the awaited CID at verdict time so a test can set it later.
struct AwaitNamed(Mutex<Cid>);

#[async_trait]
impl MergeValidator for AwaitNamed {
    async fn validate(
        &self,
        _candidate: &MergeCandidate<'_>,
        view: &dyn MergeView,
    ) -> Result<MergeVerdict, String> {
        let awaited = *self.0.lock().unwrap();
        Ok(if view.composite_fields(&awaited).await?.is_some() {
            MergeVerdict::Accept
        } else {
            MergeVerdict::defer("awaited composite not held", [awaited])
        })
    }
}

/// P1. The index lives in memory, so a restart forgets what a composite
/// awaited. Nothing re-judges it when the awaited composite finally merges.
#[tokio::test]
async fn a_deferred_composite_merges_after_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node");
    let writer = signer();
    let grant = genesis("col-grants", "writer", &writer.did, &writer);
    let note = genesis("col-notes", "grant", "anything", &writer);

    {
        let validator = Arc::new(AwaitNamed(Mutex::new(grant.cid)));
        let node = Node::open(
            RegolithStore::open(&path).unwrap(),
            MergeGovernance::new(["Notes"]).with_validator(validator),
            true,
        )
        .await;
        assert_eq!(
            note.merge(&node, &writer.did).await,
            MergeOutcome::retryable_skip("awaited composite not held")
        );
        assert_eq!(node.handler.deferred_composites(), 1);
        node.db.close().await.unwrap();
    }

    let validator = Arc::new(AwaitNamed(Mutex::new(grant.cid)));
    let node = Node::open(
        RegolithStore::open(&path).unwrap(),
        MergeGovernance::new(["Notes"]).with_validator(validator),
        false,
    )
    .await;
    // The restart forgot the deferral, so the grant's arrival releases nothing.
    assert_eq!(node.handler.deferred_composites(), 0);
    assert_eq!(grant.merge(&node, &writer.did).await, MergeOutcome::Merged);
    assert!(node.doc_ids("Notes").await.is_empty());

    node.handler.sweep_unmerged_governed().await;

    assert_eq!(node.doc_ids("Notes").await, vec![note.doc_id.clone()]);
    assert_eq!(node.forwarded(), vec![note.cid]);
}

/// P2. Beyond the index's capacity a deferral is not indexed at all, so the
/// awaited input's arrival releases nothing.
#[tokio::test]
async fn a_composite_deferred_beyond_the_index_capacity_merges() {
    let writer = signer();
    let grant = genesis("col-grants", "writer", &writer.did, &writer);
    let validator = Arc::new(AwaitNamed(Mutex::new(grant.cid)));
    let node = Node::open(
        RegolithStore::in_memory().unwrap(),
        MergeGovernance::new(["Notes"]).with_validator(validator),
        true,
    )
    .await;
    node.handler.set_deferred_capacity(1);

    let indexed = genesis("col-notes", "grant", "indexed", &writer);
    assert_eq!(
        indexed.merge(&node, &writer.did).await,
        MergeOutcome::retryable_skip("awaited composite not held")
    );
    // The second deferral finds the index full and is dropped by it.
    let beyond = genesis("col-notes", "grant", "beyond", &writer);
    assert_eq!(
        beyond.merge(&node, &writer.did).await,
        MergeOutcome::retryable_skip("awaited composite not held")
    );
    assert_eq!(node.handler.deferred_composites(), 1);

    assert_eq!(grant.merge(&node, &writer.did).await, MergeOutcome::Merged);
    // The indexed one was released by the arrival; the other was not.
    assert_eq!(node.doc_ids("Notes").await, vec![indexed.doc_id.clone()]);

    node.handler.sweep_unmerged_governed().await;

    let mut merged = node.doc_ids("Notes").await;
    merged.sort();
    let mut expected = vec![indexed.doc_id.clone(), beyond.doc_id.clone()];
    expected.sort();
    assert_eq!(merged, expected);
    assert!(node.forwarded().contains(&beyond.cid));
}

/// Defers naming nothing until a marker document exists, then accepts. The
/// host must not strand what a plugin hands it, even when it names no input.
struct AwaitMarker {
    collection: &'static str,
    field: &'static str,
    value: String,
}

#[async_trait]
impl MergeValidator for AwaitMarker {
    async fn validate(
        &self,
        _candidate: &MergeCandidate<'_>,
        view: &dyn MergeView,
    ) -> Result<MergeVerdict, String> {
        let found = view
            .find_documents(
                self.collection,
                self.field,
                &NormalValue::String(self.value.clone()),
            )
            .await?;
        Ok(if found.is_empty() {
            MergeVerdict::defer("marker not held", Vec::<Awaited>::new())
        } else {
            MergeVerdict::Accept
        })
    }
}

/// P3. A defer naming nothing is never indexed, so nothing can release it.
#[tokio::test]
async fn a_composite_deferred_on_nothing_merges_when_the_marker_arrives() {
    let writer = signer();
    let node = Node::with_immutable_grants(Arc::new(AwaitMarker {
        collection: "Grants",
        field: "writer",
        value: writer.did.clone(),
    }))
    .await;
    let note = genesis("col-notes", "grant", "anything", &writer);

    assert_eq!(
        note.merge(&node, &writer.did).await,
        MergeOutcome::retryable_skip("marker not held")
    );
    assert_eq!(node.handler.deferred_composites(), 0);

    let marker = genesis("col-grants", "writer", &writer.did, &writer);
    assert_eq!(marker.merge(&node, &writer.did).await, MergeOutcome::Merged);
    assert!(node.doc_ids("Notes").await.is_empty());

    node.handler.sweep_unmerged_governed().await;

    assert_eq!(node.doc_ids("Notes").await, vec![note.doc_id.clone()]);
    assert_eq!(node.forwarded(), vec![note.cid]);
}

/// Only composite CIDs re-drive on arrival, so a verdict that awaits a field
/// block's CID is indexed under a key nothing ever releases.
#[tokio::test]
async fn a_composite_awaiting_a_field_block_merges_once_it_is_held() {
    let writer = signer();
    let grant = genesis("col-grants", "writer", &writer.did, &writer);
    // The field block of the grant, not its composite.
    let field_cid = grant.blocks[0].0;
    let node = Node::open(
        RegolithStore::in_memory().unwrap(),
        MergeGovernance::new(["Notes"]).with_validator(Arc::new(AwaitNamed(Mutex::new(field_cid)))),
        true,
    )
    .await;
    let note = genesis("col-notes", "grant", "anything", &writer);

    assert_eq!(
        note.merge(&node, &writer.did).await,
        MergeOutcome::retryable_skip("awaited composite not held")
    );
    assert_eq!(node.handler.deferred_composites(), 1);

    // Merging the grant holds its field block, but releases only composite
    // keys, so the note stays deferred.
    assert_eq!(grant.merge(&node, &writer.did).await, MergeOutcome::Merged);
    assert!(node.doc_ids("Notes").await.is_empty());

    node.handler.sweep_unmerged_governed().await;

    assert_eq!(node.doc_ids("Notes").await, vec![note.doc_id.clone()]);
    assert_eq!(node.forwarded(), vec![note.cid]);
}

/// The sweep and an arrival re-drive can reach the same composite in one
/// window; it must merge once and be forwarded once.
#[tokio::test]
async fn a_sweep_and_an_arrival_do_not_double_merge() {
    let writer = signer();
    let grant = genesis("col-grants", "writer", &writer.did, &writer);
    let node = Node::open(
        RegolithStore::in_memory().unwrap(),
        MergeGovernance::new(["Notes"]).with_validator(Arc::new(AwaitNamed(Mutex::new(grant.cid)))),
        true,
    )
    .await;
    let note = genesis("col-notes", "grant", "anything", &writer);

    assert_eq!(
        note.merge(&node, &writer.did).await,
        MergeOutcome::retryable_skip("awaited composite not held")
    );
    // The arrival releases and re-drives the note; the sweep then finds the
    // same composite already merged.
    assert_eq!(grant.merge(&node, &writer.did).await, MergeOutcome::Merged);
    assert_eq!(node.doc_ids("Notes").await, vec![note.doc_id.clone()]);

    node.handler.sweep_unmerged_governed().await;
    node.handler.sweep_unmerged_governed().await;

    assert_eq!(node.doc_ids("Notes").await, vec![note.doc_id.clone()]);
    assert_eq!(node.forwarded(), vec![note.cid]);
}

/// A composite of a collection no validator claims is not the sweep's to
/// judge, and an ungoverned node sweeps nothing at all.
#[tokio::test]
async fn the_sweep_leaves_ungoverned_composites_alone() {
    let writer = signer();
    let node = Node::open(
        RegolithStore::in_memory().unwrap(),
        MergeGovernance::new(["Notes"])
            .with_validator(Arc::new(AwaitNamed(Mutex::new(Cid::default())))),
        true,
    )
    .await;

    // Grants is not claimed, so it merges on its own path and the sweep has
    // nothing to re-judge.
    let grant = genesis("col-grants", "writer", &writer.did, &writer);
    assert_eq!(grant.merge(&node, &writer.did).await, MergeOutcome::Merged);

    assert_eq!(node.handler.sweep_unmerged_governed().await, 0);
    assert!(node.forwarded().is_empty());
}

/// Claiming a collection by name is not installing a merge validator. An
/// application that installs a read or write validator alone claims names too,
/// and until a merge validator exists no composite of a claimed collection can
/// be judged, so the sweep must not walk the unmerged set on its behalf.
#[tokio::test]
async fn the_sweep_does_not_walk_a_claimed_collection_without_a_validator() {
    let writer = signer();
    let node = Node::open(
        RegolithStore::in_memory().unwrap(),
        MergeGovernance::new(["Notes"]),
        true,
    )
    .await;

    let note = genesis("col-notes", "grant", "anything", &writer);
    assert_eq!(
        note.merge(&node, &writer.did).await,
        MergeOutcome::retryable_skip(
            "collection Notes is governed but no merge validator is installed"
        )
    );

    assert_eq!(node.handler.sweep_unmerged_governed().await, 0);
    assert!(node.doc_ids("Notes").await.is_empty());
    assert!(node.forwarded().is_empty());
}

/// Rejects everything it sees.
struct RejectEverything;

#[async_trait]
impl MergeValidator for RejectEverything {
    async fn validate(
        &self,
        _candidate: &MergeCandidate<'_>,
        _view: &dyn MergeView,
    ) -> Result<MergeVerdict, String> {
        Ok(MergeVerdict::reject("forged"))
    }
}

/// Defers on first sight, naming a composite; rejects on every later look.
struct DeferThenReject(Cid, std::sync::atomic::AtomicBool);

#[async_trait]
impl MergeValidator for DeferThenReject {
    async fn validate(
        &self,
        _candidate: &MergeCandidate<'_>,
        _view: &dyn MergeView,
    ) -> Result<MergeVerdict, String> {
        if self.1.swap(true, std::sync::atomic::Ordering::AcqRel) {
            Ok(MergeVerdict::reject("forged"))
        } else {
            Ok(MergeVerdict::defer("first look", [self.0]))
        }
    }
}

/// A rejected composite stays in the blockstore's unmerged set, and a reject
/// rests on present bytes, so no tick can change it. The sweep must not
/// re-judge it: each one would otherwise spend the budget that exists for
/// composites a verdict deferred.
#[tokio::test]
async fn the_sweep_does_not_rejudge_a_rejected_composite() {
    let node = Node::open(
        RegolithStore::in_memory().unwrap(),
        MergeGovernance::new(["Notes"]).with_validator(Arc::new(RejectEverything)),
        true,
    )
    .await;
    let writer = signer();
    let note = genesis("col-notes", "grant", "anything", &writer);
    assert_eq!(
        note.merge(&node, &writer.did).await,
        MergeOutcome::rejected("forged")
    );
    assert_eq!(node.handler.sweep_unmerged_governed().await, 0);
    assert_eq!(node.handler.sweep_unmerged_governed().await, 0);
}

/// A composite deferred on arrival and rejected when re-driven is remembered
/// the same way: the sweep re-judges it once, then leaves it.
#[tokio::test]
async fn a_composite_rejected_on_redrive_is_not_swept_again() {
    let writer = signer();
    let grant = genesis("col-grants", "writer", &writer.did, &writer);
    let note = genesis("col-notes", "grant", "anything", &writer);
    let node = Node::open(
        RegolithStore::in_memory().unwrap(),
        MergeGovernance::new(["Notes"]).with_validator(Arc::new(DeferThenReject(
            grant.cid,
            std::sync::atomic::AtomicBool::new(false),
        ))),
        true,
    )
    .await;
    assert_eq!(
        note.merge(&node, &writer.did).await,
        MergeOutcome::retryable_skip("first look")
    );
    assert_eq!(node.handler.sweep_unmerged_governed().await, 1);
    assert_eq!(node.handler.sweep_unmerged_governed().await, 0);
    assert!(
        node.forwarded().is_empty(),
        "a rejected re-drive was forwarded"
    );
}
