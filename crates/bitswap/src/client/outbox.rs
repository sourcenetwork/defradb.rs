//! Per-peer coalescing of outgoing wants and cancels into one message per peer, one send in flight per
//! peer so messages to a peer stay ordered.

use std::mem;

use cid::Cid;
use libp2p::PeerId;
use rapidhash::{RapidHashMap, RapidHashSet};

use super::query::Kind;
use crate::message::{BitswapMessage, Priority, WantType};
use crate::protocol::ProtocolId;

const PRIORITY: Priority = 1;
/// Keeps one message far below the frame size limit; the rest goes out once the send completes.
const MAX_ENTRIES_PER_MESSAGE: usize = 4096;

/// A message ready to send and the requests it carries, as `(cid, request sequence)`.
#[derive(Debug)]
pub struct Outgoing {
    /// Destination.
    pub peer: PeerId,
    /// The coalesced message.
    pub message: BitswapMessage,
    /// The requests the message carries.
    pub requests: Vec<(Cid, u64)>,
}

#[derive(Debug, Clone, Copy)]
enum Intent {
    Want(Kind, u64),
    Cancel,
}

#[derive(Debug, Default)]
pub(crate) struct Outbox {
    queued: RapidHashMap<PeerId, RapidHashMap<Cid, Intent>>,
    busy: RapidHashSet<PeerId>,
}

impl Outbox {
    pub(crate) fn want(&mut self, peer: PeerId, cid: Cid, kind: Kind, seq: u64) {
        self.queued
            .entry(peer)
            .or_default()
            .insert(cid, Intent::Want(kind, seq));
    }

    pub(crate) fn cancel(&mut self, peer: PeerId, cid: Cid) {
        self.queued
            .entry(peer)
            .or_default()
            .insert(cid, Intent::Cancel);
    }

    pub(crate) fn forget(&mut self, peer: &PeerId) {
        self.queued.remove(peer);
    }

    pub(crate) fn done(&mut self, peer: &PeerId) {
        self.busy.remove(peer);
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.queued.is_empty() && self.busy.is_empty()
    }

    /// One message per idle peer with queued entries. A have want goes out as a block want to a peer known
    /// not to speak have.
    pub(crate) fn take_ready(
        &mut self,
        protocol: impl Fn(&PeerId) -> Option<ProtocolId>,
    ) -> Vec<Outgoing> {
        let mut ready = Vec::new();
        for (peer, mut entries) in mem::take(&mut self.queued) {
            if self.busy.contains(&peer) {
                self.queued.insert(peer, entries);
                continue;
            }
            let batch: Vec<(Cid, Intent)> = if entries.len() <= MAX_ENTRIES_PER_MESSAGE {
                entries.drain().collect()
            } else {
                let keys: Vec<Cid> = entries
                    .keys()
                    .take(MAX_ENTRIES_PER_MESSAGE)
                    .copied()
                    .collect();
                let batch = keys
                    .iter()
                    .filter_map(|cid| entries.remove(cid).map(|intent| (*cid, intent)))
                    .collect();
                self.queued.insert(peer, entries);
                batch
            };
            let have_ok = protocol(&peer).is_none_or(ProtocolId::supports_have);
            let mut message = BitswapMessage::new(false);
            let mut requests = Vec::new();
            for (cid, intent) in batch {
                match intent {
                    Intent::Cancel => {
                        message.cancel(cid);
                    }
                    Intent::Want(kind, seq) => {
                        let want = match kind {
                            Kind::Have if have_ok => WantType::Have,
                            _ => WantType::Block,
                        };
                        message.add_entry(cid, PRIORITY, want, true);
                        requests.push((cid, seq));
                    }
                }
            }
            self.busy.insert(peer);
            ready.push(Outgoing {
                peer,
                message,
                requests,
            });
        }
        ready
    }
}
