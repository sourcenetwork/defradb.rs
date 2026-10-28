mod branchable;
mod explain;
mod relational;
mod scalar;
mod validate;

use rapidhash::RapidHashMap;
use serde_json::Value;

use super::normalize::{equal_step_results, normalized_documents, normalized_result};
use super::ScenarioError;

pub use validate::validate;

/// What `same` and `unique` matchers have bound so far in one scenario.
#[derive(Clone, Debug, Default)]
pub struct Bindings {
    pub(crate) same: RapidHashMap<String, Value>,
    pub(crate) unique: RapidHashMap<String, Vec<Value>>,
}

/// The ordering escapes a step names.
#[derive(Clone, Copy, Debug, Default)]
pub struct Options<'a> {
    pub unordered: &'a [String],
    pub unordered_documents: &'a [String],
}

/// Matches `actual` against `expected`. Bindings are committed to `bindings`
/// only when the match succeeds.
pub fn match_expected(
    expected: &Value,
    actual: &Value,
    options: Options<'_>,
    bindings: &mut Bindings,
) -> Result<bool, ScenarioError> {
    if !validate(expected)? {
        return equal_step_results(
            expected,
            actual,
            options.unordered,
            options.unordered_documents,
        );
    }
    for value in [expected, actual] {
        normalized_documents(value, options.unordered_documents)?;
        normalized_result(value, options.unordered)?;
    }
    relational::match_relational(expected, actual, options, bindings)
}

pub(crate) fn contains_matcher(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.contains_key("$match") || map.values().any(contains_matcher),
        Value::Array(items) => items.iter().any(contains_matcher),
        _ => false,
    }
}
