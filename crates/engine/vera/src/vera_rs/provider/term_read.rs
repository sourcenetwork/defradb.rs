//! A read answered at one checked term.
//!
//! The provider learns that its deployment switched its active device from
//! the first revision of the new term its light client verifies, and that
//! revision may be the one a read is answered at: the read was admitted
//! under the term before, so its answer is not served (`Follow::still`).
//! Nothing is wrong with the deployment; the term only has to be checked.
//! Such a read is asked again: admission checks the term on the caller's
//! task first, and the answer served is one admitted and answered under the
//! same checked term: each ask is one `Decide` of
//! `proofs/tla/FollowDeployment.tla`, which reads only while the gate is
//! open.

use std::future::Future;

use crate::provider::ProviderError;

/// How often a read is asked again after a term overtook it. A term starts
/// only when the account's operators certify one, so a second one arriving
/// within the same read is refused like any unchecked term.
const ASKED_AGAIN: usize = 1;

/// The answer of `read`, admitted by `admitted` before it and unchanged in
/// `still` after it. A read a term overtook is asked again, at most
/// [`ASKED_AGAIN`] times; every other failure is returned as it is.
pub(super) async fn at_one_term<T, A, AF, R, RF>(
    admitted: A,
    still: impl Fn(u64) -> Result<(), ProviderError>,
    read: R,
) -> Result<T, ProviderError>
where
    A: Fn() -> AF,
    AF: Future<Output = Result<u64, ProviderError>>,
    R: Fn() -> RF,
    RF: Future<Output = Result<T, ProviderError>>,
{
    let mut asked = 0;
    loop {
        let epoch = admitted().await?;
        let answer = read().await?;
        match still(epoch) {
            Ok(()) => return Ok(answer),
            Err(error) if asked == ASKED_AGAIN => return Err(error),
            Err(_) => asked += 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

    use futures::executor::block_on;

    use super::*;

    fn unchecked() -> ProviderError {
        ProviderError::Unavailable("a term is being checked".into())
    }

    /// A provider whose light client certifies a term during each of the
    /// first `overtaken` reads, and whose admission checks it.
    struct Terms {
        certified: AtomicU64,
        handled: AtomicU64,
        overtaken: usize,
        reads: AtomicUsize,
    }

    impl Terms {
        fn overtaken(overtaken: usize) -> Self {
            Self {
                certified: AtomicU64::new(0),
                handled: AtomicU64::new(0),
                overtaken,
                reads: AtomicUsize::new(0),
            }
        }

        async fn ask(&self) -> Result<u64, ProviderError> {
            at_one_term(
                || async {
                    let certified = self.certified.load(Ordering::Relaxed);
                    self.handled.store(certified, Ordering::Relaxed);
                    Ok(certified)
                },
                |epoch| {
                    let certified = self.certified.load(Ordering::Relaxed);
                    let checked = self.handled.load(Ordering::Relaxed) == certified;
                    (checked && epoch == certified)
                        .then_some(())
                        .ok_or_else(unchecked)
                },
                || async {
                    if self.reads.fetch_add(1, Ordering::Relaxed) < self.overtaken {
                        self.certified.fetch_add(1, Ordering::Relaxed);
                    }
                    Ok(self.handled.load(Ordering::Relaxed))
                },
            )
            .await
        }
    }

    #[test]
    fn a_read_no_term_overtook_is_answered_once() {
        let terms = Terms::overtaken(0);
        assert_eq!(block_on(terms.ask()).unwrap(), 0);
        assert_eq!(terms.reads.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn a_read_a_term_overtook_is_asked_again_at_the_checked_term() {
        let terms = Terms::overtaken(1);
        assert_eq!(
            block_on(terms.ask()).unwrap(),
            1,
            "the answer served is the one read under the checked term"
        );
        assert_eq!(terms.reads.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn a_read_overtaken_at_every_ask_ends_in_the_refusal() {
        let terms = Terms::overtaken(usize::MAX);
        let refused = block_on(terms.ask()).unwrap_err();
        assert_eq!(refused.to_string(), unchecked().to_string());
        assert_eq!(terms.reads.load(Ordering::Relaxed), 1 + ASKED_AGAIN);
    }

    #[test]
    fn a_failed_read_and_a_refused_admission_are_returned_as_they_are() {
        let failed: Result<(), _> = block_on(at_one_term(
            || async { Ok(0) },
            |_| Ok(()),
            || async { Err(ProviderError::Query("no proof".into())) },
        ));
        assert!(matches!(failed, Err(ProviderError::Query(_))));
        let asked = AtomicUsize::new(0);
        let refused: Result<(), _> = block_on(at_one_term(
            || async { Err(unchecked()) },
            |_| Ok(()),
            || async {
                asked.fetch_add(1, Ordering::Relaxed);
                Ok(())
            },
        ));
        assert!(matches!(refused, Err(ProviderError::Unavailable(_))));
        assert_eq!(
            asked.load(Ordering::Relaxed),
            0,
            "nothing is read unadmitted"
        );
    }
}
