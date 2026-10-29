use defra_harness::TestCluster;
use serde_json::Value;
use std::process::Command;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum MutationError {
    #[error("execute mutation: {0}")]
    Spawn(std::io::Error),
    #[error("mutation command failed: {0}")]
    Failed(String),
    #[error("mutation output is not UTF-8: {0}")]
    NotUtf8(#[from] std::string::FromUtf8Error),
    #[error("mutation returned no JSON")]
    NoJson,
    #[error("mutation response is not JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid GraphQL errors")]
    InvalidErrors,
    #[error("mutation returned GraphQL errors: {0}")]
    GraphQl(Value),
    #[error("mutation returned no data")]
    NoData,
    #[error("mutation aborted after five transaction conflicts: {0}")]
    Aborted(String),
}

type Result<T> = std::result::Result<T, MutationError>;

/// Both runtimes end a conflict message with corekv's wording; Go emits it
/// bare, Rust prefixes it with the commit-error chain.
const TXN_CONFLICT: &str = "transaction conflict. Please retry";

/// Go can exit successfully with both mutation data and a commit error.
/// Retry only explicit transaction aborts, never a failed convergence assertion.
pub async fn execute(cluster: &TestCluster, node: usize, query: &str) -> Result<Value> {
    let address = cluster.api_url(node).trim_start_matches("http://");
    for attempt in 0..5 {
        let output = Command::new(cluster.client(node).binary_path())
            .args(["--url", address, "client", "query", query])
            .output()
            .map_err(MutationError::Spawn)?;
        if !output.status.success() {
            return Err(MutationError::Failed(
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ));
        }
        let stdout = String::from_utf8(output.stdout)?;
        if let Some(data) = mutation_result(&stdout)? {
            return Ok(data);
        }
        if attempt < 4 {
            nuclei::time::sleep(Duration::from_millis(50)).await;
        }
    }
    Err(MutationError::Aborted(query.to_owned()))
}

fn mutation_result(stdout: &str) -> Result<Option<Value>> {
    let start = stdout.find('{').ok_or(MutationError::NoJson)?;
    let response: Value = serde_json::from_str(&stdout[start..])?;
    if let Some(errors) = response.get("errors").filter(|value| !value.is_null()) {
        let errors = errors.as_array().ok_or(MutationError::InvalidErrors)?;
        if !errors.is_empty() {
            if errors.iter().all(|error| {
                error["message"]
                    .as_str()
                    .is_some_and(|message| message.ends_with(TXN_CONFLICT))
            }) {
                return Ok(None);
            }
            return Err(MutationError::GraphQl(response));
        }
    }
    let data = response
        .get("data")
        .filter(|data| !data.is_null())
        .ok_or(MutationError::NoData)?;
    Ok(Some(data.clone()))
}

#[test]
fn commit_conflict_is_not_success_even_with_mutation_data() {
    for message in [
        "transaction conflict. Please retry",
        "commit error: datastore error: storage error: transaction conflict. Please retry",
    ] {
        let response = serde_json::json!({
            "data": {"update_User": [{"_docID": "doc"}]},
            "errors": [{"message": message}]
        });
        assert!(mutation_result(&response.to_string()).unwrap().is_none());
    }
}

#[test]
fn unrelated_errors_are_not_retried_or_hidden_by_data() {
    for errors in [
        serde_json::json!([{"message": "invalid field"}]),
        serde_json::json!([
            {"message": "transaction conflict. Please retry"},
            {"message": "invalid field"}
        ]),
    ] {
        let response = serde_json::json!({"data": {"update_User": []}, "errors": errors});
        assert!(mutation_result(&response.to_string()).is_err());
    }
}

#[test]
fn successful_response_requires_data_and_allows_cli_preamble() {
    let result = mutation_result(
        "------ Request Results ------\n{\"data\": {\"update_User\": []}, \"errors\": []}",
    )
    .unwrap();
    assert_eq!(result, Some(serde_json::json!({"update_User": []})));
    for invalid in ["", "not JSON", "{}", "{\"data\":null}", "{\"errors\":{}}"] {
        assert!(mutation_result(invalid).is_err());
    }
}
