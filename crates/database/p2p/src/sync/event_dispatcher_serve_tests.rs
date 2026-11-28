//! The CAR-serve lane in the dispatcher (#2536): a full lane holds a request
//! for a bounded wait, a freed slot goes to the peer holding the fewest
//! serves, and a request answered busy is one whose wait ran out or that
//! found the waiting room full.

use std::sync::Arc;
use std::time::Duration;

use nuclei::sync::{mpsc, Semaphore};
use regolith::sync::SegQueue;

use super::*;

const LANE: usize = crate::sync::DEFAULT_MAX_ACTIVE_CAR_SERVES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Serve {
    Recovery(&'static str, usize),
    Inline(usize),
}

impl DispatchEvent for Serve {
    type ServePeer = &'static str;

    fn dispatch_class(&self) -> DispatchClass {
        match self {
            Self::Recovery(..) => DispatchClass::Recovery,
            Self::Inline(_) => DispatchClass::Inline,
        }
    }

    fn serve_peer(&self) -> Option<Self::ServePeer> {
        match self {
            Self::Recovery(peer, _) => Some(peer),
            Self::Inline(_) => None,
        }
    }
}

/// A running dispatcher whose serves from `blocking` hold their slot until
/// `release` gets a permit; every other serve returns at once.
struct Lane {
    tx: mpsc::Sender<Serve>,
    dispatcher: crate::rt::Task<()>,
    diagnostics: Arc<DispatchDiagnostics>,
    started: Arc<Semaphore>,
    release: Arc<Semaphore>,
    seen: Arc<SegQueue<(Serve, DispatchAdmission)>>,
}

fn lane(blocking: &'static str) -> Lane {
    let (tx, rx) = mpsc::channel(4 * LANE);
    let diagnostics = Arc::new(DispatchDiagnostics::default());
    let started = Arc::new(Semaphore::new(0));
    let release = Arc::new(Semaphore::new(0));
    let seen = Arc::new(SegQueue::new());
    let dispatcher = {
        let started = Arc::clone(&started);
        let release = Arc::clone(&release);
        let seen = Arc::clone(&seen);
        crate::rt::spawn(run_event_dispatcher(
            rx,
            edge::testing::edge(),
            Arc::clone(&diagnostics),
            move |event: Serve, admission| {
                let started = Arc::clone(&started);
                let release = Arc::clone(&release);
                let seen = Arc::clone(&seen);
                async move {
                    seen.push((event, admission));
                    if matches!(event, Serve::Recovery(peer, _) if peer == blocking)
                        && admission == DispatchAdmission::Admitted
                    {
                        started.add_permits(1);
                        let _permit = release.acquire().await.unwrap();
                    }
                }
            },
        ))
    };
    Lane {
        tx,
        dispatcher,
        diagnostics,
        started,
        release,
        seen,
    }
}

impl Lane {
    async fn fill(&self, peer: &'static str) {
        for index in 0..LANE {
            self.tx.send(Serve::Recovery(peer, index)).await.unwrap();
        }
        crate::rt::time::timeout(
            Duration::from_secs(1),
            self.started.acquire_many(LANE as u32),
        )
        .await
        .expect("the lane fills")
        .unwrap()
        .forget();
    }

    /// The events handled so far, waiting up to five seconds until `done` holds.
    async fn until<F>(&self, done: F) -> Vec<(Serve, DispatchAdmission)>
    where
        F: Fn(&[(Serve, DispatchAdmission)]) -> bool,
    {
        let mut events = Vec::new();
        crate::rt::time::timeout(Duration::from_secs(5), async {
            loop {
                while let Some(event) = self.seen.pop() {
                    events.push(event);
                }
                if done(&events) {
                    break;
                }
                crate::rt::yield_now().await;
            }
        })
        .await
        .expect("the dispatcher handles the expected events");
        events
    }

    async fn stop(self) {
        self.release.add_permits(4 * LANE);
        drop(self.tx);
        self.dispatcher.join().await.unwrap();
    }
}

#[nuclei::test(threads = 0)]
async fn a_serve_that_finds_the_lane_full_waits_for_a_slot() {
    let lane = lane("hub");
    lane.fill("hub").await;
    lane.tx.send(Serve::Recovery("spoke", 0)).await.unwrap();
    lane.until(|_| lane.diagnostics.snapshot().waiting_recovery == 1)
        .await;

    lane.release.add_permits(1);
    let events = lane
        .until(|events| {
            events.contains(&(Serve::Recovery("spoke", 0), DispatchAdmission::Admitted))
        })
        .await;
    assert!(
        !events
            .iter()
            .any(|(_, admission)| *admission != DispatchAdmission::Admitted),
        "a serve whose slot frees within the wait is never answered busy"
    );
    let snapshot = lane.diagnostics.snapshot();
    assert_eq!(snapshot.recovery_capacity, LANE);
    assert_eq!(snapshot.recovery_waited_total, 1);
    assert_eq!(snapshot.waiting_recovery_high_water, 1);
    assert_eq!(snapshot.recovery_saturated_total, 0);
    lane.stop().await;
}

#[nuclei::test(threads = 0)]
async fn a_wait_that_runs_out_is_answered_busy() {
    let lane = lane("hub");
    lane.fill("hub").await;
    lane.tx.send(Serve::Recovery("spoke", 0)).await.unwrap();

    lane.until(|events| {
        events.contains(&(Serve::Recovery("spoke", 0), DispatchAdmission::Saturated))
    })
    .await;
    let snapshot = lane.diagnostics.snapshot();
    assert_eq!(snapshot.active_recovery, LANE);
    assert_eq!(snapshot.waiting_recovery, 0);
    assert_eq!(snapshot.recovery_wait_expired_total, 1);
    assert_eq!(snapshot.recovery_saturated_total, 1);
    assert_eq!(snapshot.admission_saturated_total, 0);
    assert_eq!(snapshot.saturated_total, 1);
    lane.stop().await;
}

#[nuclei::test(threads = 0)]
async fn a_freed_slot_goes_to_the_peer_holding_the_fewest_serves() {
    let lane = lane("hog");
    lane.fill("hog").await;
    lane.tx.send(Serve::Recovery("hog", LANE)).await.unwrap();
    lane.tx.send(Serve::Recovery("spoke", 0)).await.unwrap();
    lane.until(|_| lane.diagnostics.snapshot().waiting_recovery == 2)
        .await;

    lane.release.add_permits(1);
    let events = lane
        .until(|events| {
            events.contains(&(Serve::Recovery("hog", LANE), DispatchAdmission::Admitted))
        })
        .await;
    let position = |serve| {
        events
            .iter()
            .position(|event| *event == (serve, DispatchAdmission::Admitted))
            .unwrap()
    };
    assert!(
        position(Serve::Recovery("spoke", 0)) < position(Serve::Recovery("hog", LANE)),
        "the spoke holding no serve goes before the older request of the hog"
    );
    lane.stop().await;
}

#[nuclei::test(threads = 0)]
async fn a_peer_past_its_share_of_the_waiting_room_is_answered_busy_at_once() {
    let share = (LANE / 4).max(1);
    let lane = lane("hub");
    lane.fill("hub").await;
    for index in 0..=share {
        lane.tx.send(Serve::Recovery("spoke", index)).await.unwrap();
    }

    let events = lane
        .until(|events| {
            events.contains(&(
                Serve::Recovery("spoke", share),
                DispatchAdmission::Saturated,
            ))
        })
        .await;
    assert_eq!(
        events
            .iter()
            .find(|(_, admission)| *admission == DispatchAdmission::Saturated),
        Some(&(
            Serve::Recovery("spoke", share),
            DispatchAdmission::Saturated
        )),
        "the request past the peer's share is answered before any wait runs out"
    );
    assert!(lane.diagnostics.snapshot().waiting_recovery_high_water >= share);
    lane.stop().await;
}

#[nuclei::test(threads = 0)]
async fn draining_other_events_keeps_one_wait_timer_armed() {
    let lane = lane("hub");
    lane.fill("hub").await;
    lane.tx.send(Serve::Recovery("spoke", 0)).await.unwrap();
    lane.until(|_| lane.diagnostics.snapshot().waiting_recovery == 1)
        .await;
    let armed = lane.diagnostics.serve_deadline_arms();

    for index in 0..32 {
        lane.tx.send(Serve::Inline(index)).await.unwrap();
    }
    lane.until(|events| {
        events
            .iter()
            .filter(|event| matches!(event, (Serve::Inline(_), DispatchAdmission::Admitted)))
            .count()
            == 32
    })
    .await;

    assert_eq!(
        lane.diagnostics.serve_deadline_arms(),
        armed,
        "draining non-serving events must not rebuild the waiter's timer"
    );
    lane.stop().await;
}
