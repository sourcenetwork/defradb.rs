//! The database's side of the receive gate (#2456): the replication filter
//! and the retention floor, judged on a pushed document before its blocks
//! are fetched or stored, on whatever blocks are at hand. A value the gate
//! cannot read lets the document go on; the merge judges it again as the
//! backstop (`replication_filter`, `retention`).

use std::sync::Arc;

use defra_core::block::CrdtDelta;
use defra_core::Block;
use p2p::sync::receive_gate::{GateBlocks, GateRefusal, ReceiveGate};
use storage::Store;

use crate::replication_filter::{add_field_values, FieldValues};

/// Judges pushed documents against a database's filter and retention rule.
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
    /// A gate for `db`, or `None` when it has neither a filter nor a
    /// retention rule, so there is nothing to judge.
    pub fn new(db: Arc<crate::database::DB<S>>) -> Option<Self> {
        let options = db.options();
        (options.replication_filter.is_some() || options.has_retention_rule())
            .then_some(Self { db })
    }
}

/// What a create's linked blocks say about its height field: a value, no
/// valid value, or nothing the gate can read.
enum Height {
    Value(i64),
    Invalid,
    Unreadable,
}

/// The height a create's height block holds, as Go's `heightIn` reads it:
/// only a plain (not encrypted) LWW value of `field` is read; a
/// non-negative integer up to `i64::MAX` is a height, any other value is no
/// valid height, and bytes that do not decode are unreadable.
fn height_in(field: &str, block: &Block) -> Height {
    let data = match &block.delta {
        CrdtDelta::Lww(delta) if block.encryption.is_none() && delta.field_name == field => {
            &delta.data
        }
        _ => return Height::Unreadable,
    };
    match ciborium::from_reader::<ciborium::Value, _>(data.as_slice()) {
        Ok(ciborium::Value::Integer(value)) => match i64::try_from(i128::from(value)) {
            Ok(height) if height >= 0 => Height::Value(height),
            _ => Height::Invalid,
        },
        Ok(_) => Height::Invalid,
        Err(_) => Height::Unreadable,
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<S> ReceiveGate for DbReceiveGate<S>
where
    S: Store + 'static,
{
    async fn judge(
        &self,
        collection_id: &str,
        doc_id: &str,
        head: &[u8],
        blocks: &dyn GateBlocks,
    ) -> Option<GateRefusal> {
        let collection = self.db.find_collection_by_id(collection_id).ok()??;
        let schema = collection.schema();
        // A branchable collection's head commits many documents at once.
        if schema.is_branchable {
            return None;
        }
        let head = Block::from_dag_cbor(head).ok()?;
        let CrdtDelta::Composite(payload) = &head.delta else {
            return None;
        };
        let mut linked = Vec::new();
        for link in head.links.iter().flatten() {
            if let Some(Ok(block)) = blocks
                .block(&link.link)
                .await
                .map(|data| Block::from_dag_cbor(&data))
            {
                linked.push((link.name.as_str(), block));
            }
        }

        if let Some(filter) = self.db.options().replication_filter.as_ref() {
            let mut fields = FieldValues::default();
            for (_, block) in &linked {
                add_field_values(&mut fields, block);
            }
            if !filter.allow_replication(collection_id, doc_id, &fields) {
                return Some(GateRefusal::Filtered);
            }
        }

        // As Go's gate: only a create (a head of priority 1) is judged, on
        // the block it links under the height field. A create with no such
        // link, or whose block is not at hand, goes on to the merge.
        if payload.priority != 1 {
            return None;
        }
        let floor = self.db.options().retention_floor(schema)?;
        let (_, height_block) = linked.iter().find(|(name, _)| *name == floor.field)?;
        match height_in(&floor.field, height_block) {
            Height::Value(height) if floor.floor > 0 && height <= floor.floor => {
                Some(GateRefusal::BelowFloor)
            }
            Height::Value(_) | Height::Unreadable => None,
            Height::Invalid => Some(GateRefusal::NoHeight),
        }
    }
}
