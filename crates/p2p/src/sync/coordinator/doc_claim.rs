//! One DAG fetch per document at a time.
//!
//! A burst of head hints for one document (vinc releases held publishes
//! together) would start a fetch per root, each naming no have, and every
//! provider would send back the history the others are already fetching. So
//! a second root of a document waits for the running fetch and starts the
//! moment it ends. Every fetch names the roots earlier fetches of its
//! document completed as haves: their whole closures are held here, merged
//! or not, and the merge that would make them haves may not have run yet.
//! Model: proofs/tla/DocFetchClaim.tla.

use std::sync::Arc;

use cid::Cid;
use futures::FutureExt;
use kovan_map::HopscotchMap;
use regolith::sync::Event;

use super::DagFetchLimiter;
use crate::sync::car::MAX_HAVE_CIDS;
use crate::sync::manager::SyncDiagnostics;

/// The most documents with a fetch running at once, and the most whose
/// completed roots are remembered. A fetch past the first runs without the
/// guard, and is counted.
pub(crate) const MAX_DOC_CLAIMS: usize = 4096;

/// A collection ID and a document ID.
type DocKey = (String, String);

/// Documents a fetch is running for; each fetch's end sets its event.
pub(crate) type DocClaims = HopscotchMap<DocKey, Arc<Event>, rapidhash::fast::RandomState>;

/// The roots fetches of each document completed, newest first.
pub(crate) type DocCompleted = HopscotchMap<DocKey, Vec<Cid>, rapidhash::fast::RandomState>;

/// Held while one fetch of a document runs. Dropping it records the roots
/// the fetch completed, frees the document and wakes the fetches waiting.
pub(crate) struct DocClaim {
    held: Option<(DagFetchLimiter, DocKey, Arc<Event>, Arc<SyncDiagnostics>)>,
    handed: Vec<Cid>,
    completed: Vec<Cid>,
}

impl DocClaim {
    /// Roots earlier fetches of the document completed.
    pub(crate) fn handed(&self) -> &[Cid] {
        &self.handed
    }

    /// Records a root this fetch completed.
    pub(crate) fn completed(&mut self, root: Cid) {
        self.completed.push(root);
    }
}

impl Drop for DocClaim {
    fn drop(&mut self) {
        let Some((limiter, key, ended, diagnostics)) = self.held.take() else {
            return;
        };
        if !self.completed.is_empty() {
            let mut roots = std::mem::take(&mut self.completed);
            for root in limiter.doc_completed.get(&key).unwrap_or_default() {
                if !roots.contains(&root) {
                    roots.push(root);
                }
            }
            roots.truncate(MAX_HAVE_CIDS);
            // ponytail: a full memory is cleared whole; an LRU if a busy node
            // shows fetches losing their haves in car_received_already_held.
            if limiter.doc_completed.len() >= MAX_DOC_CLAIMS
                && !limiter.doc_completed.contains_key(&key)
            {
                limiter.doc_completed.clear();
                diagnostics.record_dag_fetch_doc_completed_cleared();
            }
            limiter.doc_completed.insert(key.clone(), roots);
        }
        limiter.docs.remove(&key);
        ended.set();
    }
}

impl DagFetchLimiter {
    /// Whether a fetch of the document is running now.
    pub(crate) fn doc_busy(&self, collection_id: &str, doc_id: &str) -> bool {
        self.docs
            .contains_key(&(collection_id.to_string(), doc_id.to_string()))
    }

    /// Claims a document for one fetch, waiting while another holds it.
    /// `None` once the limiter closes.
    pub(crate) async fn claim_doc(
        &self,
        collection_id: &str,
        doc_id: &str,
        diagnostics: &Arc<SyncDiagnostics>,
    ) -> Option<DocClaim> {
        let key = (collection_id.to_string(), doc_id.to_string());
        loop {
            if self.closed.is_set() {
                return None;
            }
            if self.docs.len() >= MAX_DOC_CLAIMS && !self.docs.contains_key(&key) {
                diagnostics.record_dag_fetch_doc_claims_full();
                return Some(DocClaim {
                    held: None,
                    handed: self.doc_completed.get(&key).unwrap_or_default(),
                    completed: Vec::new(),
                });
            }
            let ended = Arc::new(Event::new());
            let Some(running) = self.docs.insert_if_absent(key.clone(), Arc::clone(&ended)) else {
                // Read once the claim is held: the fetch before this one
                // recorded its roots before it freed the document.
                let handed = self.doc_completed.get(&key).unwrap_or_default();
                return Some(DocClaim {
                    held: Some((self.clone(), key, ended, Arc::clone(diagnostics))),
                    handed,
                    completed: Vec::new(),
                });
            };
            diagnostics.record_dag_fetch_doc_wait();
            futures::select_biased! {
                () = self.closed.wait().fuse() => return None,
                () = running.wait().fuse() => {}
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/doc_claim.rs"]
mod tests;
