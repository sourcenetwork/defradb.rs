#[test]
fn unique_constraint_violation_is_classified() {
    let e = db::index::Error::Storage(storage::Error::UniqueConstraintViolation);
    assert!(e.is_unique_constraint_violation());
}

#[test]
fn non_unique_storage_error_is_not_classified() {
    let e = db::index::Error::Storage(storage::Error::Other("disk full".to_string()));
    assert!(!e.is_unique_constraint_violation());
}

#[test]
fn non_storage_index_error_is_not_classified() {
    let e = db::index::Error::Other("index misconfigured".to_string());
    assert!(!e.is_unique_constraint_violation());
}
