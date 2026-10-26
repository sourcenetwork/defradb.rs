use std::io::{Error, ErrorKind};
use std::process::Command;

use query::{QueryError, QueryResponse, QueryResponseError};

#[test]
#[ignore = "requires Lean; executed explicitly by the Lean Conformance CI job"]
fn storage_failure_classification_matches_executed_model() {
    let output = Command::new("lake")
        .args(["env", "lean", "--run", "StorageFailure.lean"])
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../proofs/lean"))
        .output()
        .expect("execute the storage failure model");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let mut cases = 0;
    for line in stdout.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 4, "{line}");
        let [tag, raw, message, expected] = fields[..] else {
            unreachable!()
        };
        let error = if tag == "opaque" {
            storage::Error::Backend(message.into())
        } else {
            let cause = if raw == "none" {
                Error::new(
                    if tag == "full" {
                        ErrorKind::StorageFull
                    } else {
                        ErrorKind::Other
                    },
                    message,
                )
            } else {
                {
                    #[cfg(unix)]
                    let cause = Error::from_raw_os_error(raw.parse().unwrap());
                    #[cfg(not(unix))]
                    let cause = Error::new(ErrorKind::StorageFull, message);
                    cause
                }
            };
            let expected_kind = cause.kind();
            let expected_os = cause.raw_os_error();
            let expected_message = cause.to_string();
            let error = storage::Error::from(regolith::Error::Io(cause));
            let storage::Error::Io(facts) = &error else {
                panic!("I/O facts lost: {error:?}")
            };
            assert_eq!(facts.kind(), expected_kind);
            assert_eq!(facts.raw_os_error(), expected_os);
            assert_eq!(facts.to_string(), expected_message);
            error
        };
        let response = QueryResponse::error(QueryResponseError::from_query_error(
            QueryError::Storage(error),
        ));
        assert!(!response.is_transaction_conflict());
        assert!(!response.is_transaction_retry_exhausted());
        let encoded = serde_json::to_value(&response).unwrap();
        let decoded: QueryResponse = serde_json::from_value(encoded).unwrap();
        assert_eq!(
            decoded.errors[0].code(),
            (expected != "none").then_some(expected),
            "{line}"
        );
        cases += 1;
    }
    assert!(cases > 0, "model produced no inputs");
}
