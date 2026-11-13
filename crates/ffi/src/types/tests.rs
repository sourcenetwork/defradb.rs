use super::*;

/// Whether `ptr` is a set, empty C string: what Go's `returnC` puts in the
/// field a result does not use, never null.
fn is_empty(ptr: *mut c_char) -> bool {
    !ptr.is_null() && unsafe { CStr::from_ptr(ptr) }.to_bytes().is_empty()
}

#[test]
fn test_defra_result_success() {
    let mut result = DefraResult::success(r#"{"data": "test"}"#);
    assert_eq!(result.status, 0);
    assert!(is_empty(result.error), "no error, but never null");
    assert!(!result.value.is_null());

    unsafe { defra_result_free(&mut result) };
}

#[test]
fn test_defra_result_error() {
    let mut result = DefraResult::error("something went wrong");
    assert_eq!(result.status, 1);
    assert!(!result.error.is_null());
    assert!(is_empty(result.value), "no value, but never null");

    unsafe { defra_result_free(&mut result) };
}

#[test]
fn test_new_node_result() {
    let result = DefraNewNodeResult::success(42);
    assert_eq!(result.status, 0);
    assert_eq!(result.node_ptr, 42);
    assert!(result.error.is_null());
}

// Edge case tests for null byte handling (H2)

#[test]
fn test_defra_result_success_with_null_bytes() {
    // String with embedded null byte should not panic
    let value_with_null = "hello\0world";
    let mut result = DefraResult::success(value_with_null);
    assert_eq!(result.status, 0);
    assert!(!result.value.is_null());

    // Value should have null bytes replaced
    let value = unsafe { CStr::from_ptr(result.value).to_string_lossy() };
    assert!(value.contains('\u{FFFD}'), "null byte should be replaced");
    assert!(!value.contains('\0'), "should not contain null byte");

    unsafe { defra_result_free(&mut result) };
}

#[test]
fn test_defra_result_error_with_null_bytes() {
    // Error message with embedded null byte should not panic
    let error_with_null = "error\0message";
    let mut result = DefraResult::error(error_with_null);
    assert_eq!(result.status, 1);
    assert!(!result.error.is_null());

    // Error should have null bytes replaced
    let error = unsafe { CStr::from_ptr(result.error).to_string_lossy() };
    assert!(error.contains('\u{FFFD}'), "null byte should be replaced");
    assert!(!error.contains('\0'), "should not contain null byte");

    unsafe { defra_result_free(&mut result) };
}

#[test]
fn test_new_node_result_error_with_null_bytes() {
    // Error message with embedded null byte should not panic
    let error_with_null = "node\0error";
    let mut result = DefraNewNodeResult::error(error_with_null);
    assert_eq!(result.status, 1);
    assert!(!result.error.is_null());
    assert_eq!(result.node_ptr, 0);

    // Error should have null bytes replaced
    let error = unsafe { CStr::from_ptr(result.error).to_string_lossy() };
    assert!(error.contains('\u{FFFD}'), "null byte should be replaced");

    unsafe { defra_new_node_result_free(&mut result) };
}

#[test]
fn test_c_str_to_string_null_ptr() {
    let result = unsafe { c_str_to_string(ptr::null()) };
    assert!(result.is_none());
}

#[test]
fn freeing_a_result_nulls_its_strings_so_a_second_free_does_nothing() {
    let mut result = DefraResult::error("gone");
    unsafe { defra_result_free(&mut result) };
    assert!(result.error.is_null());
    assert!(result.value.is_null());
    unsafe { defra_result_free(&mut result) };

    let mut node = DefraNewNodeResult::error("gone");
    unsafe { defra_new_node_result_free(&mut node) };
    assert!(node.error.is_null());
    unsafe { defra_new_node_result_free(&mut node) };

    let mut txn = DefraNewTxnResult::error("gone");
    unsafe { defra_new_txn_result_free(&mut txn) };
    assert!(txn.error.is_null());
    unsafe { defra_new_txn_result_free(&mut txn) };

    let mut identity = DefraNewIdentityResult::error("gone");
    unsafe { defra_new_identity_result_free(&mut identity) };
    assert!(identity.error.is_null());
    unsafe { defra_new_identity_result_free(&mut identity) };
}

#[test]
fn freeing_a_null_result_does_nothing() {
    unsafe {
        defra_result_free(ptr::null_mut());
        defra_new_node_result_free(ptr::null_mut());
        defra_new_txn_result_free(ptr::null_mut());
        defra_new_identity_result_free(ptr::null_mut());
    }
}

#[test]
fn test_defra_result_ok() {
    let mut result = DefraResult::ok();
    assert_eq!(result.status, 0);
    assert!(
        is_empty(result.error),
        "both strings are set, as Go sets them"
    );
    assert!(is_empty(result.value));
    unsafe { defra_result_free(&mut result) };
}

#[test]
fn a_secret_success_holds_the_hex_of_its_bytes() {
    let mut result = DefraResult::success_secret_hex(&[0x00, 0x1f, 0xab, 0xff]);
    assert_eq!(result.status, 0);
    assert!(is_empty(result.error));
    let value = unsafe { CStr::from_ptr(result.value) }.to_str().unwrap();
    assert_eq!(value, "001fabff");
    unsafe { defra_result_free(&mut result) };
    assert!(result.value.is_null());
}
