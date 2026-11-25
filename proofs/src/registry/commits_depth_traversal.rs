use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "query, db (#2098)";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Commits depth traversal",
        name: "minus_one_is_unlimited / other_negative_refused / nonnegative_is_itself / minus_one_clears_cap / walk_eq_fresh / walk_mem / walk_nodup / walk_unlimited / go_scan_merged / push_time_differs — `_commits(depth: -1)` reads as u64::MAX and so walks with no limit, every other negative depth is refused, and a non-negative one is itself; the stack walk below a cid, deciding each commit at pop time against the most depth it was walked with, lists the last-head-first path walk at first occurrence, each commit once, the same set as the old forward walk, and for an unlimited walk in Go's DAG-scan order, which marking at push time breaks, and keeps a commit only a shorter path reaches, which a plain seen set loses; a commit walked with at least UNLIMITED_DEPTH (2^62) left is never walked again, which -1 clears, so an unlimited walk loads each block once; every block is assumed to load",
        axis: Lean,
        anchor: "crates/database/db/src/read/commits/mod.rs CommitsFetcher::fetch_commit_by_cid, CommitsFetcher::traverse_depth, CommitsFetcher::push_heads; crates/engine/query/src/query_parse/parser.rs parse_field_to_select",
        model_ref: "CommitsDepthCast.minus_one_is_unlimited / other_negative_refused / nonnegative_is_itself / minus_one_clears_cap / run_proc / walk_eq_fresh / walk_mem / walk_nodup / walk_unlimited / go_scan_merged / walk_merged / go_scan_diamond / push_time_differs / walk_short_long / seen_only_loses_a_commit / ladder_loads_once / no_guard_loads_again",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/read/commits_depth_walk.rs commits_depth_walk_lists_each_commit_of_a_diamond_chain_once_last_head_first, commits_depth_walk_keeps_rows_a_shorter_path_reaches, commits_depth_walk_lists_a_long_history_at_unlimited_depth, commits_depth_walk_reads_each_block_once_at_unlimited_depth; crates/engine/query/tests/query_parse_tests.rs test_parse_commits_depth_minus_one_is_unlimited, test_parse_commits_depth_other_negative_returns_error",
    },
];
