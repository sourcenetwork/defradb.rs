//! Adding back-edges to a node's record, editing the encoded record in place
//! in either layout, and decoding it only to prune.

use bytes::Bytes;

use super::codec;
use super::graph::decode;
use super::DiskAnn;
use crate::index::error::Result;
use crate::index::vector::quantize::{ProductQuantizer, Quantizer};
use crate::index::vector::store::{NodeId, VectorNodeStore};

/// Where a resident-codes node's code was found.
enum Presence {
    /// In the shared code cache, left there uncopied.
    Cached,
    /// Under its own key, kept so a prune need not read it again.
    Read(Bytes),
}

impl<S> DiskAnn<S>
where
    S: VectorNodeStore,
{
    /// Adds `edges` to `target`'s record, replacing the code of an edge it
    /// already has, and prunes it back to `R` only once it passes the slack
    /// limit. A target consolidated away meanwhile is skipped.
    pub(super) async fn add_back_edges(
        &mut self,
        pq: &ProductQuantizer,
        target: NodeId,
        edges: Vec<(NodeId, Vec<u8>)>,
    ) -> Result<()> {
        if self.params.resident_codes {
            return self.add_resident_back_edges(pq, target, edges).await;
        }
        let m = pq.code_len();
        // Most back-edges leave the record under its slack, and decoding
        // it to add one edge copies every neighbour's code.
        let key = codec::record_key(target);
        let Some(bytes) = self.store().get_aux(codec::GRAPH, &key).await? else {
            return Ok(());
        };
        let mut bytes = bytes.to_vec();
        if codec::add_edges(&mut bytes, m, target, &edges)? <= self.params.slack_limit() {
            return self.store_mut().put_aux(codec::GRAPH, &key, &bytes).await;
        }
        let mut record = codec::decode_record(&bytes, m)?;
        let base = decode(pq, &record.code);
        record.neighbors = self.prune(pq, &base, std::mem::take(&mut record.neighbors));
        self.put_record(target, &record).await
    }

    /// [`add_back_edges`](Self::add_back_edges) on an ids-only record. A
    /// neighbour whose code is gone was consolidated away and is dropped, as
    /// [`record`](Self::record) drops it, but only a record past the slack
    /// has its neighbours' codes copied, to prune
    /// (`proofs/lean/ResidentBackEdges.lean` `resident_eq_decoded`).
    async fn add_resident_back_edges(
        &mut self,
        pq: &ProductQuantizer,
        target: NodeId,
        edges: Vec<(NodeId, Vec<u8>)>,
    ) -> Result<()> {
        let key = codec::record_key(target);
        let Some(bytes) = self.store().get_aux(codec::GRAPH, &key).await? else {
            return Ok(());
        };
        let Some(own) = self.presence(target).await? else {
            return Ok(());
        };
        let ids = codec::decode_ids(&bytes)?;
        let mut kept = Vec::with_capacity(ids.len());
        for &id in &ids {
            if let Some(presence) = self.presence(id).await? {
                kept.push((id, presence));
            }
        }
        let mut bytes = if kept.len() == ids.len() {
            bytes.to_vec()
        } else {
            let ids: Vec<NodeId> = kept.iter().map(|(id, _)| *id).collect();
            codec::encode_id_list(&ids)
        };
        if codec::add_ids(&mut bytes, target, &edges)? <= self.params.slack_limit() {
            return self.store_mut().put_aux(codec::GRAPH, &key, &bytes).await;
        }

        let Some(code) = self.code_of(target, own).await? else {
            return Ok(());
        };
        let mut neighbors = Vec::with_capacity(kept.len() + edges.len());
        for (id, presence) in kept {
            if let Some(code) = self.code_of(id, presence).await? {
                neighbors.push((id, code));
            }
        }
        for (id, code) in edges {
            if id == target {
                continue;
            }
            match neighbors.iter_mut().find(|(n, _)| *n == id) {
                Some(edge) => edge.1 = code,
                None => neighbors.push((id, code)),
            }
        }
        let base = decode(pq, &code);
        let neighbors = self.prune(pq, &base, neighbors);
        self.put_record(target, &codec::Record { code, neighbors })
            .await
    }

    /// Whether `id` has a code where [`code`](Self::code) would find it,
    /// without copying one the cache holds. A code read from its key fills
    /// the cache, as `code` fills it.
    async fn presence(&self, id: NodeId) -> Result<Option<Presence>> {
        if self.codes.as_ref().is_some_and(|codes| codes.contains(id)) {
            return Ok(Some(Presence::Cached));
        }
        let key = codec::record_key(id);
        let Some(code) = self.store().get_aux(codec::CODE, &key).await? else {
            return Ok(None);
        };
        if let Some(codes) = &self.codes {
            codes.insert(id, code.to_vec());
        }
        Ok(Some(Presence::Read(code)))
    }

    /// The code [`presence`](Self::presence) found.
    async fn code_of(&self, id: NodeId, presence: Presence) -> Result<Option<Vec<u8>>> {
        match presence {
            Presence::Read(code) => Ok(Some(code.to_vec())),
            Presence::Cached => self.code(id).await,
        }
    }
}
