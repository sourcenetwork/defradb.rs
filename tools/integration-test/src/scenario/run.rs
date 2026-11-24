use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::common::{Common, Eventually};
use super::matcher::{match_expected, Options};
use super::op::{Failure, FailureKind, Op};
use super::pointer::{pointer, resolve};
use crate::test_case::State;

const RETRIES: usize = 5;
const RETRY_PAUSE: Duration = Duration::from_millis(100);
const EVENTUALLY_TIMEOUT: Duration = Duration::from_secs(20);
const EVENTUALLY_INTERVAL: Duration = Duration::from_millis(100);

fn graphql_errors(value: &Value) -> Vec<&str> {
    value
        .get("errors")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|error| error.get("message").and_then(Value::as_str))
        .collect()
}

/// Whether a GraphQL error whose message contains `wanted` carries
/// `extensions.code` `code`.
fn carries_code<A>(outcome: &Result<Value, Failure>, wanted: &str, code: &str) -> bool
where
    A: Op,
{
    let Ok(value) = outcome else {
        return false;
    };
    A::GRAPHQL
        && value
            .get("errors")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|error| {
                error
                    .get("message")
                    .and_then(Value::as_str)
                    .is_some_and(|message| message.contains(wanted))
            })
            .any(|error| error.pointer("/extensions/code").and_then(Value::as_str) == Some(code))
}

/// Runs `call` again while it loses a transaction conflict. A response
/// with GraphQL errors becomes the failure of its first error, unless
/// `keeps_errors` asks for the response itself, as an action that checks an
/// error's `extensions.code` does.
fn with_retry<F>(call: F, keeps_errors: bool) -> Result<Value, Failure>
where
    F: Fn() -> Result<Value, Failure>,
{
    for attempt in 0..RETRIES {
        let outcome = call();
        let failure = match &outcome {
            Ok(value) => graphql_errors(value)
                .first()
                .map(|first| Failure::operation((*first).to_owned())),
            Err(failure) => Some(failure.clone()),
        };
        let conflict = matches!(&failure, Some(f) if f.kind == FailureKind::TransactionConflict);
        if !conflict || attempt + 1 == RETRIES {
            return match failure {
                Some(failure) if !keeps_errors || outcome.is_err() => Err(failure),
                _ => outcome,
            };
        }
        std::thread::sleep(RETRY_PAUSE);
    }
    unreachable!("the last attempt returns")
}

fn expectation<A>(action: &A) -> Option<Value>
where
    A: Op,
{
    let data = action.expected_data();
    if data.is_null() {
        return None;
    }
    let envelope = action.common().envelope;
    if !A::GRAPHQL || envelope {
        return Some(data.clone());
    }
    let mut expected = json!({"data": data});
    let extensions = action.expected_extensions();
    if !extensions.is_null() {
        expected["extensions"] = extensions.clone();
    }
    Some(expected)
}

fn poll_until<A, R>(
    state: &State,
    action: &A,
    expected: &Value,
    timing: &Eventually,
    call: &R,
) -> Result<Value, Failure>
where
    A: Op,
    R: Fn(&A, &State) -> Result<Value, Failure>,
{
    let common = action.common();
    let timeout = timing
        .timeout_millis
        .map_or(EVENTUALLY_TIMEOUT, Duration::from_millis);
    let interval = timing
        .interval_millis
        .map_or(EVENTUALLY_INTERVAL, Duration::from_millis);
    let deadline = Instant::now() + timeout;
    loop {
        let outcome = call(action, state);
        let reason = match &outcome {
            Err(failure) if failure.kind == FailureKind::Unsupported => return outcome,
            Err(failure) => failure.message.clone(),
            Ok(value) if !graphql_errors(value).is_empty() => {
                format!("GraphQL errors: {:?}", graphql_errors(value))
            }
            Ok(value) => {
                let mut candidate = state.bindings.clone();
                let options = options(common);
                match match_expected(expected, value, options, &mut candidate) {
                    Ok(true) => return outcome,
                    Ok(false) => mismatch(expected, value),
                    Err(e) => return Err(Failure::operation(e.to_string())),
                }
            }
        };
        if Instant::now() > deadline {
            return Err(Failure::operation(format!(
                "query did not pass within {timeout:?}: {reason}"
            )));
        }
        std::thread::sleep(interval);
    }
}

fn options(common: &Common) -> Options<'_> {
    Options {
        unordered: &common.unordered,
        unordered_documents: &common.unordered_documents,
    }
}

fn match_envelope(
    state: &mut State,
    common: &Common,
    envelope: &Value,
    expected: Option<&Value>,
) -> Result<(), String> {
    let Some(expected) = expected else {
        return Ok(());
    };
    match match_expected(expected, envelope, options(common), &mut state.bindings) {
        Ok(true) => Ok(()),
        Ok(false) => Err(mismatch(expected, envelope)),
        Err(e) => Err(e.to_string()),
    }
}

fn run<A, R, M>(original: &A, state: &mut State, call: &R, call_mut: &M) -> Result<(), String>
where
    A: Op,
    R: Fn(&A, &State) -> Result<Value, Failure>,
    M: Fn(&A, &mut State) -> Result<Value, Failure>,
{
    let args = serde_json::to_value(original).map_err(|e| e.to_string())?;
    let args = resolve(&args, &state.captures).map_err(|e| e.to_string())?;
    let action = serde_json::from_value::<A>(args)
        .map_err(|e| e.to_string())?
        .resolved();
    let common = action.common();
    let expected = expectation(&action);
    let outcome = match (&common.eventually, &expected) {
        (Some(timing), Some(expected)) => poll_until(state, &action, expected, timing, call),
        _ if common.retry => {
            let keeps_errors = action.expected_error_code().is_some();
            with_retry(|| call(&action, state), keeps_errors)
        }
        _ => call_mut(&action, state),
    };
    if let Err(failure) = &outcome {
        if failure.kind == FailureKind::Unsupported {
            return Err(format!("unsupported op: {}", failure.message));
        }
    }
    if common.envelope && action.expected_error_code().is_some() {
        return Err("an expected error code cannot be checked through an envelope".to_owned());
    }
    if common.envelope {
        let envelope = match outcome {
            Ok(value) => json!({"result": value, "error": ""}),
            Err(failure) => json!({"result": null, "error": failure.message}),
        };
        return match_envelope(state, common, &envelope, expected.as_ref());
    }
    if let Some(code) = action.expected_error_code() {
        let Some(wanted) = action.expected_error() else {
            return Err(format!(
                "expected error code {code:?} names no expected error"
            ));
        };
        return match carries_code::<A>(&outcome, wanted, code) {
            true => Ok(()),
            false => Err(format!(
                "expected error containing {wanted:?} with code {code:?}; got {}",
                describe(&outcome)
            )),
        };
    }
    if let Some(wanted) = action.expected_error() {
        let matched = match &outcome {
            Err(failure) => failure.message.contains(wanted),
            Ok(value) => A::GRAPHQL && graphql_errors(value).iter().any(|m| m.contains(wanted)),
        };
        if !matched {
            return Err(format!(
                "expected error containing {wanted:?}; got {}",
                describe(&outcome)
            ));
        }
        return match_failure_class(&action, &outcome);
    }
    if action.expected_commit_outcome().is_some() || action.expected_exit_code().is_some() {
        return Err(format!(
            "an expected commit outcome or exit status needs an expected error; got {}",
            describe(&outcome)
        ));
    }
    let actual = outcome.map_err(|failure| failure.message)?;
    if A::GRAPHQL && !graphql_errors(&actual).is_empty() {
        return Err(format!("GraphQL errors: {:?}", graphql_errors(&actual)));
    }
    if let Some(expected) = &expected {
        let matched = match_expected(expected, &actual, options(common), &mut state.bindings)
            .map_err(|e| e.to_string())?;
        if !matched {
            return Err(mismatch(expected, &actual));
        }
    }
    for (name, path) in &common.capture {
        if state.captures.contains_key(name) {
            return Err(format!("duplicate capture: {name}"));
        }
        let value = pointer(&actual, path).map_err(|e| e.to_string())?.clone();
        state.captures.insert(name.clone(), value);
    }
    Ok(())
}

/// The failure's commit outcome and, under the CLI, its exit status, against
/// what the action expects. A failure from a client that reports no exit
/// status (in-process, HTTP) is not checked for one.
fn match_failure_class<A>(action: &A, outcome: &Result<Value, Failure>) -> Result<(), String>
where
    A: Op,
{
    let failure = outcome.as_ref().err();
    if let Some(wanted) = action.expected_commit_outcome() {
        let got = failure.and_then(|failure| failure.outcome.as_deref());
        if got != Some(wanted) {
            return Err(format!(
                "expected commit outcome {wanted:?}; got {got:?} with {}",
                describe(outcome)
            ));
        }
    }
    if let (Some(wanted), Some(got)) = (
        action.expected_exit_code(),
        failure.and_then(|failure| failure.exit_code),
    ) {
        if got != wanted {
            return Err(format!(
                "expected exit status {wanted}; got {got} with {}",
                describe(outcome)
            ));
        }
    }
    Ok(())
}

fn describe(outcome: &Result<Value, Failure>) -> String {
    match outcome {
        Ok(value) => format!("response {value}"),
        Err(failure) => format!("error {:?}", failure.message),
    }
}

/// Runs an action in-process, with its own [`Op::run`] and [`Op::run_mut`].
pub fn execute<A>(action: &A, state: &mut State)
where
    A: Op,
{
    report(action, run(action, state, &A::run, &A::run_mut));
}

/// Runs an action with `call`, one client's body for it: resolves the
/// captures it names, calls it with its retry or polling, and checks the
/// result against what the action expects.
pub fn execute_with<A, R>(action: &A, state: &mut State, call: R)
where
    A: Op,
    R: Fn(&A, &State) -> Result<Value, Failure>,
{
    let call_mut = |action: &A, state: &mut State| call(action, state);
    report(action, run(action, state, &call, &call_mut));
}

fn report<A>(action: &A, outcome: Result<(), String>)
where
    A: Op,
{
    if let Err(message) = outcome {
        let args = serde_json::to_string(action).unwrap_or_default();
        let args: String = args.chars().take(300).collect();
        panic!("{message}\n  action: {} {args}", A::name());
    }
}

/// The most of each side a mismatch prints; a whole introspection result runs
/// to hundreds of kilobytes, and matchers report their own detail.
const SHOWN_BYTES: usize = 4096;

fn mismatch(expected: &Value, got: &Value) -> String {
    format!("expected {}; got {}", clipped(expected), clipped(got))
}

fn clipped(value: &Value) -> String {
    let text = value.to_string();
    match text.char_indices().nth(SHOWN_BYTES) {
        Some((end, _)) => format!("{}… ({} bytes)", &text[..end], text.len()),
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    fn coded(message: &str) -> Value {
        json!({"data": null, "errors": [{"message": message, "extensions": {"code": "CODE"}}]})
    }

    #[test]
    fn a_retried_error_keeps_its_code_when_the_action_checks_one() {
        let outcome = with_retry(|| Ok(coded("refused")), true);
        assert!(carries_code::<crate::actions::Request>(
            &outcome, "refused", "CODE"
        ));
    }

    #[test]
    fn a_retried_error_becomes_a_failure_when_the_action_checks_no_code() {
        let outcome = with_retry(|| Ok(coded("refused")), false);
        assert!(matches!(outcome, Err(f) if f.message == "refused"));
    }

    #[test]
    fn a_conflict_runs_again_and_the_response_after_it_keeps_its_code() {
        let calls = Cell::new(0);
        let outcome = with_retry(
            || {
                calls.set(calls.get() + 1);
                match calls.get() {
                    1 => Ok(coded("transaction conflict")),
                    _ => Ok(coded("refused")),
                }
            },
            true,
        );
        assert_eq!(calls.get(), 2);
        assert!(carries_code::<crate::actions::Request>(
            &outcome, "refused", "CODE"
        ));
    }
}
