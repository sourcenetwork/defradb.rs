//! The network runtime a process starts the first time something needs
//! network I/O and was given no [`Edge`]: started once, shared by every
//! later caller, kept until the process exits. A program that never reaches
//! the network starts no threads, and a component given an `Edge` never
//! starts this one.

use std::num::NonZeroUsize;

use regolith::sync::OnceCell;

use crate::{Edge, EdgeConfig, EdgeError, EdgeRuntime};

/// Threads of the on-demand runtime: network I/O waits, it does not compute.
const THREADS: usize = 2;

static RUNTIME: OnceCell<EdgeRuntime> = OnceCell::new();

/// The process's on-demand network runtime, started on first use. Never
/// waits: callers racing the first start each start one, one is kept and
/// the others stop at once. A start that fails is returned, and the next
/// caller tries again.
pub fn on_demand() -> Result<&'static Edge, EdgeError> {
    if let Some(runtime) = RUNTIME.get() {
        return Ok(runtime.edge());
    }
    let started = EdgeRuntime::start(EdgeConfig {
        threads: NonZeroUsize::new(THREADS),
        thread_name: "defra-edge-on-demand".to_string(),
        ..EdgeConfig::default()
    })?;
    // A racing start that published first wins; this one is dropped, which
    // stops it.
    Ok(RUNTIME.get_or_init_racy(|| started).edge())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::on_demand;

    #[test]
    fn the_on_demand_runtime_starts_once_and_is_shared() {
        let (first, second) = (on_demand().unwrap(), on_demand().unwrap());
        assert!(Arc::ptr_eq(&first.shared, &second.shared));
        let answer = futures_lite::future::block_on(first.spawn(async { 6 * 7 })).unwrap();
        assert_eq!(answer, 42);
    }
}
