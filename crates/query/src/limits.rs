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
/// A value of `0` disables that individual limit.
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
