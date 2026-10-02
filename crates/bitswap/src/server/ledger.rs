//! The wantlist the local node holds for one partner.

use cid::Cid;
use libp2p::PeerId;

use super::wantlist::{Entry, Wantlist};
use crate::message::{Priority, WantType};

/// Tracks the wantlist for a given partner.
#[derive(Debug)]
pub struct Ledger {
    partner: PeerId,
    wantlist: Wantlist,
}

impl Ledger {
    /// An empty ledger.
    pub fn new(partner: PeerId) -> Self {
        Ledger {
            partner,
            wantlist: Wantlist::default(),
        }
    }

    /// The remote peer.
    pub fn partner(&self) -> &PeerId {
        &self.partner
    }

    /// The wantlist.
    pub fn wantlist(&self) -> &Wantlist {
        &self.wantlist
    }

    /// Mutable access to the wantlist.
    pub fn wantlist_mut(&mut self) -> &mut Wantlist {
        &mut self.wantlist
    }

    /// Forgets every want.
    pub fn clear_wantlist(&mut self) {
        self.wantlist.clear();
    }

    /// Records a want.
    pub fn wants(&mut self, cid: Cid, priority: Priority, want_type: WantType) {
        self.wantlist.add(cid, priority, want_type);
    }

    /// Drops a want, returning it when present.
    pub fn cancel_want(&mut self, cid: &Cid) -> Option<Entry> {
        self.wantlist.remove(cid)
    }
}
