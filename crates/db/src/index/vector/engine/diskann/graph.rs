//! The walk, linking a node in, and the pruning they share.

use std::sync::Arc;

use rapidhash::{HashMapExt, HashSetExt, RapidHashMap, RapidHashSet};

use super::cache::Codes;
use super::codec::{self, GraphState, Record, Trained};
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
    /// A node's record, with its neighbours' codes filled in from the cache
    /// or their own keys when codes are resident. A neighbour whose code is
    /// gone was consolidated away and is left out.
    pub(super) async fn record(&self, m: usize, id: NodeId) -> Result<Option<Record>> {
        let key = codec::record_key(id);
        let Some(bytes) = self.store().get_aux(codec::GRAPH, &key).await? else {
            return Ok(None);
        };
        if !self.params.resident_codes {
            return codec::decode_record(&bytes, m).map(Some);
        }
        let Some(code) = self.code(id).await? else {
            return Ok(None);
        };
        let mut neighbors = Vec::new();
        for neighbor in codec::decode_ids(&bytes)? {
            if let Some(code) = self.code(neighbor).await? {
                neighbors.push((neighbor, code));
            }
        }
        Ok(Some(Record { code, neighbors }))
    }

    /// Writes the record's edges. A resident-codes record holds only ids; the
    /// node's own code is written once, by [`link`](Self::link).
    pub(super) async fn put_record(&mut self, id: NodeId, record: &Record) -> Result<()> {
        let bytes = if self.params.resident_codes {
            codec::encode_ids(&record.neighbors)
        } else {
            codec::encode_record(record)
        };
        self.store_mut()
            .put_aux(codec::GRAPH, &codec::record_key(id), &bytes)
            .await
    }

    async fn code(&self, id: NodeId) -> Result<Option<Vec<u8>>> {
        if let Some(code) = self.codes.as_ref().and_then(|codes| codes.get(id)) {
            return Ok(Some(code));
        }
        let code = self
            .store()
            .get_aux(codec::CODE, &codec::record_key(id))
            .await?
            .map(|code| code.to_vec());
        if let (Some(codes), Some(code)) = (&self.codes, &code) {
            codes.insert(id, code.clone());
        }
        Ok(code)
    }

    /// Reloads the cache when a merge has run since it was loaded.
    async fn refresh_codes(&self, generation: u64) -> Result<()> {
        let Some(cache) = &self.codes else {
            return Ok(());
        };
        if !self.params.resident_codes || cache.is_current(generation) {
            return Ok(());
        }
        let mut codes = Codes::default();
        self.store()
            .iterate_aux(codec::CODE, b"", |key, code| {
                codes.insert(codec::id_from_key(key)?, code);
                Ok(())
            })
            .await?;
        cache.replace(generation, codes);
        Ok(())
    }

    /// GreedySearch (paper Algorithm 1) over codes: every node expanded, with
    /// its own code, nearest first. One record read per expansion.
    ///
    /// Stops after [`walk_budget`](super::DiskAnnParams::walk_budget)
    /// expansions, so what a walk holds is bounded by `l` and `R`, never by
    /// the corpus: the frontier keeps `l`, and `seen`, `expanded` and the
    /// visited list grow only per expansion.
    pub(super) async fn walk(
        &self,
        pq: &ProductQuantizer,
        entry: NodeId,
        table: &[f32],
        l: usize,
    ) -> Result<Vec<Visit>> {
        let m = pq.code_len();
        let Some(start) = self.record(m, entry).await? else {
            return Ok(Vec::new());
        };
        let mut frontier = vec![Visit {
            id: entry,
            distance: pq.distance(table, &start.code),
            code: start.code,
        }];
        let mut seen = RapidHashSet::from_iter([entry]);
        let mut expanded = RapidHashSet::new();
        let mut visited = Vec::new();
        let budget = self.params.walk_budget(l);

        while expanded.len() < budget {
            let Some(next) = frontier
                .iter()
                .find(|visit| !expanded.contains(&visit.id))
                .cloned()
            else {
                break;
            };
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
                        distance: pq.distance(table, &code),
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

    /// Walks to `vector`'s neighbourhood and writes `id`'s record with the
    /// out-edges RobustPrune keeps from what the walk expanded. Returns those
    /// out-edges, each of which owes `id` a back-edge.
    ///
    /// The first node of an empty graph becomes its entry, with no edges.
    pub(super) async fn link(
        &mut self,
        pq: &ProductQuantizer,
        graph: &mut GraphState,
        id: NodeId,
        vector: &[f32],
        code: &[u8],
    ) -> Result<Vec<(NodeId, Vec<u8>)>> {
        if self.record(pq.code_len(), id).await?.is_none() {
            graph.live += 1;
        }
        if self.params.resident_codes {
            self.store_mut()
                .put_aux(codec::CODE, &codec::record_key(id), code)
                .await?;
        }
        let Some(entry) = graph.entry else {
            graph.entry = Some(id);
            self.put_record(
                id,
                &Record {
                    code: code.to_vec(),
                    neighbors: Vec::new(),
                },
            )
            .await?;
            return Ok(Vec::new());
        };
        let table = pq.distance_table(vector);
        let visited = self
            .walk(pq, entry, &table, self.params.l_build as usize)
            .await?;
        let neighbors = self.prune(
            pq,
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
                code: code.to_vec(),
                neighbors: neighbors.clone(),
            },
        )
        .await?;
        Ok(neighbors)
    }

    /// Adds `edges` to `target`'s record, replacing the code of an edge it
    /// already has, and prunes it back to `R` only once it passes the slack
    /// limit. A target consolidated away meanwhile is skipped.
    pub(super) async fn add_back_edges(
        &mut self,
        pq: &ProductQuantizer,
        target: NodeId,
        edges: Vec<(NodeId, Vec<u8>)>,
    ) -> Result<()> {
        let Some(mut record) = self.record(pq.code_len(), target).await? else {
            return Ok(());
        };
        for (id, code) in edges {
            if id == target {
                continue;
            }
            match record.neighbors.iter_mut().find(|(n, _)| *n == id) {
                Some(edge) => edge.1 = code,
                None => record.neighbors.push((id, code)),
            }
        }
        if record.neighbors.len() > self.params.slack_limit() {
            let base = decode(pq, &record.code);
            record.neighbors = self.prune(pq, &base, std::mem::take(&mut record.neighbors));
        }
        self.put_record(target, &record).await
    }

    /// Insert (paper Algorithm 2), back-edges applied at once: how training
    /// builds the graph one node at a time.
    pub(super) async fn graph_insert(
        &mut self,
        trained: &Trained,
        graph: &mut GraphState,
        id: NodeId,
        vector: &[f32],
    ) -> Result<()> {
        let pq = self.quantizer(trained).await?;
        let mut code = vec![0u8; pq.code_len()];
        pq.encode(vector, &mut code);
        for (neighbor, _) in self.link(&pq, graph, id, vector, &code).await? {
            self.add_back_edges(&pq, neighbor, vec![(id, code.clone())])
                .await?;
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
        let mut codes: RapidHashMap<NodeId, Vec<u8>> = RapidHashMap::with_capacity(edges.len());
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

    /// Walks the graph and scans the pending codes, then re-ranks the best `L`
    /// admitted candidates of both by their full vectors.
    ///
    /// A node both in the graph and pending (an update not merged yet) is
    /// ranked once, by its nearer code; re-ranking reads its current vector.
    ///
    /// Falls back to the exact scan when fewer than `k` survive: a selective
    /// filter, or a corpus smaller than `k`. That scan is bounded by the
    /// corpus, which is what the unrouted query would have read anyway.
    pub(super) async fn search_graph<E: Element, A: Admit>(
        &self,
        trained: &Trained,
        query: &[E],
        k: usize,
        effort: Option<usize>,
        admit: &A,
    ) -> Result<Vec<Neighbor>> {
        if k == 0 {
            return Ok(Vec::new());
        }
        if query.len() != trained.dimensions as usize {
            return Err(Error::VectorDimensionMismatch {
                indexed: trained.dimensions as usize,
                got: query.len(),
            });
        }
        let prepared = self.metric.prepare(query);
        let l = effort.unwrap_or(self.params.l_search as usize).max(k);
        let pq = self.quantizer(trained).await?;
        let table = pq.distance_table(&prepared);

        let graph = self.graph_state().await?;
        self.refresh_codes(graph.generation).await?;
        let mut candidates = match graph.entry {
            Some(entry) => self.walk(&pq, entry, &table, l).await?,
            None => Vec::new(),
        };
        self.store()
            .iterate_aux(codec::PENDING, b"", |key, code| {
                candidates.push(Visit {
                    id: codec::id_from_key(key)?,
                    distance: pq.distance(&table, code),
                    code: Vec::new(),
                });
                Ok(())
            })
            .await?;
        candidates.sort_by(by_distance);

        let mut seen = RapidHashSet::new();
        let mut ranked = Vec::with_capacity(l);
        for candidate in candidates {
            if ranked.len() >= l {
                break;
            }
            if !seen.insert(candidate.id) || !admit.admits(candidate.id) {
                continue;
            }
            let Some(node) = self.store().get_node(candidate.id).await? else {
                continue;
            };
            if node.deleted {
                continue;
            }
            ranked.push(Neighbor {
                id: candidate.id,
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
