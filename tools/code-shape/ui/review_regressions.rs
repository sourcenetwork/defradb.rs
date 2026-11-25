// compile-flags: --crate-type=lib
#![allow(dead_code)]
#![deny(unfulfilled_lint_expectations)]

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct Growing<T>(T);

impl<T> Future for Growing<T> {
    type Output = (Growing<Option<T>>, Growing<Option<T>>);

    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Pending
    }
}

#[expect(crate_result)]
pub fn growing() -> (Growing<()>, std::io::Result<()>) {
    (Growing(()), Ok(()))
}

type Two<T> = Option<Option<T>>;
type Four<T> = Two<Two<T>>;
type Eight<T> = Four<Four<T>>;
type Sixteen<T> = Eight<Eight<T>>;
type ThirtyTwo<T> = Sixteen<Sixteen<T>>;
type SixtyFour<T> = ThirtyTwo<ThirtyTwo<T>>;

#[expect(crate_result)]
pub fn nested_containers() -> SixtyFour<std::io::Result<()>> {
    None
}

pub mod unrelated {}
pub use unrelated as Result;

#[expect(crate_result)]
pub fn module_is_not_a_result_alias() -> std::io::Result<()> {
    Ok(())
}

#[expect(anonymous_callback)]
pub trait Named<Other>: Fn()
where
    Other: Fn(u8),
{
}

pub trait Factory {
    type Callback;
}

#[expect(anonymous_callback)]
pub trait NamedFactory
where
    Self: Factory<Callback: Fn(u8)>,
{
}

#[expect(callback_lifetime_name)]
pub trait NamedWhere<'a>
where
    Self: Fn(&'a u8),
{
}

#[expect(anonymous_callback, crate_result)]
pub fn consumes_error<Callback, Failure>(_: Callback) -> std::result::Result<(), Failure>
where
    Callback: FnOnce(Failure),
    Failure: From<std::io::Error>,
{
    Err(std::io::Error::other("created here").into())
}

#[expect(anonymous_callback, crate_result)]
pub fn async_consumes_error<Callback, Failure>(_: Callback) -> std::result::Result<(), Failure>
where
    Callback: std::ops::AsyncFnOnce(Failure),
    Failure: From<std::io::Error>,
{
    Err(std::io::Error::other("created here").into())
}

#[expect(anonymous_callback)]
pub fn supplies_error<Callback, Failure>(value: Callback) -> std::result::Result<(), Failure>
where
    Callback: FnOnce() -> std::result::Result<(), Failure>,
{
    value()
}

trait Choose {
    type Error;
}

struct Chosen;

impl Choose for Chosen {
    #[allow(erased_error_signature)]
    type Error = Box<dyn std::error::Error>;
}

#[expect(erased_error_signature)]
fn projected() -> <Chosen as Choose>::Error {
    Box::new(std::io::Error::other("error"))
}

macro_rules! paired_signatures {
    ($error:ty) => {
        #[allow(erased_error_signature)]
        fn allowed() -> $error {
            unimplemented!()
        }

        #[expect(erased_error_signature)]
        fn rejected() -> $error {
            unimplemented!()
        }
    };
}

paired_signatures!(Box<dyn std::error::Error>);
