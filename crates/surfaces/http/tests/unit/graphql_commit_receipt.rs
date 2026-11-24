use super::*;

#[nuclei::test]
async fn only_typed_commit_conflict_carries_abort_receipt() {
    for (error, confirmed) in [
        (TransactionError::conflict("typed"), true),
        (
            TransactionError::execution("commit error: transaction conflict. Please retry"),
            false,
        ),
    ] {
        let response = commit_error_response(error);
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["error"], query::executor::TXN_CONFLICT_MESSAGE);
        assert_eq!(value.get("outcome").is_some(), confirmed);
        if confirmed {
            assert_eq!(value["outcome"], "aborted");
        }
    }
}

#[test]
#[ignore = "Requires Lean; run in the Lean Conformance job"]
fn commit_receipt_matches_model() {
    let output = std::process::Command::new("lake")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../proofs/lean"))
        .args(["env", "lean", "--run", "CommitConflictReceipt.lean"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let expected: Vec<bool> = serde_json::from_slice(&output.stdout).unwrap();
    let typed = TransactionError::conflict("typed");
    let execution = TransactionError::execution("transaction conflict. Please retry");
    let actual: Vec<bool> = [Some(&typed), Some(&execution), None]
        .into_iter()
        .map(commit_has_abort_receipt)
        .collect();
    assert_eq!(actual, expected);
}

/// A commit failure says what the commit did: `indeterminate` for one that
/// failed while the engine made it durable, `aborted` for one refused before
/// anything was durable, what the registry retained for a transaction it no
/// longer holds, and nothing for an error that ended no commit
/// (`defra.txn.commit_outcome`).
#[nuclei::test]
async fn a_commit_failure_carries_its_outcome() {
    let io = storage::Error::from(std::io::Error::from_raw_os_error(5)).at_commit();
    for (error, status, outcome) in [
        (
            TransactionError::Storage(io),
            StatusCode::INTERNAL_SERVER_ERROR,
            Some("indeterminate"),
        ),
        (
            TransactionError::not_found("1")
                .with_commit_outcome(defra_core::CommitOutcome::NotCommitted),
            StatusCode::NOT_FOUND,
            Some("aborted"),
        ),
        (
            TransactionError::not_found("1")
                .with_commit_outcome(defra_core::CommitOutcome::Indeterminate),
            StatusCode::NOT_FOUND,
            Some("indeterminate"),
        ),
        (
            TransactionError::commit_refused("datastore is closed"),
            StatusCode::BAD_REQUEST,
            Some("aborted"),
        ),
        (
            TransactionError::operation("refused"),
            StatusCode::BAD_REQUEST,
            None,
        ),
    ] {
        let response = commit_error_response(error);
        assert_eq!(response.status(), status);
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value.get("outcome").and_then(|v| v.as_str()), outcome);
    }
}
