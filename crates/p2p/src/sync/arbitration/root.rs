//! Per-root estimates, kept in the root's pending entry and updated under the
//! registry's copy-on-write CAS: how long its attempts run and whether they
//! advance it. Per-sample, weight ½, the first attempt taken as is.

use std::time::Duration;

use super::{roll, ROOT_SHIFT};

/// A Q16 share of one: the attempt made no progress.
const NO_PROGRESS: u64 = u16::MAX as u64;

/// What one clock-dispatched fetch attempt tells its root's estimators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttemptSample {
    /// No provider was asked, or the attempt ended without a verdict on the
    /// root (deferred, abandoned, unservable).
    Skipped,
    Complete,
    /// Every provider that answered shed the request, and none delivered.
    Shed,
    /// Still making progress at its quantum. Its duration is censored at the
    /// quantum, so it never feeds the attempt-duration estimate.
    Preempted,
    Incomplete {
        duration: Duration,
        /// The attempt ended within `FAST_REFUSAL`: a refusal, not a measure
        /// of the root's work.
        fast: bool,
        /// Some provider delivered blocks for the root.
        progressed: bool,
    },
}

/// A pending root's estimates; `None` until an attempt fed one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RootEstimates {
    /// Duration of the root's slow, unshed, incomplete attempts. Shed and
    /// fast-refused attempts are load signals and never feed it.
    pub attempt_ms: Option<u32>,
    /// Whether attempts leave the root's missing frontier as it was: zero for
    /// one that advanced it, `u16::MAX` for one that did not. Every attempt
    /// with a verdict feeds it, shed ones included.
    pub no_progress_q16: Option<u16>,
}

impl RootEstimates {
    pub(crate) fn observe(&mut self, sample: AttemptSample) {
        match sample {
            AttemptSample::Skipped => {}
            AttemptSample::Complete | AttemptSample::Preempted => self.no_progress(0),
            AttemptSample::Shed => self.no_progress(NO_PROGRESS),
            AttemptSample::Incomplete {
                duration,
                fast,
                progressed,
            } => {
                self.no_progress(if progressed { 0 } else { NO_PROGRESS });
                if !fast {
                    let ms = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
                    let ms = ms.min(u64::from(u32::MAX));
                    let next = self
                        .attempt_ms
                        .map_or(ms, |avg| roll(u64::from(avg), ms, ROOT_SHIFT));
                    self.attempt_ms = Some(u32::try_from(next).unwrap_or(u32::MAX));
                }
            }
        }
    }

    fn no_progress(&mut self, sample: u64) {
        let next = self
            .no_progress_q16
            .map_or(sample, |avg| roll(u64::from(avg), sample, ROOT_SHIFT));
        self.no_progress_q16 = Some(u16::try_from(next).unwrap_or(u16::MAX));
    }

    /// `no_progress_q16` in parts per million.
    pub fn no_progress_ppm(&self) -> Option<u64> {
        self.no_progress_q16
            .map(|q16| u64::from(q16) * super::SCALE_PPM / NO_PROGRESS)
    }
}
