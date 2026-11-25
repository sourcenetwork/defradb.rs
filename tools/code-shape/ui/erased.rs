// aux-build: foreign.rs
#![allow(dead_code, type_alias_bounds)]

extern crate foreign;

use std::boxed::Box as Heap;
use std::error::Error as Failure;

type Erased = Heap<dyn Failure>;
type Wrapped<Value>
where
    Value: ?Sized,
= Heap<Value>;

fn direct() -> Heap<dyn Failure> {
    unimplemented!()
}

fn alias() -> Erased {
    unimplemented!()
}

trait ErasedCallback: FnOnce() -> Erased {}

trait AssociatedFuture {
    type Work: std::future::Future<Output = Result<(), Erased>>;
}

trait SpecificFailure: Failure {}
type Specialized = Heap<dyn SpecificFailure>;

fn specialized() -> Specialized {
    unimplemented!()
}

async fn nested() -> Option<Wrapped<dyn Failure>> {
    unimplemented!()
}

fn concrete() -> Result<(), std::io::Error> {
    Ok(())
}

struct Concrete {
    source: Heap<dyn Failure>,
}

fn accepts_conversion<Value>(_: Value)
where
    Value: Into<Erased>,
{}

fn accepts_inverse_conversion<Value>()
where
    Value: From<Erased>,
{}

fn exposed_input(_: Erased) {}

fn future_error<Work>(_: Work)
where
    Work: std::future::Future<Output = Result<(), Erased>>,
{}

#[derive(Debug)]
struct UntypedSource {
    source: Heap<dyn Failure>,
}

impl std::fmt::Display for UntypedSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.source.fmt(f)
    }
}

// This fixture isolates field detection from the independent derive rule.
#[allow(manual_error_impl)]
impl Failure for UntypedSource {}

impl foreign::Fixed for Concrete {
    fn fail() -> Heap<dyn Failure> {
        unimplemented!()
    }
}

impl TryFrom<u8> for Concrete {
    type Error = Heap<dyn Failure>;

    fn try_from(_: u8) -> Result<Self, Self::Error> {
        unimplemented!()
    }
}

fn main() {}
