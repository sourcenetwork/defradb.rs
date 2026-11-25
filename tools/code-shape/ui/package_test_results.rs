// aux-build: defra_code_shape.rs
// compile-flags: --test
#![deny(unfulfilled_lint_expectations)]

extern crate defra_code_shape as package_library;

use std::io::Result;

fn helper() -> Result<()> {
    Ok(())
}

#[test]
fn qualified() -> package_library::Result<()> {
    assert!(helper().is_ok());
    Ok(())
}

#[test]
#[expect(test_result)]
fn noncanonical() -> package_library::Other<()> {
    Ok(())
}
