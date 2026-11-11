//! The records this node has seen, as vinc's rendezvous.
//!
//! The facade has no relay client yet, so there is no network rendezvous to
//! publish to. Instead every connection carries each side's signed record on
//! a record stream, and this table keeps them: a peer seen once can be dialed
//! by id alone later, and its record's hints carry its service-record
//! pointer. The endpoint verifies a record before it uses it.

use std::cell::RefCell;
use std::future::Future;
use std::rc::Rc;

use rapidhash::RapidHashMap;
use vinc::NodeId;

/// The most records kept, vinc's own bound on known records.
const MAX_RECORDS: usize = vinc::KNOWN_MAX;

#[derive(Clone, Default)]
pub struct PeerDirectory {
    records: Rc<RefCell<RapidHashMap<NodeId, Vec<u8>>>>,
}

impl PeerDirectory {
    /// Keep `record` as `node`'s, replacing an older one. The endpoint checks
    /// that it is `node`'s and verifies its signature when it reads it.
    pub fn insert(&self, node: NodeId, record: Vec<u8>) {
        let Ok(mut records) = self.records.try_borrow_mut() else {
            return;
        };
        if records.len() >= MAX_RECORDS && !records.contains_key(&node) {
            let evict = records.keys().next().copied();
            if let Some(evict) = evict {
                records.remove(&evict);
            }
        }
        records.insert(node, record);
    }
}

impl vinc::Rendezvous for PeerDirectory {
    // This node's own record goes to its peers on their connections, not here.
    fn publish(&self, _record: &[u8]) -> impl Future<Output = vinc::Result<()>> {
        std::future::ready(Ok(()))
    }

    fn fetch(&self, node: &NodeId) -> impl Future<Output = Option<Vec<u8>>> {
        std::future::ready(
            self.records
                .try_borrow()
                .ok()
                .and_then(|records| records.get(node).cloned()),
        )
    }
}
