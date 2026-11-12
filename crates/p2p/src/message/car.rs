//! CAR fetch request types used by transport-specific CAR transfer paths.

use rapidhash::{HashSetExt, RapidHashSet};

use cid::Cid;
use serde::{Deserialize, Serialize};

/// Request for fetching blocks packaged as a CAR response.
///
/// `root_cid` is the DAG root being synchronized for correlation and logging.
/// `wanted_cids` are the CAR roots to include in the response.
/// If `recursive` is true, the responder walks the DAG from `wanted_cids`
/// (falling back to `root_cid` for legacy requests with no want list).
/// If `recursive` is false, the responder returns only the explicitly wanted blocks.
/// `max_depth` bounds a recursive walk to that many links below each wanted
/// CID. It is omitted when `None` so unbounded requests keep their legacy
/// bytes; a responder that predates it serves the whole closure.
/// `have_cids` names merged blocks the receiver holds, so it holds their
/// closures too. A responder holding one of them serves only the part of the
/// wanted closure outside those closures, and ignores `max_depth`. Omitted
/// when empty; a responder that predates it ignores it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CarFetchRequest {
    pub root_cid: Cid,
    #[serde(default)]
    pub wanted_cids: Vec<Cid>,
    #[serde(default = "default_recursive")]
    pub recursive: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub have_cids: Vec<Cid>,
}

fn default_recursive() -> bool {
    true
}

impl CarFetchRequest {
    pub fn full_dag(root_cid: Cid) -> Self {
        Self {
            root_cid,
            wanted_cids: vec![root_cid],
            recursive: true,
            max_depth: None,
            have_cids: Vec::new(),
        }
    }

    pub fn selective_blocks(root_cid: Cid, wanted_cids: Vec<Cid>) -> Self {
        Self {
            root_cid,
            wanted_cids: dedupe_cids(wanted_cids),
            recursive: false,
            max_depth: None,
            have_cids: Vec::new(),
        }
    }

    /// Fetch a bounded descendant closure rooted at the known missing
    /// frontier. This is distinct from a historical walk from `root_cid`:
    /// every traversal root is a CID the receiver already proved missing.
    pub fn selective_dag(root_cid: Cid, wanted_cids: Vec<Cid>) -> Self {
        Self {
            root_cid,
            wanted_cids: dedupe_cids(wanted_cids),
            recursive: true,
            max_depth: None,
            have_cids: Vec::new(),
        }
    }

    /// The same request, naming merged blocks the receiver holds.
    pub fn with_haves(mut self, have_cids: Vec<Cid>) -> Self {
        self.have_cids = dedupe_cids(have_cids);
        self
    }

    pub fn response_roots(&self) -> Vec<Cid> {
        if self.wanted_cids.is_empty() {
            vec![self.root_cid]
        } else {
            self.wanted_cids.clone()
        }
    }
}

fn dedupe_cids(cids: Vec<Cid>) -> Vec<Cid> {
    let mut seen = RapidHashSet::with_capacity(cids.len());
    cids.into_iter().filter(|cid| seen.insert(*cid)).collect()
}

#[cfg(test)]
#[path = "../../tests/unit/car_request_depth.rs"]
mod depth_tests;

#[cfg(test)]
#[path = "../../tests/unit/car_request_haves.rs"]
mod haves_tests;
