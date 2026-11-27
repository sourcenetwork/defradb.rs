//! Where a node's graph record and code live: the record's key, its
//! cluster, and the codes a walk ranks edges by.

use std::borrow::Cow;

use bytes::Bytes;
use rapidhash::RapidHashMap;

use super::cache::{Coded, Codes};
use super::codec::{self, Record};
use super::graph::Visit;
use super::params::CLUSTER_CANDIDATES;
use super::DiskAnn;
use crate::index::error::Result;
use crate::index::vector::store::{NodeId, VectorNodeStore};

/// A graph record as stored, with the cluster its key names.
pub(super) struct Located {
    bytes: Bytes,
    /// The node's own code, where codes are resident.
    code: Option<Vec<u8>>,
    pub cluster: Option<u32>,
}

/// How many records each cluster a merge touches holds, counted from the
/// store the first time a cluster is asked about and kept up to date as the
/// merge adds to it.
#[derive(Debug, Default)]
pub(super) struct ClusterSizes(RapidHashMap<u32, usize>);

impl ClusterSizes {
    /// Counts one more record in `tag`'s cluster.
    pub(super) fn add(&mut self, tag: u32) {
        *self.0.entry(tag).or_default() += 1;
    }
}

impl<S> DiskAnn<S>
where
    S: VectorNodeStore,
{
    /// Where `id`'s record is, and its bytes. With resident codes the key
    /// starts with the cluster the node's code names: the code cache's,
    /// which may be another generation's, and else the transaction's own
    /// (`proofs/tla/DiskAnnClusterKeys.tla` `Found`). A node with no code
    /// has no record.
    pub(super) async fn locate(&self, m: usize, id: NodeId) -> Result<Option<Located>> {
        if !self.params.resident_codes {
            let key = codec::record_key(id);
            return Ok(self
                .store()
                .get_aux(codec::GRAPH, &key)
                .await?
                .map(|bytes| Located {
                    bytes,
                    code: None,
                    cluster: None,
                }));
        }
        let Some((code, cluster)) = self.code(m, id).await? else {
            return Ok(None);
        };
        let key = codec::graph_key(id, cluster);
        if let Some(bytes) = self.store().get_aux(codec::GRAPH, &key).await? {
            return Ok(Some(Located {
                bytes,
                code: Some(code),
                cluster,
            }));
        }
        let Some((code, stored)) = self.stored_code(m, id).await? else {
            return Ok(None);
        };
        if stored == cluster {
            return Ok(None);
        }
        let key = codec::graph_key(id, stored);
        Ok(self
            .store()
            .get_aux(codec::GRAPH, &key)
            .await?
            .map(|bytes| Located {
                bytes,
                code: Some(code),
                cluster: stored,
            }))
    }

    /// A node's record and the cluster it is keyed by, with its neighbours'
    /// codes filled in from the cache or their own keys when codes are
    /// resident. A neighbour whose code is gone was consolidated away and is
    /// left out.
    pub(super) async fn record(
        &self,
        m: usize,
        id: NodeId,
    ) -> Result<Option<(Record, Option<u32>)>> {
        let Some(located) = self.locate(m, id).await? else {
            return Ok(None);
        };
        let Some(code) = located.code else {
            return Ok(Some((codec::decode_record(&located.bytes, m)?, None)));
        };
        let mut neighbors = Vec::new();
        for neighbor in codec::decode_ids(&located.bytes)? {
            if let Some((code, _)) = self.code(m, neighbor).await? {
                neighbors.push((neighbor, code));
            }
        }
        Ok(Some((Record { code, neighbors }, located.cluster)))
    }

    /// Calls `visit` with each of a node's edges and its code, borrowed from
    /// the record where the record carries codes. False when the node has no
    /// record.
    pub(super) async fn each_edge<F>(&self, m: usize, id: NodeId, mut visit: F) -> Result<bool>
    where
        F: FnMut(NodeId, Cow<'_, [u8]>),
    {
        if self.params.resident_codes {
            let Some((record, _)) = self.record(m, id).await? else {
                return Ok(false);
            };
            for (id, code) in record.neighbors {
                visit(id, Cow::Owned(code));
            }
            return Ok(true);
        }
        let key = codec::record_key(id);
        let Some(bytes) = self.store().get_aux(codec::GRAPH, &key).await? else {
            return Ok(false);
        };
        let edges = codec::record_edges(&bytes, m)?;
        for (id, code) in edges {
            visit(id, Cow::Borrowed(code));
        }
        Ok(true)
    }

    /// Writes the record's edges under `cluster`. A resident-codes record
    /// holds only ids; the node's own code is written once, by
    /// [`link`](Self::link).
    pub(super) async fn put_record(
        &mut self,
        id: NodeId,
        cluster: Option<u32>,
        record: &Record,
    ) -> Result<()> {
        let bytes = if self.params.resident_codes {
            codec::encode_ids(&record.neighbors)
        } else {
            codec::encode_record(record)
        };
        self.store_mut()
            .put_aux(codec::GRAPH, &codec::graph_key(id, cluster), &bytes)
            .await
    }

    /// A node's code and cluster, from the cache where it has them.
    pub(super) async fn code(&self, m: usize, id: NodeId) -> Result<Option<Coded>> {
        if let Some(coded) = self.codes.as_ref().and_then(|codes| codes.get(id)) {
            return Ok(Some(coded));
        }
        let coded = self.stored_code(m, id).await?;
        if let (Some(codes), Some(coded)) = (&self.codes, &coded) {
            codes.insert(id, coded.clone());
        }
        Ok(coded)
    }

    /// A node's code and cluster as the transaction reads them.
    pub(super) async fn stored_code(&self, m: usize, id: NodeId) -> Result<Option<Coded>> {
        let Some(bytes) = self
            .store()
            .get_aux(codec::CODE, &codec::record_key(id))
            .await?
        else {
            return Ok(None);
        };
        let (code, cluster) = codec::decode_code(&bytes, m)?;
        Ok(Some((code.to_vec(), cluster)))
    }

    /// Reloads the cache when a merge has run since it was loaded.
    pub(super) async fn refresh_codes(&self, m: usize, generation: u64) -> Result<()> {
        let Some(cache) = &self.codes else {
            return Ok(());
        };
        if !self.params.resident_codes || cache.is_current(generation) {
            return Ok(());
        }
        let mut codes = Codes::default();
        self.store()
            .iterate_aux(codec::CODE, b"", |key, value| {
                let (code, cluster) = codec::decode_code(value, m)?;
                codes.insert(codec::id_from_key(key)?, code, cluster)
            })
            .await?;
        cache.replace(generation, codes);
        Ok(())
    }

    /// The cluster a new node's record joins: that of the nearest of the
    /// first [`CLUSTER_CANDIDATES`] nodes its walk found whose cluster has
    /// room, or else a cluster of its own. The choice only places the record,
    /// so a cluster read from another generation costs locality, never
    /// correctness.
    pub(super) async fn choose_cluster(
        &self,
        m: usize,
        id: NodeId,
        visited: &[Visit],
        sizes: &mut ClusterSizes,
    ) -> Result<u32> {
        let capacity = self.params.cluster_capacity();
        for visit in visited
            .iter()
            .filter(|visit| visit.id != id)
            .take(CLUSTER_CANDIDATES)
        {
            let Some((_, Some(tag))) = self.code(m, visit.id).await? else {
                continue;
            };
            if self.cluster_size(sizes, tag).await? < capacity {
                return Ok(tag);
            }
        }
        Ok(codec::cluster_tag(id))
    }

    pub(super) async fn cluster_size(&self, sizes: &mut ClusterSizes, tag: u32) -> Result<usize> {
        if let Some(size) = sizes.0.get(&tag) {
            return Ok(*size);
        }
        let mut size = 0;
        self.store()
            .iterate_aux(codec::GRAPH, &tag.to_be_bytes(), |key, _| {
                size += usize::from(key.len() == 12);
                Ok(())
            })
            .await?;
        sizes.0.insert(tag, size);
        Ok(size)
    }

    /// The cluster the entry of an empty graph starts: its own, where codes
    /// are resident.
    pub(super) async fn found_cluster(
        &mut self,
        sizes: &mut ClusterSizes,
        id: NodeId,
        code: &[u8],
    ) -> Result<Option<u32>> {
        if !self.params.resident_codes {
            return Ok(None);
        }
        let tag = codec::cluster_tag(id);
        sizes.add(tag);
        self.put_code(id, code, Some(tag)).await?;
        Ok(Some(tag))
    }

    pub(super) async fn put_code(
        &mut self,
        id: NodeId,
        code: &[u8],
        cluster: Option<u32>,
    ) -> Result<()> {
        self.store_mut()
            .put_aux(
                codec::CODE,
                &codec::record_key(id),
                &codec::encode_code(code, cluster),
            )
            .await
    }
}
