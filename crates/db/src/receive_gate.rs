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
use crate::retention::RetentionFloor;

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

/// The height `blocks` hold for `floor.field`, read from a plain (not
/// encrypted) LWW value of that field, alone or in an immutable block.
fn height_in(floor: &RetentionFloor, blocks: &[Block], all_at_hand: bool) -> Height {
    for block in blocks {
        if block.encryption.is_some() {
            continue;
        }
        let data = match &block.delta {
            CrdtDelta::Lww(delta) if delta.field_name == floor.field => Some(delta.data.clone()),
            CrdtDelta::Immutable(payload) => payload
                .lww_deltas()
                .find(|delta| delta.field_name == floor.field)
                .map(|delta| delta.data),
            _ => None,
        };
        let Some(data) = data else {
            continue;
        };
        return match ciborium::from_reader::<ciborium::Value, _>(data.as_slice()) {
            Ok(ciborium::Value::Integer(value)) => match i64::try_from(i128::from(value)) {
                Ok(height) if height >= 0 => Height::Value(height),
                _ => Height::Invalid,
            },
            Ok(_) => Height::Invalid,
            Err(_) => Height::Unreadable,
        };
    }
    // A create links every field it sets: with every linked block at hand
    // and none holding the field, the field is unset.
    if all_at_hand {
        Height::Invalid
    } else {
        Height::Unreadable
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
        let mut all_at_hand = true;
        for link in head.links.iter().flatten() {
            match blocks.block(&link.link).await {
                Some(data) => match Block::from_dag_cbor(&data) {
                    Ok(block) => linked.push(block),
                    Err(_) => all_at_hand = false,
                },
                None => all_at_hand = false,
            }
        }

        if let Some(filter) = self.db.options().replication_filter.as_ref() {
            let mut fields = FieldValues::default();
            for block in &linked {
                add_field_values(&mut fields, block);
            }
            if !filter.allow_replication(collection_id, doc_id, &fields) {
                return Some(GateRefusal::Filtered);
            }
        }

        // Only a create (a head of priority 1) is judged by height.
        if payload.priority != 1 {
            return None;
        }
        let floor = self.db.options().retention_floor(schema)?;
        match height_in(&floor, &linked, all_at_hand) {
            Height::Value(height) if floor.floor > 0 && height <= floor.floor => {
                Some(GateRefusal::BelowFloor)
            }
            Height::Value(_) | Height::Unreadable => None,
            Height::Invalid => Some(GateRefusal::NoHeight),
        }
    }
}
