//! Query structure limits to prevent denial-of-service via deeply nested or wide queries.

use rapidhash::{HashSetExt, RapidHashSet};

use crate::error::{QueryError, Result};
use crate::limits::{
    QueryLimits, DEFAULT_MAX_FRAGMENT_NESTING, DEFAULT_MAX_QUERY_DEPTH, DEFAULT_MAX_QUERY_WIDTH,
};
use crate::mapper::{Requestable, Select};

/// Maximum nesting depth for GraphQL selections.
///
/// A query like `{ A { B { C { ... } } } }` has depth equal to the number of
/// nested select levels. Queries deeper than this limit are rejected.
pub const MAX_QUERY_DEPTH: usize = DEFAULT_MAX_QUERY_DEPTH;

/// Maximum number of fields at any single selection level.
///
/// This caps the fan-out per level to prevent wide queries that would generate
/// an unbounded number of fetches.
pub const MAX_QUERY_WIDTH: usize = DEFAULT_MAX_QUERY_WIDTH;

/// Default maximum number of fragments one spread chain nests; the same bound
/// as a subscription's root fragments.
pub const MAX_FRAGMENT_NESTING: usize = DEFAULT_MAX_FRAGMENT_NESTING;

/// The fragments on the spread chain being expanded, bounded by
/// `QueryLimits::max_fragment_nesting` (`0` = unlimited).
pub(super) struct FragmentPath {
    names: RapidHashSet<String>,
    max_nesting: usize,
}

impl FragmentPath {
    pub(super) fn new(limits: QueryLimits) -> Self {
        Self {
            names: RapidHashSet::new(),
            max_nesting: limits.max_fragment_nesting,
        }
    }

    pub(super) fn contains(&self, name: &str) -> bool {
        self.names.contains(name)
    }

    pub(super) fn insert(&mut self, name: String) -> bool {
        self.names.insert(name)
    }

    pub(super) fn remove(&mut self, name: &str) {
        self.names.remove(name);
    }

    /// Refuse a spread that would nest more fragments than the limit, given
    /// the fragments already on the chain.
    pub(super) fn check_nesting(&self) -> Result<()> {
        if self.max_nesting > 0 && self.names.len() >= self.max_nesting {
            return Err(QueryError::parse(format!(
                "query nests fragments more than {} deep; raise the node's \
                 api.query_max_fragment_nesting (--query-max-fragment-nesting) to allow it",
                self.max_nesting
            )));
        }
        Ok(())
    }
}

/// Validate a parsed Select tree against depth and width limits.
///
/// Called after parsing to reject queries that would cause excessive work.
pub fn validate_select_limits(select: &Select) -> Result<()> {
    validate_select_limits_with(select, QueryLimits::default())
}

/// Validate a parsed Select tree against custom depth and width limits.
pub fn validate_select_limits_with(select: &Select, limits: QueryLimits) -> Result<()> {
    validate_select_at_depth(select, 1, limits)
}

/// Validate a parsed requestable field list against custom depth and width limits.
pub fn validate_requestable_limits_with(
    requestables: &[Requestable],
    limits: QueryLimits,
) -> Result<()> {
    validate_requestables_at_depth(requestables, 1, limits)
}

fn validate_select_at_depth(select: &Select, depth: usize, limits: QueryLimits) -> Result<()> {
    if limits.max_query_depth > 0 && depth > limits.max_query_depth {
        return Err(QueryError::parse(format!(
            "query exceeds maximum nesting depth of {}",
            limits.max_query_depth
        )));
    }

    validate_requestables_at_depth(&select.fields, depth, limits)
}

fn validate_requestables_at_depth(
    requestables: &[Requestable],
    depth: usize,
    limits: QueryLimits,
) -> Result<()> {
    if limits.max_query_width > 0 && requestables.len() > limits.max_query_width {
        return Err(QueryError::parse(format!(
            "query exceeds maximum field width of {} at depth {}",
            limits.max_query_width, depth
        )));
    }

    for field in requestables {
        if let Requestable::Select(nested) = field {
            validate_select_at_depth(nested, depth + 1, limits)?;
        }
    }

    Ok(())
}
