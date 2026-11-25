// compile-flags: --test
#![allow(dead_code)]

// Test harness helpers are internal to an executable, even when marked pub.
pub fn helper() -> std::io::Result<()> {
    Ok(())
}

#[test]
fn foreign_test_result() -> std::io::Result<()> {
    Ok(())
}

#[test]
fn unit() {
    assert!(helper().is_ok());
}
