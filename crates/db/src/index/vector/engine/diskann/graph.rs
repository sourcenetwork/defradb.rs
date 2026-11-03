//! The walk, linking a node in, and the pruning they share.

use std::borrow::Cow;
use std::ops::ControlFlow;
use std::sync::Arc;

use rapidhash::{HashMapExt, HashSetExt, RapidHashMap, RapidHashSet};

use super::cache::Codes;
use super::codec::{self, GraphState, Record, Trained};
use super::pending_cache::{Copy, Segment, Tag, MAX_PENDING_CACHE_BYTES};
use super::DiskAnn;
use crate::index::error::{Error, Result};
use crate::index::vector::engine::ann::{Admit, Candidate, Neighbor, VectorIndexEngine};
use crate::index::vector::engine::select::{EdgeSelector, RobustPrune};
use crate::index::vector::quantize::{ProductQuantizer, Quantizer};
use crate::index::vector::store::{NodeId, VectorNodeStore};
use defra_core::thread_bounds::MaybeSend;
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

    /// Calls `visit` with each of a node's edges and its code, borrowed from
    /// the record where the record carries codes. False when the node has no
    /// record.
    async fn each_edge<F>(&self, m: usize, id: NodeId, mut visit: F) -> Result<bool>
    where
        F: FnMut(NodeId, Cow<'_, [u8]>),
    {
        if self.params.resident_codes {
            let Some(record) = self.record(m, id).await? else {
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
                codes.insert(codec::id_from_key(key)?, code)
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
            // A neighbour ranked after the frontier's last place is cut by the
            // truncate below, so its code is never copied.
            let last = (l > 0 && frontier.len() >= l)
                .then(|| (frontier[l - 1].distance, frontier[l - 1].id));
            let mut added = Vec::new();
            let found = self
                .each_edge(m, next.id, |id, code| {
                    if !seen.insert(id) {
                        return;
                    }
                    let distance = pq.distance(table, &code);
                    let ranked = last.is_none_or(|(d, last)| {
                        distance.total_cmp(&d).then_with(|| id.cmp(&last)).is_lt()
                    });
                    if ranked {
                        added.push(Visit {
                            id,
                            distance,
                            code: code.into_owned(),
                        });
                    }
                })
                .await?;
            // Absent when consolidation removed it after an edge was read.
            if !found {
                continue;
            }
            visited.push(next);
            frontier.append(&mut added);
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
        let m = pq.code_len();
        let mut record = if self.params.resident_codes {
            let Some(mut record) = self.record(m, target).await? else {
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
            record
        } else {
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
            codec::decode_record(&bytes, m)?
        };
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

    /// Every vector the snapshot and the transaction itself have pending,
    /// ranked by the latest code it holds: an update appends a new code, and
    /// the older ones must not take its place (`proofs/tla/PendingCache.tla`
    /// `INV_ScoresLatest`). The transaction's own appends are newer than any
    /// logged entry, so a vector it appended is ranked by its last append.
    async fn pending_candidates(
        &self,
        pq: &ProductQuantizer,
        table: &[f32],
        generation: u64,
        out: &mut Vec<Visit>,
    ) -> Result<()> {
        let m = pq.code_len();
        let start = out.len();
        let mut scanned = RapidHashMap::new();
        let mut latest = |id: NodeId, code: &[u8]| {
            out.push(Visit {
                id,
                distance: pq.distance(table, code),
                code: Vec::new(),
            })
        };
        let mut every = |id: NodeId, code: &[u8]| {
            scanned.insert(id, pq.distance(table, code));
        };
        self.snapshot_pending(m, generation, &mut latest, &mut every)
            .await?;
        let mut own = RapidHashMap::new();
        for entry in self.store().own_pending_log().await? {
            let (id, code) = codec::decode_log_entry(&entry, m)?;
            own.insert(id, pq.distance(table, code));
        }
        if !own.is_empty() {
            let mut index = 0;
            out.retain(|visit| {
                index += 1;
                index <= start || !own.contains_key(&visit.id)
            });
            scanned.retain(|id, _| !own.contains_key(id));
        }
        out.extend(scanned.into_iter().chain(own).map(|(id, distance)| Visit {
            id,
            distance,
            code: Vec::new(),
        }));
        Ok(())
    }

    /// Visits what the snapshot's pending log holds. From the shared copy,
    /// where it provably covers the snapshot, `latest` gets each vector's
    /// latest entry at or before the snapshot's head, once. From a scan of
    /// the log, where the copy may not cover it, `every` gets every entry,
    /// oldest first. A scan the copy could not stand in for reloads it, and
    /// so does a gap too big to fit under the cap: one ordered scan instead
    /// of a point read per missing entry.
    async fn snapshot_pending<L, E>(
        &self,
        m: usize,
        generation: u64,
        latest: &mut L,
        every: &mut E,
    ) -> Result<()>
    where
        L: FnMut(NodeId, &[u8]),
        E: FnMut(NodeId, &[u8]) + MaybeSend,
    {
        let Some(codes) = &self.codes else {
            return self.scan_pending(m, None, every).await;
        };
        let cache = &codes.pending;
        let head = self.store().pending_log_head().await?;
        let copy = cache.load();
        if copy.is_oversized() && copy.tag().is_some_and(|tag| tag.generation >= generation) {
            return self.scan_pending(m, None, every).await;
        }
        let reload = match copy.tag() {
            Some(tag) if tag.generation == generation => {
                if tag.position >= head {
                    copy.for_each_latest_through(head, m, latest);
                    return Ok(());
                }
                if copy.can_hold(head - tag.position, m) {
                    if let Some(added) = self.read_pending_log(m, tag.position, head).await? {
                        let next = Arc::new(copy.caught_up(head, added));
                        next.for_each_latest_through(head, m, latest);
                        cache.publish(next);
                        return Ok(());
                    }
                }
                true
            }
            Some(tag) => tag.generation < generation,
            None => true,
        };
        if !reload {
            return self.scan_pending(m, None, every).await;
        }
        let mut segment = Segment::default();
        self.scan_pending(m, Some(&mut segment), every).await?;
        let tag = Tag {
            generation,
            position: head,
        };
        cache.publish(Arc::new(Copy::loaded(tag, segment)));
        Ok(())
    }

    /// Visits every vector the snapshot's pending log inserts, copying each
    /// into `into` when given one, and stops copying once `into` is past the
    /// cap.
    async fn scan_pending<F>(
        &self,
        m: usize,
        mut into: Option<&mut Segment>,
        visit: &mut F,
    ) -> Result<()>
    where
        F: FnMut(NodeId, &[u8]) + MaybeSend,
    {
        self.store()
            .iterate_pending_log(|position, entry| {
                let (id, code) = codec::decode_log_entry(entry, m)?;
                if let Some(segment) = into.as_deref_mut() {
                    segment.push(position, id, code);
                    if segment.bytes() > MAX_PENDING_CACHE_BYTES {
                        into = None;
                    }
                }
                visit(id, code);
                Ok(ControlFlow::Continue(()))
            })
            .await
    }

    /// The inserts logged after `from` through `through`, or `None` when an
    /// entry is gone: a truncate the copy did not see, which only a scan can
    /// stand in for.
    async fn read_pending_log(&self, m: usize, from: u64, through: u64) -> Result<Option<Segment>> {
        let mut added = Segment::default();
        for position in from + 1..=through {
            let Some(bytes) = self.store().pending_log_entry(position).await? else {
                return Ok(None);
            };
            let (id, code) = codec::decode_log_entry(&bytes, m)?;
            added.push(position, id, code);
        }
        Ok(Some(added))
    }

    /// Walks the graph and scans the pending codes, then re-ranks the best `L`
    /// admitted candidates of both by their full vectors.
    ///
    /// A pending node is ranked by its latest pending code. One both in the
    /// graph and pending (an update not merged yet) is ranked once, by the
    /// nearer of that and its graph code; re-ranking reads its current vector.
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
        self.pending_candidates(&pq, &table, graph.generation, &mut candidates)
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
