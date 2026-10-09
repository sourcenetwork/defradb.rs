//! The walk, the insert, and the pruning they share.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::codec::{self, Record, State};
use super::DiskAnn;
use crate::index::error::{Error, Result};
use crate::index::vector::engine::ann::{Admit, Candidate, Neighbor, VectorIndexEngine};
use crate::index::vector::engine::select::{EdgeSelector, RobustPrune};
use crate::index::vector::quantize::{ProductQuantizer, Quantizer};
use crate::index::vector::store::{NodeId, VectorNodeStore};
use defra_core::vector::Element;

/// A node the walk reached, ranked by its code.
#[derive(Debug, Clone)]
pub(super) struct Visit {
    pub id: NodeId,
    pub distance: f64,
    pub code: Vec<u8>,
}

fn by_distance(a: &Visit, b: &Visit) -> std::cmp::Ordering {
    a.distance
        .total_cmp(&b.distance)
        .then_with(|| a.id.cmp(&b.id))
}

impl<S: VectorNodeStore> DiskAnn<S> {
    pub(super) async fn record(&self, m: usize, id: NodeId) -> Result<Option<Record>> {
        match self
            .store()
            .get_aux(codec::GRAPH, &codec::record_key(id))
            .await?
        {
            Some(bytes) => codec::decode_record(&bytes, m).map(Some),
            None => Ok(None),
        }
    }

    pub(super) async fn put_record(&mut self, id: NodeId, record: &Record) -> Result<()> {
        self.store_mut()
            .put_aux(
                codec::GRAPH,
                &codec::record_key(id),
                &codec::encode_record(record),
            )
            .await
    }

    /// GreedySearch (paper Algorithm 1) over codes: every node expanded, with
    /// its own code, nearest first. One record read per expansion.
    pub(super) async fn walk(
        &self,
        pq: &ProductQuantizer,
        entry: NodeId,
        query: &[f32],
        l: usize,
    ) -> Result<Vec<Visit>> {
        let m = pq.code_len();
        let table = pq.distance_table(query);
        let Some(start) = self.record(m, entry).await? else {
            return Ok(Vec::new());
        };
        let mut frontier = vec![Visit {
            id: entry,
            distance: pq.distance(&table, &start.code),
            code: start.code,
        }];
        let mut seen = HashSet::from([entry]);
        let mut expanded = HashSet::new();
        let mut visited = Vec::new();

        while let Some(next) = frontier
            .iter()
            .find(|visit| !expanded.contains(&visit.id))
            .cloned()
        {
            expanded.insert(next.id);
            // Absent when consolidation removed it after an edge was read.
            let Some(record) = self.record(m, next.id).await? else {
                continue;
            };
            visited.push(next);
            for (id, code) in record.neighbors {
                if seen.insert(id) {
                    frontier.push(Visit {
                        id,
                        distance: pq.distance(&table, &code),
                        code,
                    });
                }
            }
            frontier.sort_by(by_distance);
            frontier.truncate(l);
        }
        visited.sort_by(by_distance);
        Ok(visited)
    }

    /// Insert (paper Algorithm 2): walk to the new node's neighbourhood, prune
    /// its out-edges from what the walk expanded, then add the back-edges,
    /// pruning any neighbour pushed over `R`.
    pub(super) async fn graph_insert(
        &mut self,
        state: &mut State,
        id: NodeId,
        vector: &[f32],
    ) -> Result<()> {
        let pq = self.quantizer(state).await?;
        let m = pq.code_len();
        let mut code = vec![0u8; m];
        pq.encode(vector, &mut code);
        let existed = self.record(m, id).await?.is_some();
        if !existed {
            state.live += 1;
        }

        let Some(entry) = state.entry else {
            state.entry = Some(id);
            return self
                .put_record(
                    id,
                    &Record {
                        code,
                        neighbors: Vec::new(),
                    },
                )
                .await;
        };

        let visited = self
            .walk(&pq, entry, vector, self.params.l_build as usize)
            .await?;
        let neighbors = self.prune(
            &pq,
            vector,
            visited
                .into_iter()
                .filter(|visit| visit.id != id)
                .map(|visit| (visit.id, visit.code))
                .collect(),
        );
        self.put_record(
            id,
            &Record {
                code: code.clone(),
                neighbors: neighbors.clone(),
            },
        )
        .await?;

        let r = self.params.r as usize;
        for (neighbor, _) in neighbors {
            let Some(mut record) = self.record(m, neighbor).await? else {
                continue;
            };
            match record.neighbors.iter_mut().find(|(n, _)| *n == id) {
                // A re-insert: the edge stays, under the vector's new code.
                Some(edge) => edge.1 = code.clone(),
                None => record.neighbors.push((id, code.clone())),
            }
            if record.neighbors.len() > r {
                let base = decode(&pq, &record.code);
                record.neighbors = self.prune(&pq, &base, std::mem::take(&mut record.neighbors));
            }
            self.put_record(neighbor, &record).await?;
        }
        Ok(())
    }

    /// RobustPrune over decoded codes, as StreamingMerge does: the full
    /// vectors are on disk and the codes are already in hand.
    pub(super) fn prune(
        &self,
        pq: &ProductQuantizer,
        base: &[f32],
        edges: Vec<(NodeId, Vec<u8>)>,
    ) -> Vec<(NodeId, Vec<u8>)> {
        let mut codes: HashMap<NodeId, Vec<u8>> = HashMap::with_capacity(edges.len());
        let candidates: Vec<Candidate> = edges
            .into_iter()
            .filter_map(|(id, code)| {
                let vector = decode(pq, &code);
                codes.insert(id, code).is_none().then(|| Candidate {
                    id,
                    distance: self.metric.distance_stored(base, &vector),
                    vector: Arc::from(vector),
                })
            })
            .collect();
        RobustPrune::new(self.params.alpha())
            .select(self.metric, base, &candidates, self.params.r as usize)
            .into_iter()
            .filter_map(|kept| codes.remove(&kept.id).map(|code| (kept.id, code)))
            .collect()
    }

    /// Walks by code, then re-ranks the best `L` admitted nodes by their full
    /// vectors.
    ///
    /// Falls back to the exact scan when the walk admits fewer than `k`: a
    /// selective filter, or a graph smaller than `k`. That scan is bounded by
    /// the corpus, which is what the unrouted query would have read anyway.
    pub(super) async fn search_graph<E: Element, A: Admit>(
        &self,
        state: &State,
        query: &[E],
        k: usize,
        effort: Option<usize>,
        admit: &A,
    ) -> Result<Vec<Neighbor>> {
        if k == 0 {
            return Ok(Vec::new());
        }
        if query.len() != state.dimensions as usize {
            return Err(Error::VectorDimensionMismatch {
                indexed: state.dimensions as usize,
                got: query.len(),
            });
        }
        let Some(entry) = state.entry else {
            return Ok(Vec::new());
        };
        let prepared = self.metric.prepare(query);
        let l = effort.unwrap_or(self.params.l_search as usize).max(k);
        let pq = self.quantizer(state).await?;

        let mut ranked = Vec::with_capacity(l);
        for visit in self.walk(&pq, entry, &prepared, l).await? {
            if ranked.len() >= l {
                break;
            }
            if !admit.admits(visit.id) {
                continue;
            }
            let Some(node) = self.store().get_node(visit.id).await? else {
                continue;
            };
            if node.deleted {
                continue;
            }
            ranked.push(Neighbor {
                id: visit.id,
                distance: self.metric.distance_stored(&prepared, &node.vector),
            });
        }
        if ranked.len() < k {
            return self.staging.search_where(query, k, effort, admit).await;
        }
        ranked.sort_by(|a, b| {
            a.distance
                .total_cmp(&b.distance)
                .then_with(|| a.id.cmp(&b.id))
        });
        ranked.truncate(k);
        Ok(ranked)
    }
}

pub(super) fn decode(pq: &ProductQuantizer, code: &[u8]) -> Vec<f32> {
    let mut vector = vec![0.0f32; pq.dimensions()];
    pq.decode(code, &mut vector);
    vector
}
