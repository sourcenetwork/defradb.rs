//! The database's side of the receive gate: the replication filter, asked
//! about a pushed document before its blocks are fetched or stored, on
//! whatever blocks are at hand. A field the gate cannot read is left out of
//! what the filter sees; the merge asks again as the backstop
//! (`replication_filter`).

use std::sync::Arc;

use defra_core::block::CrdtDelta;
use defra_core::Block;
use p2p::sync::receive_gate::{GateBlocks, ReceiveGate};
use storage::Store;

use crate::replication_filter::{add_field_values, FieldValues};

/// Asks a database's replication filter about pushed documents.
pub struct DbReceiveGate<S>
where
    S: Store,
{
    db: Arc<crate::database::DB<S>>,
}

impl<S> DbReceiveGate<S>
where
    S: Store,
{
    /// A gate for `db`, or `None` when it has no replication filter, so
    /// there is nothing to ask and the receive path stays as it was.
    pub fn new(db: Arc<crate::database::DB<S>>) -> Option<Self> {
        db.options()
            .replication_filter
            .is_some()
            .then_some(Self { db })
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<S> ReceiveGate for DbReceiveGate<S>
where
    S: Store + 'static,
{
    async fn allows(
        &self,
        collection_id: &str,
        doc_id: &str,
        head: &[u8],
        blocks: &dyn GateBlocks,
    ) -> bool {
        let Some(filter) = self.db.options().replication_filter.as_ref() else {
            return true;
        };
        let Ok(Some(collection)) = self.db.find_collection_by_id(collection_id) else {
            return true;
        };
        // A branchable collection's head commits many documents at once.
        if collection.schema().is_branchable {
            return true;
        }
        let Ok(head) = Block::from_dag_cbor(head) else {
            return true;
        };
        if !matches!(head.delta, CrdtDelta::Composite(_)) {
            return true;
        }
        let mut fields = FieldValues::default();
        for link in head.links.iter().flatten() {
            if let Some(Ok(block)) = blocks
                .block(&link.link)
                .await
                .map(|data| Block::from_dag_cbor(&data))
            {
                add_field_values(&mut fields, &block);
            }
        }
        filter.allow_replication(collection_id, doc_id, &fields)
    }
}
