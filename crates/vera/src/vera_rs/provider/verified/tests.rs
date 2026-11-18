use std::cell::Cell;
use std::error::Error;
use std::time::{Duration, Instant};

use acp_light_client::TransportError;

use super::{verified, verified_within, ATTEMPTS, BUSY_PAUSE};

/// More than the late answers below take to give way.
const PATIENT: Duration = Duration::from_secs(10);
/// Late answers that never give way.
const LATE_ANSWERS: usize = 4;

type Failure = Box<dyn Error + Send + Sync>;

const REFUSAL: &str = "ACP state changed during verification; retry the request";
const DECISION_REFUSAL: &str = "ACP state changed during decision verification; retry the request";

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

#[nuclei::test]
async fn a_read_the_chain_overtook_is_asked_again_until_answered() {
    let asked = Cell::new(0);
    let outcome = verified(|| {
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
    let outcome = verified(|| {
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
    let outcome: Result<(), _> = verified(|| {
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
    let outcome: Result<(), _> = verified(|| {
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
    let outcome: Result<(), _> = verified(|| {
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

/// Evidence the node could not give in time is asked for again, and the
/// read ends with the answer that follows.
#[nuclei::test]
async fn a_read_the_node_answered_late_is_asked_again_until_answered() {
    let asked = Cell::new(0);
    let outcome = verified_within(PATIENT, || {
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
}

/// A node that stays busy is asked again only while the read's patience
/// lasts, and its last answer is the read's error.
#[nuclei::test]
async fn a_read_the_node_answers_late_every_time_returns_once_its_patience_is_spent() {
    let patience = BUSY_PAUSE * 4;
    let asked = Cell::new(0);
    let started = Instant::now();
    let outcome: Result<(), _> = verified_within(patience, || {
        asked.set(asked.get() + 1);
        async { Err(late()) }
    })
    .await;
    assert!(super::busy(outcome.unwrap_err().as_ref()));
    assert!(started.elapsed() >= patience - BUSY_PAUSE);
    assert!(started.elapsed() < patience + BUSY_PAUSE);
    assert!((2..=4).contains(&asked.get()), "{}", asked.get());
}

/// The two kinds of asking again are counted apart: late answers do not use
/// an ask an overtaken one would need, nor the other way round.
#[nuclei::test]
async fn late_and_overtaken_answers_are_counted_apart() {
    let asked = Cell::new(0);
    let outcome = verified_within(PATIENT, || {
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
