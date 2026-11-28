//! One search's reload of a resident-codes index's codes.
//!
//! Only one search reloads a generation, and it drops the old codes before it
//! starts, so the new ones get the room the old ones held
//! (`proofs/tla/VectorMemoryBudget.tla`). Until it finishes, searches find no
//! codes and read each from storage, and keep none of them. A reload of a
//! newer generation takes the claim over, and the older reload lets go at its
//! next step. A reload that never finishes (an error, a cancelled search)
//! hands its claim back on drop, so the next search reloads.

use std::ops::ControlFlow;
use std::sync::Arc;

use super::cache::{Claim, CodeCache, Codes, Loaded};
use crate::index::error::Result;
use crate::index::vector::budget::Charge;
use crate::index::vector::store::NodeId;

/// Bytes a reload takes from the budget at a time.
const LOAD_STEP: u64 = 1 << 20;

/// The reload of one generation's codes one search owns.
#[derive(Debug)]
pub struct Reload<'a> {
    cache: &'a CodeCache,
    claim: Claim,
    codes: Codes,
    charge: Charge,
    /// The budget refused a code, or the claim was taken over: every later
    /// code is left out.
    full: bool,
}

impl<'a> Reload<'a> {
    pub(super) fn begin(cache: &'a CodeCache, generation: u64) -> Option<Self> {
        let update = cache.pending.begin_update()?;
        let claim = Claim {
            generation,
            ticket: cache.next_ticket(),
        };
        let mut dropped = None;
        cache.loaded.rcu(|current| {
            dropped = None;
            let current_enough = current.generation.is_some_and(|at| at >= generation);
            let taken = current
                .loading
                .is_some_and(|running| running.generation >= generation);
            if current_enough || taken {
                return current.clone();
            }
            dropped = Some(current.clone());
            Arc::new(Loaded::empty(
                cache.budget(),
                current.generation,
                Some(claim),
            ))
        });
        let dropped = dropped?;
        dropped.release();
        cache.pending.clear_current();
        drop(update);
        kovan::flush();
        Some(Self {
            cache,
            claim,
            codes: Codes::default(),
            charge: Charge::empty(cache.budget()),
            full: false,
        })
    }

    /// Adds a code, unless the budget has no room for it. Each time the
    /// reload takes more room it first checks it still holds its claim: one
    /// taken over stops and gives its room back.
    pub fn insert(&mut self, id: NodeId, code: &[u8]) -> Result<ControlFlow<()>> {
        if self.full {
            return Ok(ControlFlow::Break(()));
        }
        let bound = self.codes.bound_with_one_more(code.len());
        if self.charge.bytes() < bound && !self.owned() {
            self.stop();
            return Ok(ControlFlow::Break(()));
        }
        if !self.charge.cover(bound, LOAD_STEP) {
            self.full = true;
            return Ok(ControlFlow::Break(()));
        }
        self.codes.insert(id, code)?;
        Ok(ControlFlow::Continue(()))
    }

    /// Whether this reload still holds its claim.
    fn owned(&self) -> bool {
        self.cache.loaded.load().loading == Some(self.claim)
    }

    /// Leaves every later code out and gives back what the loaded ones hold.
    fn stop(&mut self) {
        self.full = true;
        self.codes = Codes::default();
        self.charge.release();
    }

    /// Publishes the codes loaded, unless another reload took the claim over
    /// meanwhile.
    pub fn finish(mut self) {
        let codes = std::mem::take(&mut self.codes);
        let charge = std::mem::replace(&mut self.charge, Charge::empty(self.cache.budget()));
        self.cache
            .replace_charged(self.claim.generation, codes, charge, Some(self.claim));
    }
}

impl Drop for Reload<'_> {
    /// Hands the claim back if this reload still holds it, so the next search
    /// reloads. After [`finish`](Self::finish) nothing holds it.
    fn drop(&mut self) {
        let claim = self.claim;
        let budget = self.cache.budget().clone();
        self.cache.loaded.rcu(|current| {
            if current.loading == Some(claim) {
                Arc::new(Loaded::empty(&budget, current.generation, None))
            } else {
                current.clone()
            }
        });
    }
}
