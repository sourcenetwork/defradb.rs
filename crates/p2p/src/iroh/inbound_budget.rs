//! Room in memory for inbound message bodies, taken before a body is read.
//!
//! Each [`Priority`] has its own byte budget, and each connection may hold at
//! most half of any one budget. A body's room is held until its event has been
//! handled. Modeled in `proofs/tla/InboundAdmission.tla`; the accounting is
//! proved in `proofs/lean/ByteBudget/Accounting.lean`.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use regolith::sync::{OwnedSemaphorePermit, Semaphore};

use super::services::Priority;
use crate::transport::InboundClassSnapshot;

/// Bytes each priority may hold, in [`Priority::ALL`] order. Half of each
/// must fit that priority's largest message.
const BUDGET_BYTES: [usize; 4] = [
    32 * 1024 * 1024,
    64 * 1024 * 1024,
    64 * 1024 * 1024,
    128 * 1024 * 1024,
];

/// The most body bytes one connection may hold across every class, which is
/// also its QUIC receive window.
pub const CONNECTION_RECEIVE_WINDOW: usize = share_of(BUDGET_BYTES[0])
    + share_of(BUDGET_BYTES[1])
    + share_of(BUDGET_BYTES[2])
    + share_of(BUDGET_BYTES[3]);

/// How long a stream waits for room before it is reset.
pub const ADMIT_DEADLINE: Duration = Duration::from_secs(5);

/// The node-wide inbound byte budgets.
#[derive(Debug)]
pub struct InboundBudget {
    classes: [ClassBudget; 4],
}

#[derive(Debug)]
struct ClassBudget {
    bytes: Arc<Semaphore>,
    stats: Arc<ClassStats>,
}

#[derive(Debug, Default)]
struct ClassStats {
    held: AtomicUsize,
    high_water: AtomicUsize,
    busy_resets: AtomicU64,
    oversized_resets: AtomicU64,
}

/// One connection's share of every class budget.
#[derive(Debug)]
pub struct ConnectionShare {
    bytes: [Arc<Semaphore>; 4],
}

/// No room was free before the deadline.
#[derive(Debug, PartialEq, Eq)]
pub struct Busy;

/// Room for one body, given back when dropped.
#[derive(Debug)]
pub struct InboundLease {
    _share: OwnedSemaphorePermit,
    _class: OwnedSemaphorePermit,
    len: usize,
    stats: Arc<ClassStats>,
}

impl Drop for InboundLease {
    fn drop(&mut self) {
        self.stats.held.fetch_sub(self.len, Ordering::Relaxed);
    }
}

impl Default for InboundBudget {
    fn default() -> Self {
        Self {
            classes: BUDGET_BYTES.map(|capacity| ClassBudget {
                bytes: Arc::new(Semaphore::new(capacity)),
                stats: Arc::default(),
            }),
        }
    }
}

impl InboundBudget {
    /// A new connection's share: half of each class budget.
    pub fn connection_share(&self) -> ConnectionShare {
        ConnectionShare {
            bytes: BUDGET_BYTES.map(|capacity| Arc::new(Semaphore::new(share_of(capacity)))),
        }
    }

    /// Waits up to `deadline` for room for a `len`-byte body. The connection's
    /// share is taken first, so a connection already at its share never holds
    /// class bytes while it waits.
    pub async fn admit(
        &self,
        share: &ConnectionShare,
        priority: Priority,
        len: usize,
        deadline: Duration,
    ) -> Result<InboundLease, Busy> {
        let class = &self.classes[priority.index()];
        let acquire = async {
            let share = share.bytes[priority.index()].acquire_owned(len).await;
            let class_bytes = class.bytes.acquire_owned(len).await;
            (share, class_bytes)
        };
        let Ok((share, class_bytes)) = n0_future::time::timeout(deadline, acquire).await else {
            class.stats.busy_resets.fetch_add(1, Ordering::Relaxed);
            return Err(Busy);
        };
        let held = class.stats.held.fetch_add(len, Ordering::Relaxed) + len;
        class.stats.high_water.fetch_max(held, Ordering::Relaxed);
        Ok(InboundLease {
            _share: share,
            _class: class_bytes,
            len,
            stats: Arc::clone(&class.stats),
        })
    }

    pub fn record_oversized(&self, priority: Priority) {
        self.classes[priority.index()]
            .stats
            .oversized_resets
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> [InboundClassSnapshot; 4] {
        Priority::ALL.map(|priority| {
            let stats = &self.classes[priority.index()].stats;
            InboundClassSnapshot {
                class: priority.name(),
                capacity_bytes: BUDGET_BYTES[priority.index()],
                held_bytes: stats.held.load(Ordering::Relaxed),
                held_bytes_high_water: stats.high_water.load(Ordering::Relaxed),
                busy_resets: stats.busy_resets.load(Ordering::Relaxed),
                oversized_resets: stats.oversized_resets.load(Ordering::Relaxed),
            }
        })
    }
}

const fn share_of(capacity: usize) -> usize {
    capacity / 2
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iroh::services::SERVICES;

    const SHORT: Duration = Duration::from_millis(20);

    #[test]
    fn every_message_fits_a_connection_share() {
        for service in SERVICES {
            assert!(
                service.max_message <= share_of(BUDGET_BYTES[service.priority.index()]),
                "{:?} cannot fit its largest message",
                String::from_utf8_lossy(service.tag)
            );
        }
    }

    #[tokio::test]
    async fn a_lease_holds_its_bytes_until_dropped() {
        let budget = InboundBudget::default();
        let share = budget.connection_share();
        let len = share_of(BUDGET_BYTES[0]);

        let lease = budget
            .admit(&share, Priority::Control, len, SHORT)
            .await
            .unwrap();
        assert_eq!(budget.snapshot()[0].held_bytes, len);
        assert_eq!(
            budget
                .admit(&share, Priority::Control, 1, SHORT)
                .await
                .unwrap_err(),
            Busy
        );

        drop(lease);
        assert_eq!(budget.snapshot()[0].held_bytes, 0);
        assert_eq!(budget.snapshot()[0].held_bytes_high_water, len);
        assert_eq!(budget.snapshot()[0].busy_resets, 1);
        budget
            .admit(&share, Priority::Control, 1, SHORT)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn one_connection_cannot_take_a_whole_class() {
        let budget = InboundBudget::default();
        let greedy = budget.connection_share();
        let other = budget.connection_share();
        let half = share_of(BUDGET_BYTES[0]);

        let _held = budget
            .admit(&greedy, Priority::Control, half, SHORT)
            .await
            .unwrap();
        assert_eq!(
            budget
                .admit(&greedy, Priority::Control, 1, SHORT)
                .await
                .unwrap_err(),
            Busy
        );
        budget
            .admit(&other, Priority::Control, half, SHORT)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn a_full_class_does_not_block_another() {
        let budget = InboundBudget::default();
        let shares = [budget.connection_share(), budget.connection_share()];
        let half = share_of(BUDGET_BYTES[3]);
        let _bulk = [
            budget
                .admit(&shares[0], Priority::Bulk, half, SHORT)
                .await
                .unwrap(),
            budget
                .admit(&shares[1], Priority::Bulk, half, SHORT)
                .await
                .unwrap(),
        ];

        assert_eq!(
            budget
                .admit(&shares[0], Priority::Bulk, 1, SHORT)
                .await
                .unwrap_err(),
            Busy
        );
        budget
            .admit(&shares[0], Priority::Control, 1, SHORT)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn a_failed_admission_keeps_no_share() {
        let budget = InboundBudget::default();
        let shares = [budget.connection_share(), budget.connection_share()];
        let third = budget.connection_share();
        let half = share_of(BUDGET_BYTES[3]);
        let _bulk = [
            budget
                .admit(&shares[0], Priority::Bulk, half, SHORT)
                .await
                .unwrap(),
            budget
                .admit(&shares[1], Priority::Bulk, half, SHORT)
                .await
                .unwrap(),
        ];

        assert_eq!(
            budget
                .admit(&third, Priority::Bulk, half, SHORT)
                .await
                .unwrap_err(),
            Busy
        );
        assert_eq!(third.bytes[3].available_permits(), half);
    }
}
