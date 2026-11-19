use std::cell::{Cell, RefCell};
use std::error::Error;
use std::time::Duration;

use acp_light_client::TransportError;
use inputs::Jitter;

use super::{verified, verified_within, Pace, ATTEMPTS, BUSY_PATIENCE, BUSY_PAUSE};

/// More than the late answers below take to give way.
const PATIENT: Duration = Duration::from_secs(10);
/// Late answers that never give way.
const LATE_ANSWERS: usize = 4;
/// How long a node takes to answer busy when it waits out its whole
/// evidence deadline.
const NODE_DEADLINE: Duration = Duration::from_secs(2);

type Failure = Box<dyn Error + Send + Sync>;

const REFUSAL: &str = "ACP state changed during verification; retry the request";
const DECISION_REFUSAL: &str = "ACP state changed during decision verification; retry the request";

/// A clock that moves only when a read takes time or a pause is slept, with
/// pauses drawn from a seeded jitter.
struct TestPace {
    now: Cell<Duration>,
    jitter: Jitter,
    slept: RefCell<Vec<Duration>>,
    asked_at: RefCell<Vec<Duration>>,
    /// When the next fresh revision arrives, if one does.
    fresh_at: Cell<Option<Duration>>,
    waited: RefCell<Vec<Duration>>,
}

impl TestPace {
    fn new(seed: u64) -> Self {
        Self {
            now: Cell::new(Duration::ZERO),
            jitter: Jitter::new(seed),
            slept: RefCell::new(Vec::new()),
            asked_at: RefCell::new(Vec::new()),
            fresh_at: Cell::new(None),
            waited: RefCell::new(Vec::new()),
        }
    }

    /// One request that takes `took` to come back.
    fn ask(&self, took: Duration) {
        self.asked_at.borrow_mut().push(self.now.get());
        self.now.set(self.now.get() + took);
    }

    fn slept(&self) -> Vec<Duration> {
        self.slept.borrow().clone()
    }

    fn asked_at(&self) -> Vec<Duration> {
        self.asked_at.borrow().clone()
    }
}

impl Pace for TestPace {
    fn elapsed(&self) -> Duration {
        self.now.get()
    }

    fn pause(&self) -> Duration {
        self.jitter.pause(BUSY_PAUSE)
    }

    async fn sleep(&self, pause: Duration) {
        self.slept.borrow_mut().push(pause);
        self.now.set(self.now.get() + pause);
    }

    async fn fresh(&self, within: Duration) -> bool {
        self.waited.borrow_mut().push(within);
        let now = self.now.get();
        match self.fresh_at.get() {
            Some(at) if at <= now + within => {
                self.now.set(at.max(now));
                true
            }
            _ => {
                self.now.set(now + within);
                false
            }
        }
    }
}

const STALE: &str = "verified revision is stale; wait for a fresh finalized revision";

fn refused(message: &str) -> Failure {
    message.into()
}

/// The node's answer that the evidence a read asks for is not ready yet,
/// as the light client reports it: wrapped in the context of its read.
fn late() -> Failure {
    #[derive(Debug, thiserror::Error)]
    #[error("deserializing the proof")]
    struct Context(#[source] TransportError);

    Box::new(Context(TransportError::Rpc {
        method: "vera_getCurrentRecordProof",
        code: -32002,
        message: "current record evidence deadline exceeded".into(),
    }))
}

fn answered(outcome: Result<usize, Failure>) -> Option<usize> {
    outcome.ok()
}

/// Asks a node that answers busy every time, each answer taking `took`,
/// until the read's patience is spent.
async fn always_busy(pace: &TestPace, patience: Duration, took: Duration) -> Result<(), Failure> {
    verified_within(patience, pace, || {
        pace.ask(took);
        async { Err(late()) }
    })
    .await
}

#[nuclei::test]
async fn a_read_the_chain_overtook_is_asked_again_until_answered() {
    let asked = Cell::new(0);
    let heads = super::super::transport_tests::silent_heads().await;
    let outcome = verified(&heads, || {
        asked.set(asked.get() + 1);
        let ask = asked.get();
        async move {
            if ask < ATTEMPTS {
                Err(refused(REFUSAL))
            } else {
                Ok(ask)
            }
        }
    })
    .await;
    assert_eq!(answered(outcome), Some(ATTEMPTS));
    assert_eq!(asked.get(), ATTEMPTS);
}

#[nuclei::test]
async fn a_decision_the_chain_overtook_is_asked_again() {
    let asked = Cell::new(0);
    let heads = super::super::transport_tests::silent_heads().await;
    let outcome = verified(&heads, || {
        asked.set(asked.get() + 1);
        let ask = asked.get();
        async move {
            if ask == 1 {
                Err(refused(DECISION_REFUSAL))
            } else {
                Ok(ask)
            }
        }
    })
    .await;
    assert_eq!(answered(outcome), Some(2));
}

#[nuclei::test]
async fn a_read_overtaken_at_every_ask_returns_the_refusal_after_the_last() {
    let asked = Cell::new(0);
    let heads = super::super::transport_tests::silent_heads().await;
    let outcome: Result<(), _> = verified(&heads, || {
        asked.set(asked.get() + 1);
        async { Err(refused(REFUSAL)) }
    })
    .await;
    assert_eq!(outcome.unwrap_err().to_string(), REFUSAL);
    assert_eq!(asked.get(), ATTEMPTS);
}

#[nuclei::test]
async fn any_other_error_is_returned_at_the_first_ask() {
    let asked = Cell::new(0);
    let heads = super::super::transport_tests::silent_heads().await;
    let outcome: Result<(), _> = verified(&heads, || {
        asked.set(asked.get() + 1);
        async { Err(refused("no verified finalized state available")) }
    })
    .await;
    assert_eq!(
        outcome.unwrap_err().to_string(),
        "no verified finalized state available"
    );
    assert_eq!(asked.get(), 1);
}

/// A node error other than its busy code is the node's answer, returned at
/// once.
#[nuclei::test]
async fn another_node_error_is_returned_at_the_first_ask() {
    let asked = Cell::new(0);
    let heads = super::super::transport_tests::silent_heads().await;
    let outcome: Result<(), _> = verified(&heads, || {
        asked.set(asked.get() + 1);
        async {
            Err(Box::new(TransportError::Rpc {
                method: "vera_getCurrentRecordProof",
                code: -32602,
                message: "invalid params".into(),
            }) as Failure)
        }
    })
    .await;
    assert!(outcome.is_err());
    assert_eq!(asked.get(), 1);
}

/// On the wall clock too, a busy answer is asked again after a pause and the
/// read ends with the answer that follows.
#[nuclei::test]
async fn a_read_the_node_answered_late_once_is_answered_on_the_wall_clock() {
    let asked = Cell::new(0);
    let heads = super::super::transport_tests::silent_heads().await;
    let outcome = verified(&heads, || {
        asked.set(asked.get() + 1);
        let ask = asked.get();
        async move {
            if ask == 1 {
                Err(late())
            } else {
                Ok(ask)
            }
        }
    })
    .await;
    assert_eq!(answered(outcome), Some(2));
}

/// Evidence the node could not give in time is asked for again, and the
/// read ends with the answer that follows.
#[nuclei::test]
async fn a_read_the_node_answered_late_is_asked_again_until_answered() {
    let pace = TestPace::new(1);
    let asked = Cell::new(0);
    let outcome = verified_within(PATIENT, &pace, || {
        asked.set(asked.get() + 1);
        let ask = asked.get();
        async move {
            if ask <= LATE_ANSWERS {
                Err(late())
            } else {
                Ok(ask)
            }
        }
    })
    .await;
    assert_eq!(answered(outcome), Some(LATE_ANSWERS + 1));
    assert_eq!(pace.slept().len(), LATE_ANSWERS);
}

/// Two readers the node turned away together draw different pauses, each
/// between half of the pause and all of it, so they do not come back in
/// step.
#[nuclei::test]
async fn busy_retries_of_two_readers_are_spread() {
    let first = TestPace::new(1);
    let second = TestPace::new(2);
    let patience = BUSY_PAUSE * 8;
    assert!(always_busy(&first, patience, Duration::ZERO).await.is_err());
    assert!(always_busy(&second, patience, Duration::ZERO)
        .await
        .is_err());

    let (first, second) = (first.slept(), second.slept());
    assert!(
        first.len() >= 8 && second.len() >= 8,
        "{first:?} {second:?}"
    );
    for pause in first.iter().chain(&second) {
        assert!(
            *pause >= BUSY_PAUSE / 2 && *pause <= BUSY_PAUSE,
            "{pause:?}"
        );
    }
    assert!(
        first.iter().zip(&second).all(|(one, other)| one != other),
        "{first:?} {second:?}"
    );
}

/// A node that stays busy is asked again only while the read's patience
/// lasts: no ask starts past it, the pauses sum to less than it, and the
/// node's last answer is the read's error.
#[nuclei::test]
async fn busy_retries_stay_within_patience() {
    let took = Duration::from_millis(10);
    let pace = TestPace::new(3);
    let outcome = always_busy(&pace, BUSY_PATIENCE, took).await;

    assert!(super::busy(outcome.unwrap_err().as_ref()));
    let asked_at = pace.asked_at();
    assert!(
        asked_at.iter().all(|at| *at < BUSY_PATIENCE),
        "{asked_at:?}"
    );
    assert!(pace.slept().iter().sum::<Duration>() < BUSY_PATIENCE);
    assert!(pace.elapsed() < BUSY_PATIENCE + took);
    assert!(pace.elapsed() + BUSY_PAUSE >= BUSY_PATIENCE);
    let fewest = (BUSY_PATIENCE.as_millis() / (BUSY_PAUSE + took).as_millis()) as usize;
    assert!(asked_at.len() >= fewest, "{}", asked_at.len());
}

/// A node that takes its whole deadline to answer busy spends the read's
/// patience in far fewer asks: the requests' own time counts, and the read
/// ends with the busy answer before a pause could carry it past patience.
#[nuclei::test]
async fn a_slow_busy_node_spends_patience_early() {
    let slow = TestPace::new(4);
    let outcome = always_busy(&slow, BUSY_PATIENCE, NODE_DEADLINE).await;
    assert!(super::busy(outcome.unwrap_err().as_ref()));

    let asked_at = slow.asked_at();
    assert!((13..=15).contains(&asked_at.len()), "{asked_at:?}");
    assert!(
        asked_at.iter().all(|at| *at < BUSY_PATIENCE),
        "{asked_at:?}"
    );
    assert!(slow.elapsed() < BUSY_PATIENCE + NODE_DEADLINE);

    let fast = TestPace::new(4);
    assert!(always_busy(&fast, BUSY_PATIENCE, Duration::ZERO)
        .await
        .is_err());
    assert!(asked_at.len() * 8 < fast.asked_at().len());
}

/// The two kinds of asking again are counted apart: late answers do not use
/// an ask an overtaken one would need, nor the other way round.
#[nuclei::test]
async fn late_and_overtaken_answers_are_counted_apart() {
    let pace = TestPace::new(5);
    let asked = Cell::new(0);
    let outcome = verified_within(PATIENT, &pace, || {
        asked.set(asked.get() + 1);
        let ask = asked.get();
        async move {
            match ask {
                ask if ask <= LATE_ANSWERS => Err(late()),
                ask if ask < LATE_ANSWERS + ATTEMPTS => Err(refused(REFUSAL)),
                ask => Ok(ask),
            }
        }
    })
    .await;
    assert_eq!(answered(outcome), Some(LATE_ANSWERS + ATTEMPTS));
}

/// A read refused for a stale revision waits for the next one and is asked
/// again when it arrives within the patience.
#[nuclei::test]
async fn a_read_at_a_stale_revision_waits_for_the_next_and_is_answered() {
    let pace = TestPace::new(4);
    pace.fresh_at.set(Some(Duration::from_secs(12)));
    let asked = Cell::new(0);
    let outcome = verified_within(BUSY_PATIENCE, &pace, || {
        asked.set(asked.get() + 1);
        let ask = asked.get();
        pace.ask(Duration::from_millis(10));
        async move {
            if ask == 1 {
                Err(refused(STALE))
            } else {
                Ok(ask)
            }
        }
    })
    .await;
    assert_eq!(answered(outcome), Some(2));
    assert_eq!(pace.waited.borrow().len(), 1);
    assert_eq!(pace.asked_at()[1], Duration::from_secs(12));
}

/// When no fresh revision arrives within the patience the stale refusal is
/// the read's outcome: nothing is answered at the stale revision.
#[nuclei::test]
async fn a_read_at_a_stale_revision_fails_once_the_patience_is_spent() {
    let pace = TestPace::new(5);
    let asked = Cell::new(0);
    let outcome: Result<usize, Failure> = verified_within(BUSY_PATIENCE, &pace, || {
        asked.set(asked.get() + 1);
        pace.ask(Duration::from_millis(10));
        async { Err(refused(STALE)) }
    })
    .await;
    assert_eq!(outcome.unwrap_err().to_string(), STALE);
    assert_eq!(asked.get(), 1, "asked once, then waited out");
    assert_eq!(pace.elapsed(), BUSY_PATIENCE);
}
