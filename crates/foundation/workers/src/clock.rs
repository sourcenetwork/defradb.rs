//! The clock timers and the stuck-poll monitor read: the platform's monotonic
//! clock natively, a regolith `Env`'s clock where the caller has one, or a
//! clock the caller supplies (wasm has no default).

use std::fmt;
use std::sync::Arc;

/// Monotonic microseconds from an arbitrary origin.
pub trait Clock: Send + Sync + fmt::Debug {
    /// The current time.
    fn now_micros(&self) -> u64;
}

/// The platform's monotonic clock (`std::time::Instant`).
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
#[derive(Debug)]
pub struct StdClock {
    origin: std::time::Instant,
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
impl StdClock {
    /// A clock whose origin is now.
    pub fn new() -> StdClock {
        StdClock {
            origin: std::time::Instant::now(),
        }
    }
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
impl Default for StdClock {
    fn default() -> Self {
        StdClock::new()
    }
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
impl Clock for StdClock {
    fn now_micros(&self) -> u64 {
        u64::try_from(self.origin.elapsed().as_micros()).unwrap_or(u64::MAX)
    }
}

/// A regolith [`Env`](regolith::Env)'s monotonic clock, so timers and the
/// database measure time alike.
#[derive(Debug)]
pub struct EnvClock {
    env: Arc<dyn regolith::Env>,
}

impl EnvClock {
    /// `None` when `env` has no monotonic clock.
    pub fn new(env: Arc<dyn regolith::Env>) -> Option<EnvClock> {
        env.now_micros().map(|_| EnvClock { env })
    }
}

impl Clock for EnvClock {
    fn now_micros(&self) -> u64 {
        // `new` checked the env has a clock; an env does not lose it.
        self.env.now_micros().unwrap_or(0)
    }
}

/// The platform clock, where the target has one.
pub(crate) fn platform() -> Option<Arc<dyn Clock>> {
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    {
        Some(Arc::new(StdClock::new()))
    }
    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    {
        None
    }
}

/// A clock a test moves by hand.
#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct ManualClock(std::sync::atomic::AtomicU64);

#[cfg(test)]
impl ManualClock {
    pub(crate) fn advance(&self, micros: u64) {
        self.0
            .fetch_add(micros, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(test)]
impl Clock for ManualClock {
    fn now_micros(&self) -> u64 {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    #[test]
    fn the_std_clock_never_goes_back() {
        let clock = StdClock::new();
        let a = clock.now_micros();
        assert!(clock.now_micros() >= a);
    }

    #[test]
    fn an_env_clock_needs_an_env_with_a_clock() {
        let env = Arc::new(regolith::MemEnv::new());
        let clock = EnvClock::new(env.clone()).unwrap();
        env.advance_micros(5);
        assert_eq!(clock.now_micros(), 5);
        env.set_clocks(None, None);
        assert!(EnvClock::new(env).is_none());
    }
}
