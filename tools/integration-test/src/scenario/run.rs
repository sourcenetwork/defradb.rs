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

fn with_retry<F>(call: F) -> Result<Value, Failure>
where
    F: Fn() -> Result<Value, Failure>,
{
    for attempt in 0..RETRIES {
        let mut outcome = call();
        if let Ok(value) = &outcome {
            if let Some(first) = graphql_errors(value).first() {
                outcome = Err(Failure::operation((*first).to_owned()));
            }
        }
        let conflict = matches!(&outcome, Err(f) if f.kind == FailureKind::TransactionConflict);
        if !conflict || attempt + 1 == RETRIES {
            return outcome;
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
                    Ok(false) => format!("expected {expected}; got {value}"),
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
        Ok(false) => Err(format!("expected {expected}; got {envelope}")),
        Err(e) => Err(e.to_string()),
    }
}

fn run<A, R>(original: &A, state: &mut State, call: &R) -> Result<(), String>
where
    A: Op,
    R: Fn(&A, &State) -> Result<Value, Failure>,
{
    let args = serde_json::to_value(original).map_err(|e| e.to_string())?;
    let args = resolve(&args, &state.captures).map_err(|e| e.to_string())?;
    let action: A = serde_json::from_value(args).map_err(|e| e.to_string())?;
    let common = action.common();
    let expected = expectation(&action);
    let outcome = match (&common.eventually, &expected) {
        (Some(timing), Some(expected)) => poll_until(state, &action, expected, timing, call),
        _ if common.retry => with_retry(|| call(&action, state)),
        _ => call(&action, state),
    };
    if let Err(failure) = &outcome {
        if failure.kind == FailureKind::Unsupported {
            return Err(format!("unsupported op: {}", failure.message));
        }
    }
    if common.envelope {
        let envelope = match outcome {
            Ok(value) => json!({"result": value, "error": ""}),
            Err(failure) => json!({"result": null, "error": failure.message}),
        };
        return match_envelope(state, common, &envelope, expected.as_ref());
    }
    if let Some(wanted) = action.expected_error() {
        let matched = match &outcome {
            Err(failure) => failure.message.contains(wanted),
            Ok(value) => A::GRAPHQL && graphql_errors(value).iter().any(|m| m.contains(wanted)),
        };
        return match matched {
            true => Ok(()),
            false => Err(format!(
                "expected error containing {wanted:?}; got {}",
                describe(&outcome)
            )),
        };
    }
    let actual = outcome.map_err(|failure| failure.message)?;
    if A::GRAPHQL && !graphql_errors(&actual).is_empty() {
        return Err(format!("GraphQL errors: {:?}", graphql_errors(&actual)));
    }
    if let Some(expected) = &expected {
        let matched = match_expected(expected, &actual, options(common), &mut state.bindings)
            .map_err(|e| e.to_string())?;
        if !matched {
            return Err(format!("expected {expected}; got {actual}"));
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

fn describe(outcome: &Result<Value, Failure>) -> String {
    match outcome {
        Ok(value) => format!("response {value}"),
        Err(failure) => format!("error {:?}", failure.message),
    }
}

/// Runs an action in-process: [`execute_with`] its own [`Op::run`].
pub fn execute<A>(action: &A, state: &mut State)
where
    A: Op,
{
    execute_with(action, state, A::run);
}

/// Runs an action with `call`, one client's body for it: resolves the
/// captures it names, calls it with its retry or polling, and checks the
/// result against what the action expects.
pub fn execute_with<A, R>(action: &A, state: &mut State, call: R)
where
    A: Op,
    R: Fn(&A, &State) -> Result<Value, Failure>,
{
    if let Err(message) = run(action, state, &call) {
        let args = serde_json::to_string(action).unwrap_or_default();
        let args: String = args.chars().take(300).collect();
        panic!("{message}\n  action: {} {args}", A::name());
    }
}
