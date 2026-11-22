use std::convert::Infallible;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use defra_core::thread_bounds::MaybeBoxFuture;
use regolith::sync::SegQueue;

use super::{until_receipt, Fallback, Heads, Start};

/// Longer than any test runs: a loop that waits it out never finishes.
const HOUR: Duration = Duration::from_secs(3600);
/// How long a test may take before it counts as stuck.
const STUCK: Duration = Duration::from_secs(10);

#[derive(Debug, PartialEq, Eq)]
enum Step {
    Ask,
    Send,
    Wait { seen: u64, bound: Duration },
}

/// A light client whose headers a test controls, logging what the loop does.
struct Script {
    height: AtomicU64,
    /// Whether a wait that begins at the newest height sees the next header
    /// verified, or waits out its bound.
    next_header: bool,
    steps: SegQueue<Step>,
}

impl Script {
    fn new(height: u64, next_header: bool) -> Self {
        Self {
            height: AtomicU64::new(height),
            next_header,
            steps: SegQueue::new(),
        }
    }

    fn steps(&self) -> Vec<Step> {
        std::iter::from_fn(|| self.steps.pop()).collect()
    }
}

impl Heads for Script {
    fn height(&self) -> u64 {
        self.height.load(Ordering::SeqCst)
    }

    fn above(&self, seen: u64, bound: Duration) -> MaybeBoxFuture<'_, bool> {
        self.steps.push(Step::Wait { seen, bound });
        Box::pin(async move {
            if self.height() > seen {
                return true;
            }
            if self.next_header {
                self.height.store(seen + 1, Ordering::SeqCst);
                return true;
            }
            nuclei::time::sleep(bound).await;
            false
        })
    }
}

#[nuclei::test]
async fn a_receipt_is_asked_for_at_the_next_verified_height_and_not_after_a_sleep() {
    let script = Script::new(5, true);
    let heads = &script;
    let fallback = Fallback::new(HOUR);
    let receipt = nuclei::time::timeout(
        STUCK,
        until_receipt(
            heads,
            &fallback,
            Start::Send,
            move || async move {
                heads.steps.push(Step::Ask);
                Ok::<_, Infallible>((heads.height() >= 6).then_some("receipt"))
            },
            move || async move {
                heads.steps.push(Step::Send);
                Ok(true)
            },
        ),
    )
    .await
    .expect("the loop asked at the next header, not after its fallback");
    assert_eq!(receipt, Ok("receipt"));
    assert_eq!(fallback.expired(), 0);
    assert_eq!(
        script.steps(),
        [
            Step::Send,
            Step::Wait {
                seen: 5,
                bound: HOUR
            },
            Step::Ask,
        ]
    );
}

#[nuclei::test]
async fn a_header_verified_while_the_request_is_sent_still_ends_the_wait() {
    let script = Script::new(5, false);
    let heads = &script;
    let fallback = Fallback::new(HOUR);
    let receipt = nuclei::time::timeout(
        STUCK,
        until_receipt(
            heads,
            &fallback,
            Start::Send,
            move || async move {
                heads.steps.push(Step::Ask);
                Ok::<_, Infallible>((heads.height() >= 6).then_some("receipt"))
            },
            move || async move {
                heads.steps.push(Step::Send);
                // The request's block finalizes and its header is verified
                // before the send returns.
                heads.height.store(6, Ordering::SeqCst);
                Ok(true)
            },
        ),
    )
    .await
    .expect("a header verified during the send ended the wait");
    assert_eq!(receipt, Ok("receipt"));
    assert_eq!(fallback.expired(), 0);
    assert_eq!(
        script.steps(),
        [
            Step::Send,
            Step::Wait {
                seen: 5,
                bound: HOUR
            },
            Step::Ask,
        ]
    );
}

#[nuclei::test]
async fn with_no_header_each_wait_ends_at_its_fallback_and_an_unheard_send_is_sent_again() {
    let script = Script::new(5, false);
    let heads = &script;
    let bound = Duration::from_millis(20);
    let fallback = Fallback::new(bound);
    let asks = &AtomicU64::new(0);
    let sends = &AtomicU64::new(0);
    let started = std::time::Instant::now();
    let receipt = nuclei::time::timeout(
        STUCK,
        until_receipt(
            heads,
            &fallback,
            Start::Ask,
            move || async move {
                heads.steps.push(Step::Ask);
                let asked = asks.fetch_add(1, Ordering::SeqCst) + 1;
                Ok::<_, Infallible>((asked == 3).then_some("receipt"))
            },
            move || async move {
                heads.steps.push(Step::Send);
                Ok(sends.fetch_add(1, Ordering::SeqCst) > 0)
            },
        ),
    )
    .await
    .expect("each wait ended at its fallback");
    assert_eq!(receipt, Ok("receipt"));
    assert!(started.elapsed() >= 2 * bound);
    assert_eq!(fallback.expired(), 2);
    let wait = || Step::Wait { seen: 5, bound };
    assert_eq!(
        script.steps(),
        [
            Step::Ask,
            Step::Send,
            wait(),
            Step::Ask,
            Step::Send,
            wait(),
            Step::Ask,
        ]
    );
}
