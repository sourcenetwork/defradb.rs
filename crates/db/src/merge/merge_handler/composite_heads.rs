use super::composite::{CompositeMergeContext, CompositeMergeState};
use super::*;

impl<S: Store, B: blockstore::Blockstore> DbMergeHandler<S, B> {
    pub(crate) async fn update_heads(
        &self,
        headstore: &NamespaceView,
        context: &CompositeMergeContext<'_, '_>,
        state: &CompositeMergeState,
    ) -> std::result::Result<(), MergeError> {
        let priority_bytes = encode_priority_varint(context.payload.priority);

        if let Some(heads) = &context.block.heads {
            for parent_cid in heads {
                let parent_key = storage::keys::headstore::HeadstoreDocKey::new(
                    context.doc_short_id,
                    "C",
                    *parent_cid,
                );
                headstore
                    .delete(
                        &<storage::keys::headstore::HeadstoreDocKey as storage::corekv::Key>::bytes(
                            &parent_key,
                        ),
                    )
                    .await
                    .map_err(|e| MergeError::Storage(e.to_string()))?;
            }
        }

        let composite_head_key =
            storage::keys::headstore::HeadstoreDocKey::new(context.doc_short_id, "C", *context.cid);
        headstore
            .set(
                &<storage::keys::headstore::HeadstoreDocKey as storage::corekv::Key>::bytes(
                    &composite_head_key,
                ),
                &priority_bytes,
            )
            .await
            .map_err(|e| MergeError::Storage(e.to_string()))?;

        let composite_priority_key = storage::keys::headstore::HeadstorePriorityKey::new(
            context.doc_short_id,
            context.payload.priority,
            *context.cid,
        );
        headstore
            .set(
                &<storage::keys::headstore::HeadstorePriorityKey as storage::corekv::Key>::bytes(
                    &composite_priority_key,
                ),
                &[],
            )
            .await
            .map_err(|e| MergeError::Storage(e.to_string()))?;

        if let Some(links) = &context.block.links {
            for dag_link in links {
                if !state.linked_field_cids.contains(&dag_link.link) {
                    continue;
                }
                if let Some(parent_cids) = state.field_block_heads.get(&dag_link.name) {
                    for parent_cid in parent_cids {
                        let parent_key = storage::keys::headstore::HeadstoreDocKey::new(
                            context.doc_short_id,
                            &dag_link.name,
                            *parent_cid,
                        );
                        headstore
                            .delete(
                                &<storage::keys::headstore::HeadstoreDocKey as storage::corekv::Key>::bytes(
                                    &parent_key,
                                ),
                            )
                            .await.map_err(|e| MergeError::Storage(e.to_string()))?;
                    }
                }

                let field_head_key = storage::keys::headstore::HeadstoreDocKey::new(
                    context.doc_short_id,
                    &dag_link.name,
                    dag_link.link,
                );
                headstore
                    .set(
                        &<storage::keys::headstore::HeadstoreDocKey as storage::corekv::Key>::bytes(
                            &field_head_key,
                        ),
                        &priority_bytes,
                    )
                    .await
                    .map_err(|e| MergeError::Storage(e.to_string()))?;

                let field_priority_key = storage::keys::headstore::HeadstorePriorityKey::new(
                    context.doc_short_id,
                    context.payload.priority,
                    dag_link.link,
                );
                headstore
                    .set(
                        &<storage::keys::headstore::HeadstorePriorityKey as storage::corekv::Key>::bytes(
                            &field_priority_key,
                        ),
                        &[],
                    )
                    .await.map_err(|e| MergeError::Storage(e.to_string()))?;
            }
        }
        Ok(())
    }
}
