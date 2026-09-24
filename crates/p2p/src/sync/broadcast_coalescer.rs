//! Short-window coalescing for rapid document and collection gossip updates (#1102).

use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cid::Cid;
use kovan::{Atom, AtomOption};
use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use tokio::sync::Notify;

use super::BroadcastResult;
use crate::message::PushLogBroadcast;

pub(crate) const DEFAULT_BROADCAST_COALESCING_WINDOW: Duration = Duration::from_millis(250);
pub(crate) const DEFAULT_BROADCAST_MAX_COALESCING_DELAY: Duration = Duration::from_secs(1);

type SharedResult = std::result::Result<BroadcastResult, String>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct BroadcastKey {
    collection_id: String,
    doc_id: String,
}

#[derive(Clone)]
struct Latest {
    version: (u64, Cid),
    broadcast: PushLogBroadcast,
    updated_at: n0_future::time::Instant,
    sealed: bool,
}

struct PendingBroadcast {
    latest: Atom<Latest>,
    started_at: n0_future::time::Instant,
    result: AtomOption<SharedResult>,
    cancelled: AtomicBool,
    notify: Notify,
}

impl PendingBroadcast {
    fn new(
        broadcast: PushLogBroadcast,
        version: (u64, Cid),
        now: n0_future::time::Instant,
    ) -> Self {
        Self {
            latest: Atom::new(Latest {
                version,
                broadcast,
                updated_at: now,
                sealed: false,
            }),
            started_at: now,
            result: AtomOption::none(),
            cancelled: AtomicBool::new(false),
            notify: Notify::new(),
        }
    }

    /// Buffers `candidate` when it is newer than the window's head. Once the
    /// leader has sealed the window a newer candidate comes back as `Some`,
    /// so the caller re-elects instead of silently losing the update.
    fn offer(
        &self,
        candidate: PushLogBroadcast,
        version: (u64, Cid),
        now: n0_future::time::Instant,
    ) -> Option<PushLogBroadcast> {
        let mut candidate = candidate;
        loop {
            let current = self.latest.load();
            if version <= current.version {
                return None;
            }
            if current.sealed {
                return Some(candidate);
            }
            let next = Latest {
                version,
                broadcast: candidate,
                updated_at: now,
                sealed: false,
            };
            match self.latest.compare_and_swap(&current, next) {
                Ok(_) => return None,
                Err(next) => candidate = next.broadcast,
            }
        }
    }

    fn seal(&self) -> PushLogBroadcast {
        self.latest.rcu(|current| Latest {
            sealed: true,
            ..current.clone()
        });
        self.latest.peek(|current| current.broadcast.clone())
    }

    fn updated_at(&self) -> n0_future::time::Instant {
        self.latest.peek(|current| current.updated_at)
    }
}

pub(crate) struct BroadcastCoalescer {
    pending: HopscotchMap<BroadcastKey, Arc<PendingBroadcast>, RandomState>,
    window: Duration,
    max_delay: Duration,
    coalesced: AtomicU64,
}

struct BroadcastLeaderGuard<'a> {
    coalescer: &'a BroadcastCoalescer,
    key: BroadcastKey,
    pending: Arc<PendingBroadcast>,
    listed: bool,
    armed: bool,
}

impl BroadcastLeaderGuard<'_> {
    fn unlist(&mut self) {
        if std::mem::take(&mut self.listed) {
            self.coalescer.pending.force_remove(&self.key);
        }
    }

    fn complete(&mut self) {
        self.armed = false;
    }
}

impl Drop for BroadcastLeaderGuard<'_> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        self.unlist();
        self.pending.cancelled.store(true, Ordering::Release);
        self.pending.notify.notify_waiters();
    }
}

impl Default for BroadcastCoalescer {
    fn default() -> Self {
        Self::with_limits(
            DEFAULT_BROADCAST_COALESCING_WINDOW,
            DEFAULT_BROADCAST_MAX_COALESCING_DELAY,
        )
    }
}

impl BroadcastCoalescer {
    #[cfg(test)]
    fn with_window(window: Duration) -> Self {
        Self::with_limits(window, window * 4)
    }

    fn with_limits(window: Duration, max_delay: Duration) -> Self {
        Self {
            pending: HopscotchMap::with_hasher(RandomState::default()),
            window,
            max_delay,
            coalesced: AtomicU64::new(0),
        }
    }

    pub(crate) fn coalesced(&self) -> u64 {
        self.coalesced.load(Ordering::Relaxed)
    }

    /// Cancellation safe: if the leader is dropped, its guard removes the
    /// dead window and wakes followers to re-admit the latest buffered update.
    pub(crate) async fn run<F, Fut>(&self, broadcast: PushLogBroadcast, send: F) -> SharedResult
    where
        F: FnOnce(PushLogBroadcast) -> Fut,
        Fut: Future<Output = SharedResult>,
    {
        let mut candidate = broadcast;
        let mut send = Some(send);
        loop {
            let Some(incoming_version) = version(&candidate) else {
                // Without a decoded priority there is no safe proof that this
                // update subsumes another scope head.
                return send.take().expect("send closure available")(candidate).await;
            };
            let key = BroadcastKey {
                collection_id: candidate.collection_id.clone(),
                doc_id: candidate.doc_id.clone(),
            };
            let now = n0_future::time::Instant::now();
            let (pending, leader) = match self.pending.get(&key) {
                Some(pending) => (pending, false),
                None => {
                    let fresh = Arc::new(PendingBroadcast::new(
                        candidate.clone(),
                        incoming_version,
                        now,
                    ));
                    match self
                        .pending
                        .insert_if_absent(key.clone(), Arc::clone(&fresh))
                    {
                        None => (fresh, true),
                        Some(existing) => (existing, false),
                    }
                }
            };

            if leader {
                let mut guard = BroadcastLeaderGuard {
                    coalescer: self,
                    key,
                    pending: Arc::clone(&pending),
                    listed: true,
                    armed: true,
                };
                wait_for_quiet(&pending, self.window, self.max_delay).await;
                guard.unlist();
                let latest = pending.seal();
                let result = send.take().expect("send closure available")(latest).await;
                pending.result.store_some(result.clone());
                pending.notify.notify_waiters();
                guard.complete();
                return result;
            }

            if let Some(sealed_out) = pending.offer(candidate, incoming_version, now) {
                candidate = sealed_out;
                continue;
            }
            self.coalesced.fetch_add(1, Ordering::Relaxed);

            loop {
                let notified = pending.notify.notified();
                tokio::pin!(notified);
                // Register before checking the result. `notify_waiters` does
                // not retain a permit, so polling only after the check can
                // miss the leader's one completion notification forever.
                notified.as_mut().enable();
                let result = pending.result.load().map(|result| (*result).clone());
                if let Some(result) = result {
                    return result;
                }
                if pending.cancelled.load(Ordering::Acquire) {
                    candidate = pending.latest.peek(|current| current.broadcast.clone());
                    break;
                }
                notified.await;
            }
        }
    }
}

async fn wait_for_quiet(pending: &PendingBroadcast, window: Duration, max_delay: Duration) {
    let max_deadline = pending.started_at + max_delay;
    loop {
        let deadline = (pending.updated_at() + window).min(max_deadline);
        n0_future::time::sleep_until(deadline).await;
        let now = n0_future::time::Instant::now();
        if now >= max_deadline || now >= pending.updated_at() + window {
            return;
        }
    }
}

fn version(broadcast: &PushLogBroadcast) -> Option<(u64, Cid)> {
    let priority = match defra_core::Block::from_dag_cbor(&broadcast.block) {
        Ok(block) => block.delta.priority(),
        Err(error) => {
            tracing::warn!(
                cid = %String::from_utf8_lossy(&broadcast.cid),
                %error,
                "broadcast head priority decode failed; bypassing scope coalescing"
            );
            return None;
        }
    };
    let cid = match Cid::try_from(broadcast.cid.as_ref()) {
        Ok(cid) => cid,
        Err(error) => {
            tracing::warn!(%error, "broadcast CID decode failed; bypassing scope coalescing");
            return None;
        }
    };
    Some((priority, cid))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use bytes::Bytes;
    use multihash_codetable::{Code, MultihashDigest};

    use super::*;

    fn broadcast(seed: &[u8]) -> PushLogBroadcast {
        use defra_core::{Block, CompositeDeltaPayload, CrdtDelta};

        let block = Block::new_with_options(
            CrdtDelta::Composite(CompositeDeltaPayload {
                schema_version_id: "schema".to_string(),
                priority: seed.iter().map(|byte| u64::from(*byte)).sum(),
                status: 1,
            }),
            vec![],
            vec![],
            None,
            None,
        );
        let block = Bytes::from(block.to_dag_cbor().unwrap());
        let cid = defra_core::block::generate_cid_from_bytes(&block).unwrap();
        PushLogBroadcast::new(
            "doc".to_string(),
            Bytes::from(cid.to_bytes()),
            "collection".to_string(),
            "creator".to_string(),
            block,
        )
    }

    fn undecodable_broadcast(seed: &[u8]) -> PushLogBroadcast {
        let cid = Cid::new_v1(0x55, Code::Sha2_256.digest(seed));
        PushLogBroadcast::new(
            "doc".to_string(),
            Bytes::from(cid.to_bytes()),
            "collection".to_string(),
            "creator".to_string(),
            Bytes::copy_from_slice(seed),
        )
    }

    #[tokio::test]
    async fn rapid_updates_publish_only_the_greatest_version() {
        let coalescer = Arc::new(BroadcastCoalescer::with_window(Duration::from_millis(10)));
        let sends = Arc::new(AtomicUsize::new(0));
        let sent_cid = Arc::new(AtomOption::<Bytes>::none());
        let updates: Vec<_> = [b"1".as_slice(), b"2".as_slice(), b"3".as_slice()]
            .into_iter()
            .map(broadcast)
            .collect();
        let expected_cid = updates
            .iter()
            .max_by_key(|update| version(update))
            .unwrap()
            .cid
            .clone();
        let mut tasks = Vec::new();
        for update in updates {
            let coalescer = Arc::clone(&coalescer);
            let sends = Arc::clone(&sends);
            let sent_cid = Arc::clone(&sent_cid);
            tasks.push(n0_future::task::spawn(async move {
                coalescer
                    .run(update, move |latest| async move {
                        sends.fetch_add(1, Ordering::Relaxed);
                        sent_cid.store_some(latest.cid);
                        Ok(BroadcastResult::Success)
                    })
                    .await
            }));
        }
        for task in tasks {
            assert_eq!(task.await.unwrap().unwrap(), BroadcastResult::Success);
        }
        assert_eq!(sends.load(Ordering::Relaxed), 1);
        assert_eq!(sent_cid.load().as_deref(), Some(&expected_cid));
        assert_eq!(coalescer.coalesced(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn sequential_update_resets_the_quiet_window() {
        let window = Duration::from_millis(40);
        let coalescer = Arc::new(BroadcastCoalescer::with_window(window));
        let sends = Arc::new(AtomicUsize::new(0));
        let leader = {
            let coalescer = Arc::clone(&coalescer);
            let sends = Arc::clone(&sends);
            n0_future::task::spawn(async move {
                coalescer
                    .run(broadcast(b"1"), move |_| async move {
                        sends.fetch_add(1, Ordering::Relaxed);
                        Ok(BroadcastResult::Success)
                    })
                    .await
            })
        };
        tokio::task::yield_now().await;
        tokio::time::advance(window / 2).await;
        let follower = {
            let coalescer = Arc::clone(&coalescer);
            let sends = Arc::clone(&sends);
            n0_future::task::spawn(async move {
                coalescer
                    .run(broadcast(b"second"), move |_| async move {
                        sends.fetch_add(1, Ordering::Relaxed);
                        Ok(BroadcastResult::Success)
                    })
                    .await
            })
        };
        tokio::task::yield_now().await;

        tokio::time::advance(window * 3 / 4).await;
        assert_eq!(sends.load(Ordering::Relaxed), 0);
        leader.await.unwrap().unwrap();
        follower.await.unwrap().unwrap();
        assert_eq!(sends.load(Ordering::Relaxed), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn stale_update_does_not_reset_the_quiet_window() {
        let window = Duration::from_millis(40);
        let coalescer = Arc::new(BroadcastCoalescer::with_window(window));
        let sends = Arc::new(AtomicUsize::new(0));
        let leader = {
            let coalescer = Arc::clone(&coalescer);
            let sends = Arc::clone(&sends);
            n0_future::task::spawn(async move {
                coalescer
                    .run(broadcast(b"newer"), move |_| async move {
                        sends.fetch_add(1, Ordering::Relaxed);
                        Ok(BroadcastResult::Success)
                    })
                    .await
            })
        };
        tokio::task::yield_now().await;
        tokio::time::advance(window / 2).await;
        let follower = {
            let coalescer = Arc::clone(&coalescer);
            n0_future::task::spawn(async move {
                coalescer
                    .run(broadcast(b"old"), |_| async { unreachable!() })
                    .await
            })
        };
        while coalescer.coalesced() == 0 {
            tokio::task::yield_now().await;
        }

        tokio::time::advance(window / 2).await;
        tokio::task::yield_now().await;
        assert_eq!(sends.load(Ordering::Relaxed), 1);
        assert_eq!(leader.await.unwrap().unwrap(), BroadcastResult::Success);
        assert_eq!(follower.await.unwrap().unwrap(), BroadcastResult::Success);
    }

    #[tokio::test(start_paused = true)]
    async fn sustained_updates_flush_at_the_max_delay() {
        let window = Duration::from_millis(250);
        let max_delay = Duration::from_secs(1);
        let coalescer = Arc::new(BroadcastCoalescer::with_limits(window, max_delay));
        let sends = Arc::new(AtomicUsize::new(0));
        let leader = {
            let coalescer = Arc::clone(&coalescer);
            let sends = Arc::clone(&sends);
            n0_future::task::spawn(async move {
                coalescer
                    .run(broadcast(b"1"), move |_| async move {
                        sends.fetch_add(1, Ordering::Relaxed);
                        Ok(BroadcastResult::Success)
                    })
                    .await
            })
        };
        tokio::task::yield_now().await;

        let mut followers = Vec::new();
        for seed in [b"22".as_slice(), b"333", b"4444", b"55555"] {
            tokio::time::advance(Duration::from_millis(200)).await;
            let coalescer = Arc::clone(&coalescer);
            followers.push(n0_future::task::spawn(async move {
                coalescer
                    .run(broadcast(seed), |_| async { unreachable!() })
                    .await
            }));
            tokio::task::yield_now().await;
        }
        tokio::time::advance(Duration::from_millis(200)).await;

        assert_eq!(leader.await.unwrap().unwrap(), BroadcastResult::Success);
        for follower in followers {
            assert_eq!(follower.await.unwrap().unwrap(), BroadcastResult::Success);
        }
        assert_eq!(sends.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn undecodable_heads_bypass_document_coalescing() {
        let coalescer = BroadcastCoalescer::with_window(Duration::from_millis(10));
        let sends = Arc::new(AtomicUsize::new(0));
        for seed in [b"first".as_slice(), b"second"] {
            let sends = Arc::clone(&sends);
            coalescer
                .run(undecodable_broadcast(seed), move |_| async move {
                    sends.fetch_add(1, Ordering::Relaxed);
                    Ok(BroadcastResult::Success)
                })
                .await
                .unwrap();
        }
        assert_eq!(sends.load(Ordering::Relaxed), 2);
        assert_eq!(coalescer.coalesced(), 0);
    }

    #[tokio::test]
    async fn follower_re_elects_after_leader_cancellation() {
        let coalescer = Arc::new(BroadcastCoalescer::with_window(Duration::from_millis(200)));
        let leader = {
            let coalescer = Arc::clone(&coalescer);
            n0_future::task::spawn(async move {
                coalescer
                    .run(broadcast(b"leader"), |_| async { unreachable!() })
                    .await
            })
        };
        while coalescer.pending.is_empty() {
            tokio::task::yield_now().await;
        }

        let sends = Arc::new(AtomicUsize::new(0));
        let sent_cid = Arc::new(AtomOption::<Bytes>::none());
        let follower = {
            let coalescer = Arc::clone(&coalescer);
            let sends = Arc::clone(&sends);
            let sent_cid = Arc::clone(&sent_cid);
            n0_future::task::spawn(async move {
                coalescer
                    .run(broadcast(b"follower"), move |latest| async move {
                        sends.fetch_add(1, Ordering::Relaxed);
                        sent_cid.store_some(latest.cid);
                        Ok(BroadcastResult::Success)
                    })
                    .await
            })
        };
        while coalescer.coalesced() == 0 {
            tokio::task::yield_now().await;
        }
        let expected_cid = coalescer
            .pending
            .values()
            .next()
            .unwrap()
            .latest
            .peek(|current| current.broadcast.cid.clone());

        leader.abort();
        assert!(leader.await.unwrap_err().is_cancelled());
        assert_eq!(
            n0_future::time::timeout(Duration::from_secs(1), follower)
                .await
                .expect("replacement leader must complete")
                .unwrap()
                .unwrap(),
            BroadcastResult::Success
        );
        assert_eq!(sends.load(Ordering::Relaxed), 1);
        assert_eq!(sent_cid.load().as_deref(), Some(&expected_cid));
        assert!(coalescer.pending.is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn many_followers_receive_completion_without_a_missed_wakeup() {
        const FOLLOWERS: usize = 256;
        let coalescer = Arc::new(BroadcastCoalescer::with_window(Duration::from_millis(250)));
        let leader = {
            let coalescer = Arc::clone(&coalescer);
            n0_future::task::spawn(async move {
                coalescer
                    .run(broadcast(b"leader"), |_| async {
                        Ok(BroadcastResult::Success)
                    })
                    .await
            })
        };
        while coalescer.pending.is_empty() {
            tokio::task::yield_now().await;
        }

        let mut followers = Vec::new();
        for seed in 0..FOLLOWERS {
            let coalescer = Arc::clone(&coalescer);
            followers.push(n0_future::task::spawn(async move {
                coalescer
                    .run(broadcast(&seed.to_le_bytes()), |_| async { unreachable!() })
                    .await
            }));
        }
        n0_future::time::timeout(Duration::from_secs(1), async {
            while coalescer.coalesced() < FOLLOWERS as u64 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("all followers must join the leader's window");

        assert_eq!(leader.await.unwrap().unwrap(), BroadcastResult::Success);
        for follower in followers {
            assert_eq!(
                n0_future::time::timeout(Duration::from_secs(1), follower)
                    .await
                    .expect("follower must wake")
                    .unwrap()
                    .unwrap(),
                BroadcastResult::Success
            );
        }
    }
}
