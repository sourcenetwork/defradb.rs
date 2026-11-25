#![allow(dead_code)]
#![deny(unfulfilled_lint_expectations)]

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<Failure = Error, Value = ()> = std::result::Result<Value, Failure>;

#[cfg(not(test))]
pub fn nested_success() -> Result<Error, std::result::Result<(), Error>> {
    Ok(Ok(()))
}

pub fn correctly_spelled_success() -> Result<Error, Result> {
    Ok(Ok(()))
}

fn main() {}
