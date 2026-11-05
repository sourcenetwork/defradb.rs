use super::composite::{CompositeMergeContext, CompositeMergeState};
use super::*;
use storage::keys::headstore::HeadstoreDocKey;

impl<S: Store, B: blockstore::Blockstore> DbMergeHandler<S, B> {
    /// Publish the composite and each field block it merged into the
    /// document's head set, in one operand.
    pub(crate) async fn update_heads(
        &self,
        headstore: &NamespaceView,
        context: &CompositeMergeContext<'_, '_>,
        state: &mut CompositeMergeState,
    ) -> std::result::Result<(), MergeError> {
        let storage_error = |e: storage::corekv::Error| MergeError::Storage(e.to_string());
        let set = state
            .reads
            .heads(headstore, context.doc_short_id)
            .await
            .map_err(storage_error)?;
        self.merge_kinds.push(set.live().next().is_none());
        let mut write = storage::merge::head_set::Write::default();
        let head = HeadstoreDocKey::new(context.doc_short_id, "C", *context.cid);
        crate::block::heads::document::publish(
            headstore,
            set,
            &mut write,
            &head,
            context.payload.priority,
            context.block.heads.as_deref().unwrap_or_default(),
        )
        .await
        .map_err(storage_error)?;

        if let Some(links) = &context.block.links {
            for link in links {
                if !state.linked_field_cids.contains(&link.link) {
                    continue;
                }
                // An Immutable block is the head of every field it carries, so
                // a peer holding one of them as mutable chains its update to it.
                let names: Vec<&str> = match state.immutable_block_fields.get(&link.link) {
                    Some(fields) => fields.iter().map(String::as_str).collect(),
                    None => vec![link.name.as_str()],
                };
                let Some((first, others)) = names.split_first() else {
                    continue;
                };
                let priority = state.field_block_priorities[&link.link];
                let head = HeadstoreDocKey::new(context.doc_short_id, *first, link.link);
                let published = crate::block::heads::document::publish(
                    headstore,
                    set,
                    &mut write,
                    &head,
                    priority,
                    state
                        .field_block_heads
                        .get(&link.name)
                        .map(Vec::as_slice)
                        .unwrap_or_default(),
                )
                .await
                .map_err(storage_error)?;
                if published {
                    for other in others {
                        write.add(
                            &crate::block::heads::document::name(other, &link.link),
                            priority,
                        );
                    }
                }
            }
        }
        crate::block::heads::document::commit(headstore, context.doc_short_id, &write)
            .await
            .map_err(storage_error)?;
        state.reads.wrote_heads(context.doc_short_id, &write);
        Ok(())
    }
}
