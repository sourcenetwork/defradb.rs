//! What one merge attempt reads of its document before it writes it: the
//! head set and the stored value, each read once and shared by the block's
//! field merges, its value write and its heads write.
//!
//! Both keys take blind merges, so a read of either inside a transaction
//! folds every operand the transaction has merged into it so far. A walk
//! that merges a DAG's blocks in one transaction would otherwise pay that
//! fold for every field of every block. Within a block nothing writes the
//! head set before `update_heads`, and only a counter merge writes the stored
//! value before `persist_merged_document` (an LWW field writes keys of its
//! own), so a read kept here is what a fresh read would return until the
//! writer that invalidates it. A head write applies its exact operand to the
//! decoded snapshot, so subsequent blocks in the same page reuse that state.
//! Neither cache outlives the engine attempt.

use datastore::NamespaceView;
use document::{DocID, Document};
use storage::merge::head_set::{Set, Write};

use crate::collection::Collection;

/// The head set and the stored value of the attempt's document, read on first
/// use.
#[derive(Default)]
pub(crate) struct BlockReads {
    heads: Option<(u64, Set)>,
    /// The value read, with the version and short ID it was read for.
    document: Option<(String, u64, Option<Document>)>,
}

impl BlockReads {
    /// The document's head set.
    pub(crate) async fn heads(
        &mut self,
        headstore: &NamespaceView,
        doc_short_id: u64,
    ) -> storage::corekv::Result<&Set> {
        let set = match self.heads.take() {
            Some((short_id, set)) if short_id == doc_short_id => set,
            _ => crate::block::heads::document::load(headstore, doc_short_id).await?,
        };
        Ok(&self.heads.insert((doc_short_id, set)).1)
    }

    /// The highest priority among `field`'s live heads, `0` when it has none:
    /// the heads whose name starts `{field}/`, as the field's head keys do.
    pub(crate) async fn field_priority(
        &mut self,
        headstore: &NamespaceView,
        doc_short_id: u64,
        field: &str,
    ) -> storage::corekv::Result<u64> {
        let set = self.heads(headstore, doc_short_id).await?;
        Ok(set
            .live()
            .filter(|(name, _)| {
                name.strip_prefix(field.as_bytes())
                    .is_some_and(|rest| rest.first() == Some(&b'/'))
            })
            .map(|(_, priority)| priority)
            .max()
            .unwrap_or(0))
    }

    /// The document as `collection` reads it, `None` when it has no value.
    pub(crate) async fn document(
        &mut self,
        collection: &Collection,
        datastore: &NamespaceView,
        doc_short_id: u64,
        doc_id: &DocID,
    ) -> crate::error::Result<Option<Document>> {
        if let Some((version, short_id, document)) = &self.document {
            if version == collection.version_id() && *short_id == doc_short_id {
                return Ok(document.clone());
            }
        }
        let document = collection
            .get_with_datastore(datastore, doc_short_id, doc_id)
            .await?;
        self.document = Some((
            collection.version_id().to_string(),
            doc_short_id,
            document.clone(),
        ));
        Ok(document)
    }

    /// The stored value was written: the next read reads it again.
    pub(crate) fn wrote_document(&mut self) {
        self.document = None;
    }

    /// Keep only this attempt's own head changes; the cache never crosses a commit.
    pub(crate) fn wrote_heads(&mut self, doc_short_id: u64, write: &Write) {
        if let Some((short_id, set)) = &mut self.heads {
            if *short_id == doc_short_id {
                set.apply(write);
            } else {
                self.heads = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::heads::document;
    use crate::database::DB;
    use cid::Cid;
    use storage::corekv::IterOptions;
    use storage::keys::headstore::HeadstoreDocKey;
    use storage::merge::head_set::Write;

    fn cid(seed: u8) -> Cid {
        defra_core::block::generate_cid_from_bytes(&[seed]).unwrap()
    }

    /// The highest priority among the head keys under `field`'s prefix, as
    /// the merge read it before the head set was read once per block.
    async fn iterated_priority(headstore: &NamespaceView, doc_short_id: u64, field: &str) -> u64 {
        let mut iter = document::iterator(
            headstore,
            IterOptions::new().with_prefix(HeadstoreDocKey::field_prefix(doc_short_id, field)),
        )
        .await
        .unwrap();
        let mut max = 0;
        while let Some(pair) = iter.next().await.unwrap() {
            max = max.max(crate::block::builder::decode_priority_varint(&pair.value));
        }
        iter.close().await.unwrap();
        max
    }

    #[test]
    fn a_field_priority_is_its_highest_live_head_as_its_head_keys_give_it() {
        workers::block_on(async {
            let db = DB::new(storage::RegolithStore::in_memory().unwrap()).unwrap();
            let txn = db.new_txn(false).await.unwrap();
            let headstore = txn.headstore().unwrap();
            // A short ID whose encoding is the byte `/`.
            let doc_short_id = 0x2f;
            let mut write = Write::default();
            write.add(&document::name("name", &cid(1)), 3);
            write.add(&document::name("name", &cid(2)), 5);
            write.add(&document::name("nam", &cid(3)), 9);
            write.add(&document::name("name_2", &cid(4)), 11);
            write.add(&document::name("name", &cid(5)), 20);
            write.supersede(&document::name("name", &cid(5)));
            write.supersede_ahead(&document::name("name", &cid(6)));
            write.add(&document::name("C", &cid(7)), 2);
            document::commit(&headstore, doc_short_id, &write)
                .await
                .unwrap();

            let mut reads = BlockReads::default();
            for field in ["name", "nam", "name_2", "C", "absent"] {
                assert_eq!(
                    reads
                        .field_priority(&headstore, doc_short_id, field)
                        .await
                        .unwrap(),
                    iterated_priority(&headstore, doc_short_id, field).await,
                    "{field}"
                );
            }
            assert_eq!(
                reads
                    .field_priority(&headstore, doc_short_id, "name")
                    .await
                    .unwrap(),
                5
            );
        })
        .unwrap()
    }
}
