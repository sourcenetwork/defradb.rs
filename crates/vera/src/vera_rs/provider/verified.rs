//! Light-client reads the chain can overtake.
//!
//! The light client checks every answer at the newest state root it has
//! authenticated. A block finalized while an answer is on its way back makes
//! it refuse that answer ("ACP state changed during verification; retry the
//! request"), although nothing failed: the same read asked again is answered
//! at the newer root. Every proven read goes through [`verified`], which asks
//! again, at most [`ATTEMPTS`] times (`proofs/tla/VerifiedRead.tla`).

use std::fmt::Display;
use std::future::Future;

/// How many times one read is asked. A block has to finalize during every
/// round trip for a read to use them all.
const ATTEMPTS: usize = 4;

/// How the light client's refusal of an overtaken answer starts, both from
/// `HeaderChain::accept_response` and from
/// `AcpLightClient::verify_access_decision` (acp-light-client at backbone
/// 2720b61, whose errors carry no type to match). A bump of the light client
/// rechecks this wording.
const OVERTAKEN: &str = "ACP state changed during";

/// Runs `read` again while the chain overtakes its answer, at most
/// [`ATTEMPTS`] times in all; any other outcome is returned as it is.
pub(super) async fn verified<T, E, F, Fut>(mut read: F) -> Result<T, E>
where
    E: Display,
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    edge::compat(async {
        let mut asked = 1;
        loop {
            match read().await {
                Err(error) if asked < ATTEMPTS && overtaken(&error) => asked += 1,
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

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::{verified, ATTEMPTS};

    const REFUSAL: &str = "ACP state changed during verification; retry the request";
    const DECISION_REFUSAL: &str =
        "ACP state changed during decision verification; retry the request";

    #[nuclei::test]
    async fn a_read_the_chain_overtook_is_asked_again_until_answered() {
        let asked = Cell::new(0);
        let outcome = verified(|| {
            asked.set(asked.get() + 1);
            let ask = asked.get();
            async move {
                if ask < ATTEMPTS {
                    Err(REFUSAL)
                } else {
                    Ok(ask)
                }
            }
        })
        .await;
        assert_eq!(outcome, Ok(ATTEMPTS));
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
                    Err(DECISION_REFUSAL)
                } else {
                    Ok(ask)
                }
            }
        })
        .await;
        assert_eq!(outcome, Ok(2));
    }

    #[nuclei::test]
    async fn a_read_overtaken_at_every_ask_returns_the_refusal_after_the_last() {
        let asked = Cell::new(0);
        let outcome: Result<(), _> = verified(|| {
            asked.set(asked.get() + 1);
            async { Err(REFUSAL) }
        })
        .await;
        assert_eq!(outcome, Err(REFUSAL));
        assert_eq!(asked.get(), ATTEMPTS);
    }

    #[nuclei::test]
    async fn any_other_error_is_returned_at_the_first_ask() {
        let asked = Cell::new(0);
        let outcome: Result<(), _> = verified(|| {
            asked.set(asked.get() + 1);
            async { Err("no verified finalized state available") }
        })
        .await;
        assert_eq!(outcome, Err("no verified finalized state available"));
        assert_eq!(asked.get(), 1);
    }
}
