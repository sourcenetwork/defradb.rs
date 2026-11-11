use std::time::{Duration, Instant};

use super::Request;
use crate::client_type::ClientType;
use crate::scenario::{Common, Failure, Op, Text};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// The `query.benchmarkIndex` op.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Benchmark {
    pub base_query: String,
    pub optimized_query: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for Benchmark {
    fn common(&self) -> &Common {
        &self.common
    }

    fn expected_data(&self) -> &Value {
        &self.expected_data
    }

    fn expected_error(&self) -> Option<&str> {
        self.expected_error.as_deref()
    }

    /// Go's `benchmarkAction`: run each query `repetitions` times through the
    /// test's client, and pass when the base case takes more than `factor`
    /// times the optimized one.
    fn run(&self, state: &State) -> Result<Value, Failure> {
        let (repetitions, factor) = self.settings()?;
        let base = self.timed(state, &self.base_query, repetitions)?;
        let optimized = self.timed(state, &self.optimized_query, repetitions)?;
        Ok(json!({
            "passed": base.as_secs_f64() / factor > optimized.as_secs_f64(),
            "repetitions": repetitions,
            "factor": self.expected_settings()["factor"],
        }))
    }
}

impl Benchmark {
    /// The repetitions and factor, which the corpus writes only in the
    /// expected result.
    fn settings(&self) -> Result<(u64, f64), Failure> {
        let settings = self.expected_settings();
        match (
            settings["repetitions"].as_u64(),
            settings["factor"].as_f64(),
        ) {
            (Some(repetitions), Some(factor)) if factor > 0.0 => Ok((repetitions, factor)),
            _ => Err(Failure::operation(format!(
                "Benchmark needs repetitions and factor in its expected data, got {settings}"
            ))),
        }
    }

    fn expected_settings(&self) -> &Value {
        self.expected_data
            .get("fields")
            .unwrap_or(&self.expected_data)
    }

    /// How long `query` takes to run `repetitions` times; an error response
    /// fails the benchmark.
    fn timed(&self, state: &State, query: &str, repetitions: u64) -> Result<Duration, Failure> {
        let started = Instant::now();
        for _ in 0..repetitions {
            self.query_once(state, query)?;
        }
        Ok(started.elapsed())
    }

    /// Run `query` once through the test's client, as the action's client
    /// identity on every client, as Go's `benchmarkAction` runs its `Request`.
    fn query_once(&self, state: &State, query: &str) -> Result<(), Failure> {
        let node = self.common.node();
        let identity = self.common.client_identity(Self::name())?;
        let errors = match state.client_type() {
            ClientType::InProcess => {
                let request = Request {
                    request: Text::from(query),
                    common: self.common.clone(),
                    ..Default::default()
                };
                request.run(state)?.get("errors").cloned()
            }
            ClientType::RustHttp | ClientType::GoHttp => {
                let query = query.to_owned();
                let response = state
                    .http_as(node, identity, |client| async move {
                        client.graphql(&query, None, None).await
                    })?
                    .map_err(|e| Failure::operation(e.to_string()))?;
                let messages: Vec<String> =
                    response.errors.into_iter().map(|e| e.message).collect();
                (!messages.is_empty()).then(|| json!(messages))
            }
            // Go benchmarks only its embedded client. A process per query
            // costs more than the gap the benchmark measures.
            ClientType::Cli => {
                return Err(Failure::unsupported(
                    "Benchmark over the CLI, whose process per query swamps the gap",
                ))
            }
        };
        match errors {
            Some(errors) => Err(Failure::operation(format!(
                "Benchmark query failed: {errors}"
            ))),
            None => Ok(()),
        }
    }
}

impl Action for Benchmark {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}
