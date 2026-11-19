//! Light-client reads the chain can overtake, or answer late.
//!
//! The light client checks every answer at the newest state root it has
//! authenticated. A block finalized while an answer is on its way back makes
//! it refuse that answer ("ACP state changed during verification; retry the
//! request"), although nothing failed: the same read asked again is answered
//! at the newer root. Every proven read goes through [`verified`], which asks
//! again, at most [`ATTEMPTS`] times (`proofs/tla/VerifiedRead.tla`).
//!
//! A Vera node also answers a proven read with its busy code, `-32002`, when
//! the certificate of the revision the read asks for is not ready within the
//! node's evidence deadline, or when its proof slots are taken. The evidence
//! is late, not absent: the same read asked a moment later is answered. Such
//! a read is asked again after a pause for as long as [`BUSY_PATIENCE`] since
//! its first ask, as the submission client asks again on the same code. Each
//! pause is drawn between half of [`BUSY_PAUSE`] and all of it, 250 ms on
//! average, so readers
//! the node turned away together do not come back together and keep its
//! proof slots taken. The patience counts the requests' own time too: a node
//! that takes its full deadline to answer busy spends it in fewer asks. An
//! idle single-validator deployment finalizes a block only every ten
//! seconds, so its evidence can be that late.

use std::error::Error;
use std::fmt::Display;
use std::future::Future;
use std::time::{Duration, Instant};

use acp_light_client::TransportError;
use inputs::Jitter;
use regolith::sync::Lazy;

/// How many times one read is asked. A block has to finalize during every
/// round trip for a read to use them all.
const ATTEMPTS: usize = 4;

/// How long, from its first ask, a read is asked again while the node
/// answers it is busy: the light client's freshness limit, past which the
/// state the read was asked at is no longer one it accepts.
const BUSY_PATIENCE: Duration = Duration::from_secs(30);

/// The longest wait before a read the node answered busy is asked again;
/// each wait is drawn between half of it and all of it. The waits average
/// 250 ms, so one reader asks a busy node as often as it did with a fixed
/// wait of that length, and only the readers' timing against each other
/// changes.
const BUSY_PAUSE: Duration = Duration::from_millis(333);

/// The JSON-RPC code a Vera node answers with when the evidence a read asks
/// for is not ready yet or its proof slots are taken.
const BUSY: i64 = -32002;

/// How the light client's refusal of an overtaken answer starts, both from
/// `HeaderChain::accept_response` and from
/// `AcpLightClient::verify_access_decision` (acp-light-client at backbone
/// a853cc6, whose errors carry no type to match). A bump of the light client
/// rechecks this wording.
const OVERTAKEN: &str = "ACP state changed during";

/// One jitter for every read in the process: reads that draw from it
/// together draw different pauses.
static JITTER: Lazy<Jitter> = Lazy::new(Jitter::random);

/// The clock and the pauses a read's busy retries are timed on.
trait Pace {
    /// The time since the read's first ask.
    fn elapsed(&self) -> Duration;

    /// The next pause before asking a busy node again.
    fn pause(&self) -> Duration;

    /// Waits `pause`.
    async fn sleep(&self, pause: Duration);
}

/// The wall clock, with pauses from [`JITTER`].
struct WallPace {
    started: Instant,
}

impl Pace for WallPace {
    fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    fn pause(&self) -> Duration {
        JITTER.pause(BUSY_PAUSE)
    }

    async fn sleep(&self, pause: Duration) {
        nuclei::time::sleep(pause).await;
    }
}

/// Runs `read` again while the chain overtakes its answer, at most
/// [`ATTEMPTS`] times in all, and while the node answers it is busy, for at
/// most [`BUSY_PATIENCE`]; any other outcome is returned as it is.
pub(super) async fn verified<T, E, F, Fut>(read: F) -> Result<T, E>
where
    E: AsRef<dyn Error + Send + Sync + 'static> + Display,
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    edge::compat(async {
        let pace = WallPace {
            started: Instant::now(),
        };
        verified_within(BUSY_PATIENCE, &pace, read).await
    })
    .await
}

/// [`verified`], timed on `pace`: a busy read is asked again only when the
/// drawn pause ends within `patience` of its first ask.
async fn verified_within<T, E, F, Fut, P>(patience: Duration, pace: &P, mut read: F) -> Result<T, E>
where
    E: AsRef<dyn Error + Send + Sync + 'static> + Display,
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    P: Pace,
{
    let mut asked = 1;
    loop {
        match read().await {
            Err(error) if asked < ATTEMPTS && overtaken(&error) => asked += 1,
            Err(error) if busy(error.as_ref()) => {
                let pause = pace.pause();
                if pace.elapsed() + pause >= patience {
                    return Err(error);
                }
                pace.sleep(pause).await;
            }
            outcome => return outcome,
        }
    }
}

fn overtaken<E>(error: &E) -> bool
where
    E: Display,
{
    error.to_string().starts_with(OVERTAKEN)
}

/// Whether the node answered with its busy code, anywhere in the error's
/// chain of causes.
fn busy(error: &(dyn Error + 'static)) -> bool {
    let mut link = Some(error);
    while let Some(error) = link {
        if let Some(TransportError::Rpc { code, .. }) = error.downcast_ref::<TransportError>() {
            return *code == BUSY;
        }
        link = error.source();
    }
    false
}

#[cfg(test)]
mod tests;
