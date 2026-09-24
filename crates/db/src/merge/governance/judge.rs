use cid::Cid;
use defra_core::block::{Block, CompositeDeltaPayload, CrdtDelta};
use defra_core::merge::{BlockMetadata, MergeBlock, MergeOutcome};
use document::NormalValue;
use schema::CollectionVersion;
use storage::corekv::Store;

use super::awaited::{is_immutable_scalar_field, Awaited, WaitKey};
use super::validator::MergeCandidate;
use super::verdict::MergeVerdict;
use super::view::DbMergeView;
use crate::merge::merge_handler::{DbMergeHandler, MergeError};

/// One composite frame as dispatch hands it to the judge.
pub(crate) struct GovernedFrame<'a> {
    pub cid: &'a Cid,
    pub block: &'a Block,
    pub payload: &'a CompositeDeltaPayload,
    pub doc_id: &'a str,
    pub collection: &'a CollectionVersion,
}

pub(crate) enum Judgement {
    Ungoverned,
    Accept,
    Verdict {
        outcome: MergeOutcome,
        awaiting: Vec<WaitKey>,
    },
}

impl<S: Store, B: blockstore::Blockstore> DbMergeHandler<S, B> {
    pub(crate) fn is_governed(&self, collection: &CollectionVersion) -> bool {
        self.db
            .merge_governance()
            .is_some_and(|governance| governance.governs(collection))
    }

    /// Judge one composite frame of a governed collection. The signature is
    /// verified here from the block itself, so a frame reached through
    /// recovery, where dispatch skips verification, is judged the same way.
    pub(crate) async fn judge_governed(
        &self,
        frame: GovernedFrame<'_>,
    ) -> Result<Judgement, MergeError> {
        let GovernedFrame {
            cid,
            block,
            payload,
            doc_id,
            collection,
        } = frame;
        let Some(governance) = self.db.merge_governance() else {
            return Ok(Judgement::Ungoverned);
        };
        if !governance.governs(collection) {
            return Ok(Judgement::Ungoverned);
        }
        let Some(validator) = governance.validator() else {
            return Ok(Judgement::Verdict {
                outcome: MergeOutcome::retryable_skip(format!(
                    "collection {} is governed but no merge validator is installed",
                    collection.name
                )),
                awaiting: Vec::new(),
            });
        };

        let candidate = MergeCandidate {
            cid,
            block,
            payload,
            doc_id,
            collection,
            is_genesis: block.heads.as_deref().is_none_or(<[Cid]>::is_empty),
            signature: self.frame_signature(cid, block).await?,
        };
        let view = DbMergeView::new(self);
        let verdict = validator.validate(&candidate, &view).await;
        view.finish().await;
        let verdict = verdict.map_err(|error| {
            MergeError::MergeFailed(format!("merge validator failed on {cid}: {error}"))
        })?;
        tracing::debug!(%cid, %doc_id, collection = %collection.name, ?verdict, "Governed composite judged");
        if matches!(verdict, MergeVerdict::Reject { .. }) {
            self.rejected_governed.insert(*cid, ());
        }

        Ok(match verdict.into_outcome() {
            (None, _) => Judgement::Accept,
            (Some(outcome), awaiting) => Judgement::Verdict {
                outcome,
                awaiting: self.wait_keys(awaiting)?,
            },
        })
    }

    /// Index keys for a verdict's awaited inputs. An awaited field of a local
    /// collection that is not an `@immutable` scalar LWW field is a validator
    /// error: its value could differ between replicas. A collection this node
    /// does not hold yet is not an error: `find_documents` answers it empty
    /// and the validator defers on that, so the key must be indexed for the
    /// composite to be re-driven when the collection and a match arrive.
    fn wait_keys(&self, awaiting: Vec<Awaited>) -> Result<Vec<WaitKey>, MergeError> {
        awaiting
            .into_iter()
            .map(|awaited| match awaited {
                Awaited::Composite(cid) => Ok(WaitKey::Composite(cid)),
                Awaited::ImmutableField {
                    collection,
                    field,
                    value,
                } => {
                    if let Some(local) = self.db.get_collection(&collection)? {
                        if !is_immutable_scalar_field(local.schema(), &field) {
                            return Err(MergeError::MergeFailed(format!(
                                "awaited field '{field}' in collection '{collection}' must be an @immutable scalar LWW field"
                            )));
                        }
                    }
                    WaitKey::immutable_field(&collection, &field, &value)
                        .map_err(MergeError::MergeFailed)
                }
            })
            .collect()
    }

    /// Release composites waiting on a composite that just merged: on its CID,
    /// and, when any deferred composite awaits a field value, on each
    /// `@immutable` scalar LWW value it sets.
    pub(crate) async fn release_merged_composite(&self, cid: &Cid, block: Option<&Block>) {
        // Its own entry first: a judgement that deferred this composite may
        // have filed it after another accepted and merged it.
        self.deferred.forget(cid);
        let mut keys = vec![WaitKey::Composite(*cid)];
        if self.deferred.awaits_fields() {
            match self.immutable_field_keys(cid, block).await {
                Ok(fields) => keys.extend(fields),
                Err(error) => {
                    tracing::debug!(%cid, %error, "Merged composite's immutable fields unreadable for re-drive")
                }
            }
        }
        self.deferred.release(keys);
    }

    async fn immutable_field_keys(
        &self,
        cid: &Cid,
        block: Option<&Block>,
    ) -> Result<Vec<WaitKey>, MergeError> {
        let loaded;
        let block = match block {
            Some(block) => block,
            None => {
                let Some(data) = self
                    .blockstore
                    .get(cid)
                    .await
                    .map_err(|error| MergeError::Storage(error.to_string()))?
                else {
                    return Ok(Vec::new());
                };
                loaded = Block::from_dag_cbor(&data)
                    .map_err(|error| MergeError::BlockDecode(error.to_string()))?;
                &loaded
            }
        };
        let CrdtDelta::Composite(payload) = &block.delta else {
            return Ok(Vec::new());
        };
        let Some(collection) = self
            .block_collection(&payload.schema_version_id, None)
            .await?
        else {
            return Ok(Vec::new());
        };
        let schema = collection.schema();
        let mut keys = Vec::new();
        for link in block.links.iter().flatten() {
            if !is_immutable_scalar_field(schema, &link.name) {
                continue;
            }
            let Some(data) = self
                .blockstore
                .get(&link.link)
                .await
                .map_err(|error| MergeError::Storage(error.to_string()))?
            else {
                continue;
            };
            let field = Block::from_dag_cbor(&data)
                .map_err(|error| MergeError::BlockDecode(error.to_string()))?;
            let CrdtDelta::Lww(lww) = &field.delta else {
                continue;
            };
            if field.encryption.is_some() || lww.data.is_empty() {
                continue;
            }
            let Ok(value) = ciborium::from_reader::<NormalValue, _>(lww.data.as_slice()) else {
                continue;
            };
            keys.push(
                WaitKey::immutable_field(&schema.name, &link.name, &value)
                    .map_err(MergeError::MergeFailed)?,
            );
        }
        Ok(keys)
    }

    /// Index the composite being merged under the CIDs a frame of its DAG is
    /// waiting for, keeping only what re-drive needs from the carrier's
    /// metadata.
    pub(crate) fn index_deferred(
        &self,
        root: &Cid,
        doc_id: &str,
        metadata: &BlockMetadata<'_>,
        awaiting: Vec<WaitKey>,
    ) {
        self.deferred.defer(
            MergeBlock {
                cid: *root,
                block_data: bytes::Bytes::new(),
                doc_id: doc_id.to_string(),
                // Only what the carrier said. A recovery merge carries no
                // collection id, and the empty string keeps the re-driven merge
                // off the replicator push, as the first attempt was; the schema
                // version id is not an id replicators subscribe to.
                collection_id: metadata.collection_id.unwrap_or_default().to_string(),
                creator: metadata.creator.unwrap_or_default().to_string(),
                sender_peer: metadata.sender_peer.map(str::to_string),
                is_explicit_replicator: metadata.is_explicit_replicator,
                explicit_replay_authorization: metadata.explicit_replay_authorization.clone(),
                verified_creator: None,
            },
            awaiting,
        );
    }

    /// An update whose document cannot be identified because an ancestor is
    /// not held. In a governed collection, resolved from the block's own
    /// schema version, that is a missing input: defer on the first ancestor
    /// not held, so its arrival re-drives the update. Otherwise the error
    /// stands.
    pub(crate) async fn defer_unresolved_document(
        &self,
        cid: &Cid,
        block: &Block,
        payload: &CompositeDeltaPayload,
        metadata: &BlockMetadata<'_>,
        error: MergeError,
    ) -> Result<MergeOutcome, MergeError> {
        let Some(governance) = self.db.merge_governance() else {
            return Err(error);
        };
        let Some(collection) = self
            .block_collection(&payload.schema_version_id, None)
            .await?
        else {
            return Err(error);
        };
        if !governance.governs(collection.schema()) {
            return Err(error);
        }
        // A claimed collection with no validator defers everything and indexes
        // nothing, as `judge_governed` does: nothing may reach the deferral
        // machinery until a validator is installed.
        if governance.validator().is_none() {
            return Ok(MergeOutcome::retryable_skip(format!(
                "collection {} is governed but no merge validator is installed",
                collection.name()
            )));
        }
        let Some(missing) = self.first_missing_ancestor(cid, block).await? else {
            return Err(error);
        };
        self.index_deferred(
            cid,
            metadata.doc_id.unwrap_or_default(),
            metadata,
            vec![WaitKey::Composite(missing)],
        );
        Ok(MergeOutcome::retryable_skip("document genesis not held"))
    }

    async fn first_missing_ancestor(
        &self,
        cid: &Cid,
        block: &Block,
    ) -> Result<Option<Cid>, MergeError> {
        let mut pending: Vec<(Cid, usize)> = block
            .heads
            .iter()
            .flatten()
            .map(|head| (*head, 1))
            .collect();
        let mut visited = rapidhash::RapidHashSet::default();
        while let Some((ancestor, depth)) = pending.pop() {
            self.ensure_merge_depth(cid, depth)?;
            if !visited.insert(ancestor) {
                continue;
            }
            let data = match self.blockstore.get(&ancestor).await {
                Ok(Some(data)) => data,
                Ok(None) => return Ok(Some(ancestor)),
                Err(error) => return Err(MergeError::Storage(error.to_string())),
            };
            let parent = Block::from_dag_cbor(&data)
                .map_err(|error| MergeError::BlockDecode(error.to_string()))?;
            pending.extend(parent.heads.iter().flatten().map(|head| (*head, depth + 1)));
        }
        Ok(None)
    }

    /// Deferred composites currently indexed for re-drive.
    pub fn deferred_composites(&self) -> usize {
        self.deferred.len()
    }
}
