//! Go Shinzo's CAR exchange: what a source puts in a `CARReply`, what a
//! requester accepts from one, and which peers it stops asking.
//!
//! Modeled by `proofs/tla/ShinzoCarExchange.tla`: [`serve_cars`] is the
//! source side (`Build`), [`accept_reply`] the requester side (`Reply`), and
//! [`CarPeerBackoff`] decides who is passed over (`Fail`, `standing`). Go:
//! shinzo-develop `internal/db/p2p/car_exchange.go`.

use std::collections::VecDeque;
use std::future::Future;
use std::time::{Duration, Instant};

use bytes::Bytes;
use cid::Cid;
use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use rapidhash::{HashSetExt, RapidHashSet};

use crate::error::Result;
use crate::message::car_exchange::CarReply;
use crate::message::MAX_INBOUND_DOCUMENTS;
use crate::sync::car::decode_car;

#[cfg(test)]
#[path = "../../tests/unit/shinzo_car.rs"]
mod tests;

/// The most heads one reply attempts: the largest batch a Go receiver takes.
pub const MAX_CAR_REQUEST_CIDS: usize = MAX_INBOUND_DOCUMENTS;

/// The most CAR bytes in one reply, and the largest CAR ever sent: Go's
/// `maxCARReplyBytes`, under the 16 MiB message cap so the envelope fits.
pub const MAX_CAR_REPLY_BYTES: usize = 8 << 20;

/// What a source does with one asked head.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeadCar {
    /// Its CAR, rooted at the head.
    Car(Vec<u8>),
    /// Not served: malformed, not held, not readable by the peer, not a
    /// document block, or failed to build. An empty entry.
    Skip,
}

/// Build a reply for the asked heads, in order. `build` gives each head's
/// CAR or a skip; it runs only for heads the reply reaches. A CAR bigger
/// than a whole reply is skipped; one that does not fit what is left ends the
/// reply there, and the requester asks for the rest again. Heads past
/// [`MAX_CAR_REQUEST_CIDS`] are never attempted.
pub async fn serve_cars<F, Fut>(heads: &[&[u8]], mut build: F) -> Vec<Vec<u8>>
where
    F: FnMut(Cid) -> Fut,
    Fut: Future<Output = HeadCar>,
{
    let heads = &heads[..heads.len().min(MAX_CAR_REQUEST_CIDS)];
    let mut cars = Vec::with_capacity(heads.len());
    let mut budget = MAX_CAR_REPLY_BYTES;
    for raw in heads {
        let Ok(head) = Cid::try_from(*raw) else {
            cars.push(Vec::new());
            continue;
        };
        match build(head).await {
            HeadCar::Skip => cars.push(Vec::new()),
            HeadCar::Car(car) if car.len() > MAX_CAR_REPLY_BYTES => cars.push(Vec::new()),
            HeadCar::Car(car) if car.len() > budget => break,
            HeadCar::Car(car) => {
                budget -= car.len();
                cars.push(car);
            }
        }
    }
    cars
}

/// What a requester took from one asked head's entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// A CAR rooted at the head: import it.
    Fetched(Vec<u8>),
    /// Empty, or rooted elsewhere: fetch the head another way.
    NotServed,
}

/// Why a reply as a whole was refused; every asked head falls back.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BadReply {
    #[error("peer reported an error: {0}")]
    Error(String),
    #[error("reply answered no head")]
    Empty,
    #[error("reply has {got} entries for {asked} heads")]
    TooLong { got: usize, asked: usize },
}

/// Match a reply to the heads asked, position by position. Entries cover a
/// prefix: heads past the end were not tried. A reply with an error, no
/// entries, or more entries than heads is refused whole.
pub fn accept_reply(asked: &[Cid], reply: &CarReply) -> std::result::Result<Vec<Entry>, BadReply> {
    if let Some(error) = reply
        .err_message
        .as_deref()
        .filter(|error| !error.is_empty())
    {
        return Err(BadReply::Error(error.to_string()));
    }
    if reply.is_empty() {
        return Err(BadReply::Empty);
    }
    if reply.len() > asked.len() {
        return Err(BadReply::TooLong {
            got: reply.len(),
            asked: asked.len(),
        });
    }
    Ok(asked
        .iter()
        .zip(reply.car_bytes())
        .map(|(head, car)| {
            if car_rooted_at(car, head) {
                Entry::Fetched(car.to_vec())
            } else {
                Entry::NotServed
            }
        })
        .collect())
}

/// Whether `car` is a CAR whose sole root is `head`.
pub fn car_rooted_at(car: &[u8], head: &Cid) -> bool {
    !car.is_empty() && decode_car(car).is_ok_and(|(roots, _)| roots == [*head])
}

/// Collect one commit's blocks, as Go's `buildCAR` does: the head, the blocks
/// its links name (each followed by its own links), and their signature
/// blocks. Heads are not followed, so the CAR carries no history. Encryption
/// blocks stay out: they travel only over the KMS topic (issue #976).
///
/// `None` when the head is not held or is not a document block, or when any
/// block it needs is missing: a receiver imports a CAR in place of walking
/// the DAG, so a CAR missing a block would leave that block unfetched. Once
/// the blocks pass [`MAX_CAR_REPLY_BYTES`] collection stops, and the CAR is
/// too big to send anyway.
pub async fn collect_commit_blocks<G, Fut>(
    head: Cid,
    mut get: G,
) -> Result<Option<Vec<(Cid, Bytes)>>>
where
    G: FnMut(Cid) -> Fut,
    Fut: Future<Output = Result<Option<Bytes>>>,
{
    let mut blocks = Vec::new();
    let mut seen = RapidHashSet::new();
    let mut queue = VecDeque::from([head]);
    let mut total = 0usize;
    while let Some(cid) = queue.pop_front() {
        if !seen.insert(cid) {
            continue;
        }
        let Some(data) = get(cid).await? else {
            return Ok(None);
        };
        match defra_core::Block::from_dag_cbor(&data) {
            Ok(block) => {
                queue.extend(block.links.iter().flatten().map(|link| link.link));
                queue.extend(block.signature);
            }
            // The head must be a document block, as an update announces one.
            Err(_) if cid == head => return Ok(None),
            Err(_) => {}
        }
        total += data.len();
        blocks.push((cid, data));
        if total > MAX_CAR_REPLY_BYTES {
            break;
        }
    }
    Ok(Some(blocks))
}

/// The first window after a peer's failure; it doubles per failure in a row.
const BACKOFF_BASE: Duration = Duration::from_millis(250);
/// The longest window.
const BACKOFF_MAX: Duration = Duration::from_secs(30);
/// Failures in a row, inside the window, after which a peer is not asked.
const HARD_FAILURES: u32 = 3;

/// How a request to a peer ended, for its backoff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// It answered (even with nothing usable).
    Answered,
    /// Its fault: timeout, dial failure, reset, resource limit.
    Fault,
    /// This node gave up or shut down; not the peer's doing.
    Cancelled,
}

/// Peers whose CAR requests keep failing. A peer that failed
/// [`HARD_FAILURES`] times in a row is passed over until its window ends;
/// one that answers is forgiven. Shared by the tasks that send requests; two
/// faults recorded at the same moment may count as one, which only shortens
/// a window.
pub struct CarPeerBackoff {
    peers: HopscotchMap<String, (Instant, u32), RandomState>,
}

impl Default for CarPeerBackoff {
    fn default() -> Self {
        Self {
            peers: HopscotchMap::with_hasher(RandomState::default()),
        }
    }
}

impl CarPeerBackoff {
    /// Whether `peer` is passed over at `now`; an expired window is forgotten.
    pub fn passed_over(&self, peer: &str, now: Instant) -> bool {
        match self.peers.get(peer) {
            Some((until, _)) if now >= until => {
                self.peers.remove(peer);
                false
            }
            Some((_, failures)) => failures >= HARD_FAILURES,
            None => false,
        }
    }

    /// Record how a request to `peer` ended at `now`.
    pub fn record(&self, peer: &str, outcome: Outcome, now: Instant) {
        match outcome {
            Outcome::Answered => {
                self.peers.remove(peer);
            }
            Outcome::Cancelled => {}
            Outcome::Fault => {
                let failures = self.peers.get(peer).map_or(0, |(_, failures)| failures) + 1;
                let shift = (failures - 1).min(8);
                let until = now + (BACKOFF_BASE * (1 << shift)).min(BACKOFF_MAX);
                self.peers.insert(peer.to_string(), (until, failures));
            }
        }
    }
}
