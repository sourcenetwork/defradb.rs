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
//! a read is asked again after [`BUSY_PAUSE`] for as long as
//! [`BUSY_PATIENCE`] since its first ask, as the submission client asks
//! again on the same code. An idle single-validator deployment finalizes a
//! block only every ten seconds, so its evidence can be that late.

use std::error::Error;
use std::fmt::Display;
use std::future::Future;
use std::time::{Duration, Instant};

use acp_light_client::TransportError;

/// How many times one read is asked. A block has to finalize during every
/// round trip for a read to use them all.
const ATTEMPTS: usize = 4;

/// How long, from its first ask, a read is asked again while the node
/// answers it is busy: the light client's freshness limit, past which the
/// state the read was asked at is no longer one it accepts.
const BUSY_PATIENCE: Duration = Duration::from_secs(30);

/// The wait before a read the node answered busy is asked again.
const BUSY_PAUSE: Duration = Duration::from_millis(250);

/// The JSON-RPC code a Vera node answers with when the evidence a read asks
/// for is not ready yet or its proof slots are taken.
const BUSY: i64 = -32002;

/// How the light client's refusal of an overtaken answer starts, both from
/// `HeaderChain::accept_response` and from
/// `AcpLightClient::verify_access_decision` (acp-light-client at backbone
/// 502e604, whose errors carry no type to match). A bump of the light client
/// rechecks this wording.
const OVERTAKEN: &str = "ACP state changed during";

/// Runs `read` again while the chain overtakes its answer, at most
/// [`ATTEMPTS`] times in all, and while the node answers it is busy, for at
/// most [`BUSY_PATIENCE`]; any other outcome is returned as it is.
pub(super) async fn verified<T, E, F, Fut>(read: F) -> Result<T, E>
where
    E: AsRef<dyn Error + Send + Sync + 'static> + Display,
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    verified_within(BUSY_PATIENCE, read).await
}

/// [`verified`], asking again while busy for at most `patience`.
async fn verified_within<T, E, F, Fut>(patience: Duration, mut read: F) -> Result<T, E>
where
    E: AsRef<dyn Error + Send + Sync + 'static> + Display,
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    edge::compat(async {
        let started = Instant::now();
        let mut asked = 1;
        loop {
            match read().await {
                Err(error) if asked < ATTEMPTS && overtaken(&error) => asked += 1,
                Err(error) if started.elapsed() + BUSY_PAUSE < patience && busy(error.as_ref()) => {
                    nuclei::time::sleep(BUSY_PAUSE).await;
                }
                outcome => return outcome,
            }
        }
    })
    .await
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
