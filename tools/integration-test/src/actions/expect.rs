use std::fmt::Display;

/// Checks a node result against an optional expected error, matching Go's
/// `ExpectedError`: a substring of the returned error.
pub fn expect_error<T, E>(result: Result<T, E>, expected: Option<&str>) -> Option<T>
where
    E: Display,
{
    match (result, expected) {
        (Ok(value), None) => Some(value),
        (Ok(_), Some(expected)) => panic!("expected error containing {expected:?}, got success"),
        (Err(e), None) => panic!("unexpected error: {e}"),
        (Err(e), Some(expected)) => {
            let message = e.to_string();
            assert!(
                message.contains(expected),
                "expected error containing {expected:?}, got {message:?}"
            );
            None
        }
    }
}
