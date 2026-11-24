//! Query guardrail defaults and configuration.

/// Default maximum nesting depth for GraphQL selection sets.
pub const DEFAULT_MAX_QUERY_DEPTH: usize = 20;

/// Default maximum number of fields at any single selection level.
pub const DEFAULT_MAX_QUERY_WIDTH: usize = 100;

/// Default maximum recursive depth for filter evaluation.
pub const DEFAULT_MAX_FILTER_DEPTH: usize = 50;

/// Default maximum number of fragments one spread chain nests.
///
/// Fragments flatten into the selection that spreads them, so a chain adds no
/// nesting depth, but the parser expands it one fragment at a time; a longer
/// chain is refused before it can exhaust a thread's stack (#2465).
pub const DEFAULT_MAX_FRAGMENT_NESTING: usize = 32;

/// Configurable limits applied while parsing and executing queries.
///
/// A value of `0` disables the depth, width and filter limits; the fragment
/// nesting limit is always on (see [`QueryLimits::check_fragment_nesting`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryLimits {
    pub max_query_depth: usize,
    pub max_query_width: usize,
    pub max_filter_depth: usize,
    pub max_fragment_nesting: usize,
}

impl Default for QueryLimits {
    fn default() -> Self {
        Self {
            max_query_depth: DEFAULT_MAX_QUERY_DEPTH,
            max_query_width: DEFAULT_MAX_QUERY_WIDTH,
            max_filter_depth: DEFAULT_MAX_FILTER_DEPTH,
            max_fragment_nesting: DEFAULT_MAX_FRAGMENT_NESTING,
        }
    }
}

/// Stack one level of fragment nesting costs the parser, measured on x86_64
/// Linux by parsing ever longer chains on fixed-size threads until one
/// overflowed (2026-10-31): 10.4 KiB a level in a debug build, 2.25 KiB in
/// release. Rounded up.
const FRAGMENT_LEVEL_STACK_BYTES: usize = if cfg!(debug_assertions) {
    12 * 1024
} else {
    3 * 1024
};

/// The default stack of a spawned thread, std's and tokio's alike.
const SPAWNED_THREAD_STACK_BYTES: usize = 2 * 1024 * 1024;

/// The stack of the threads that parse queries. Database workers are
/// spawned with std's default, which `RUST_MIN_STACK` changes; the network
/// edge runs on tokio's, which it does not; a wasm module has its one
/// 1 MiB stack. A query may be parsed on any of them, so the smallest counts.
pub fn query_thread_stack_bytes() -> usize {
    if cfg!(target_arch = "wasm32") {
        return 1024 * 1024;
    }
    std::env::var("RUST_MIN_STACK")
        .ok()
        .and_then(|bytes| bytes.trim().parse::<usize>().ok())
        .unwrap_or(SPAWNED_THREAD_STACK_BYTES)
        .min(SPAWNED_THREAD_STACK_BYTES)
}

/// The deepest fragment nesting a `stack_bytes` thread parses safely: the
/// walk may use half the stack, leaving the rest to the request around it.
pub fn max_fragment_nesting_for_stack(stack_bytes: usize) -> usize {
    stack_bytes / 2 / FRAGMENT_LEVEL_STACK_BYTES
}

impl QueryLimits {
    /// Refuse a fragment nesting limit a query thread's stack cannot hold,
    /// or one of zero, which would refuse every fragment.
    pub fn check_fragment_nesting(&self) -> Result<(), FragmentNestingError> {
        let stack_bytes = query_thread_stack_bytes();
        let safe_max = max_fragment_nesting_for_stack(stack_bytes);
        match self.max_fragment_nesting {
            nesting if nesting >= 1 && nesting <= safe_max => Ok(()),
            configured => Err(FragmentNestingError {
                configured,
                stack_kib: stack_bytes / 1024,
                safe_max,
            }),
        }
    }
}

/// A fragment nesting limit outside what a query thread's stack holds.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "api.query_max_fragment_nesting (--query-max-fragment-nesting) is {configured}, but a query \
     thread's {stack_kib} KiB stack holds fragments nested at most {safe_max} deep; set it from \
     1 to {safe_max}"
)]
pub struct FragmentNestingError {
    pub configured: usize,
    pub stack_kib: usize,
    pub safe_max: usize,
}
