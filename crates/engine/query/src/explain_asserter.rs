//! Path-based assertions over `explain(type: execute)` output.

use std::ops::{Bound, Range, RangeBounds, RangeInclusive};

use serde_json::Value;

/// An exact count or an acceptable range of counts.
#[derive(Clone, Debug)]
pub enum ExpectedCount {
    Exact(u64),
    Range(Range<u64>),
    RangeInclusive(RangeInclusive<u64>),
    AtMost(u64),
}

impl ExpectedCount {
    pub fn exact(value: u64) -> Self {
        Self::Exact(value)
    }

    pub fn between(start: u64, end: u64) -> Self {
        Self::RangeInclusive(start..=end)
    }

    pub fn at_most(value: u64) -> Self {
        Self::AtMost(value)
    }

    fn bounds(&self) -> (Bound<u64>, Bound<u64>) {
        match self {
            Self::Exact(value) => (Bound::Included(*value), Bound::Included(*value)),
            Self::Range(range) => (Bound::Included(range.start), Bound::Excluded(range.end)),
            Self::RangeInclusive(range) => (
                Bound::Included(*range.start()),
                Bound::Included(*range.end()),
            ),
            Self::AtMost(value) => (Bound::Unbounded, Bound::Included(*value)),
        }
    }
}

impl From<u64> for ExpectedCount {
    fn from(value: u64) -> Self {
        Self::Exact(value)
    }
}

impl From<Range<u64>> for ExpectedCount {
    fn from(range: Range<u64>) -> Self {
        Self::Range(range)
    }
}

impl From<RangeInclusive<u64>> for ExpectedCount {
    fn from(range: RangeInclusive<u64>) -> Self {
        Self::RangeInclusive(range)
    }
}

/// Asserts metrics at a named level of an explain tree, then chains deeper.
pub struct ExplainAsserter<'a> {
    root: &'a Value,
    path: Vec<String>,
}

impl<'a> ExplainAsserter<'a> {
    pub fn new(root: &'a Value) -> Self {
        Self {
            root,
            path: Vec::new(),
        }
    }

    /// Moves to an absolute path. A numeric segment indexes an array; a named
    /// segment selects the first object child with that key.
    pub fn path(mut self, segments: &[&str]) -> Self {
        self.path = segments
            .iter()
            .map(|segment| (*segment).to_string())
            .collect();
        self.exists()
    }

    /// Moves one level deeper from the current path.
    pub fn enter(mut self, segment: &str) -> Self {
        self.path.push(segment.to_string());
        self.exists()
    }

    /// The current level, for shapes that carry no count of their own.
    pub fn node(&self) -> &'a Value {
        self.require_node()
    }

    /// Confirms the current level exists without reading a count from it.
    pub fn exists(self) -> Self {
        self.require_node();
        self
    }

    pub fn iterations(self, expected: impl Into<ExpectedCount>) -> Self {
        self.count("iterations", expected)
    }

    pub fn doc_fetches(self, expected: impl Into<ExpectedCount>) -> Self {
        self.count("docFetches", expected)
    }

    pub fn field_fetches(self, expected: impl Into<ExpectedCount>) -> Self {
        self.count("fieldFetches", expected)
    }

    pub fn index_fetches(self, expected: impl Into<ExpectedCount>) -> Self {
        self.count("indexFetches", expected)
    }

    pub fn filter_matches(self, expected: impl Into<ExpectedCount>) -> Self {
        self.count("filterMatches", expected)
    }

    pub fn size_of_result(self, expected: impl Into<ExpectedCount>) -> Self {
        self.count("sizeOfResult", expected)
    }

    pub fn plan_executions(self, expected: impl Into<ExpectedCount>) -> Self {
        self.count("planExecutions", expected)
    }

    fn require_node(&self) -> &'a Value {
        self.path
            .iter()
            .try_fold(self.root, |value, segment| descend(value, segment))
            .unwrap_or_else(|| {
                panic!(
                    "missing explain node at path [{}]: {}",
                    self.path.join(", "),
                    self.root
                )
            })
    }

    fn count(self, name: &str, expected: impl Into<ExpectedCount>) -> Self {
        let expected = expected.into();
        let node = self.require_node();
        let value = match node.get(name) {
            Some(value) => value,
            None => panic!(
                "missing '{name}' at path [{}]: {node}",
                self.path.join(", "),
            ),
        };
        let actual = match value.as_u64() {
            Some(actual) => actual,
            None => panic!(
                "'{name}' at path [{}] is not a count: {value}",
                self.path.join(", "),
            ),
        };

        assert!(
            expected.bounds().contains(&actual),
            "'{name}' at path [{}] expected {}, got {actual}: {node}",
            self.path.join(", "),
            describe(expected),
        );
        self
    }
}

fn descend<'a>(value: &'a Value, segment: &str) -> Option<&'a Value> {
    match value {
        Value::Object(fields) => fields.get(segment),
        Value::Array(items) => {
            if let Ok(index) = segment.parse::<usize>() {
                items.get(index)
            } else {
                items.iter().find_map(|item| item.get(segment))
            }
        }
        _ => None,
    }
}

fn describe(expected: ExpectedCount) -> String {
    match expected {
        ExpectedCount::Exact(value) => format!("{value}..={value}"),
        ExpectedCount::Range(range) => format!("{}..{}", range.start, range.end),
        ExpectedCount::RangeInclusive(range) => format!("{}..={}", range.start(), range.end()),
        ExpectedCount::AtMost(value) => format!("..={value}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn failure(assert: impl FnOnce() + std::panic::UnwindSafe) -> String {
        *std::panic::catch_unwind(assert)
            .unwrap_err()
            .downcast::<String>()
            .unwrap()
    }

    #[test]
    fn follows_objects_arrays_and_named_array_children() {
        let explain = json!({
            "explain": {
                "operationNode": [{
                    "selectTopNode": {
                        "selectNode": {
                            "iterations": 2,
                            "typeIndexJoin": {
                                "sequenceNode": [
                                    {"orphanNode": {}},
                                    {"typeJoinOne": {"root": {"scanNode": {"indexFetches": 2}}}},
                                ]
                            }
                        }
                    }
                }]
            }
        });

        let join_one = ExplainAsserter::new(&explain)
            .path(&[
                "explain",
                "operationNode",
                "0",
                "selectTopNode",
                "selectNode",
                "typeIndexJoin",
                "sequenceNode",
                "typeJoinOne",
            ])
            .node();
        assert_eq!(join_one["root"]["scanNode"]["indexFetches"], 2);
    }

    #[test]
    fn asserts_counts_while_chaining_deeper() {
        let explain = json!({
            "selectNode": {
                "iterations": 2,
                "filterMatches": 2,
                "typeIndexJoin": {
                    "iterations": 2,
                    "typeJoinOne": {
                        "root": {"scanNode": {"docFetches": 2, "fieldFetches": 4}},
                        "subType": {"selectTopNode": {"selectNode": {"scanNode": {}}}},
                    }
                }
            }
        });

        ExplainAsserter::new(&explain)
            .path(&["selectNode"])
            .iterations(2)
            .filter_matches(2)
            .enter("typeIndexJoin")
            .iterations(2)
            .enter("typeJoinOne")
            .enter("root")
            .enter("scanNode")
            .doc_fetches(2)
            .field_fetches(4)
            .exists();
    }

    #[test]
    fn a_wrong_count_names_the_path_expected_and_actual() {
        let explain = json!({"selectNode": {"iterations": 3}});
        let message = failure(|| {
            ExplainAsserter::new(&explain)
                .path(&["selectNode"])
                .iterations(2);
        });

        assert!(message.contains("path [selectNode]"));
        assert!(message.contains("expected 2..=2"));
        assert!(message.contains("got 3"));
        assert!(message.contains(r#"{"iterations":3}"#));
    }

    #[test]
    fn a_missing_node_names_the_full_path() {
        let explain = json!({"selectNode": {}});
        let message = failure(|| {
            ExplainAsserter::new(&explain)
                .path(&["selectNode", "typeIndexJoin", "typeJoinOne"])
                .iterations(1);
        });

        assert!(message.contains("path [selectNode, typeIndexJoin, typeJoinOne]"));
        assert!(message.contains(r#"{"selectNode":{}}"#));
    }

    #[test]
    fn a_missing_or_nonnumeric_count_fails_with_its_path() {
        let explain = json!({"selectNode": {"iterations": "two"}});
        let missing = failure(|| {
            ExplainAsserter::new(&explain)
                .path(&["selectNode"])
                .doc_fetches(1);
        });
        assert!(missing.contains("missing 'docFetches' at path [selectNode]"));

        let nonnumeric = failure(|| {
            ExplainAsserter::new(&explain)
                .path(&["selectNode"])
                .iterations(2);
        });
        assert!(nonnumeric.contains("'iterations' at path [selectNode] is not a count"));
    }

    #[test]
    fn range_and_upper_bound_counts_are_enforced() {
        let explain = json!({"selectNode": {"iterations": 5}});

        ExplainAsserter::new(&explain)
            .path(&["selectNode"])
            .iterations(ExpectedCount::between(4, 6));

        let range = failure(|| {
            ExplainAsserter::new(&explain)
                .path(&["selectNode"])
                .iterations(ExpectedCount::between(2, 4));
        });
        assert!(range.contains("expected 2..=4"));
        assert!(range.contains("got 5"));

        let at_most = failure(|| {
            ExplainAsserter::new(&explain)
                .path(&["selectNode"])
                .iterations(ExpectedCount::at_most(4));
        });
        assert!(at_most.contains("expected ..=4"));
        assert!(at_most.contains("got 5"));

        let empty = failure(|| {
            ExplainAsserter::new(&explain)
                .path(&["selectNode"])
                .iterations(ExpectedCount::Range(0..0));
        });
        assert!(empty.contains("expected 0..0"));
    }
}
