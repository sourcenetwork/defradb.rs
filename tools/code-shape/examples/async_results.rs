#![allow(dead_code)]
#![deny(unfulfilled_lint_expectations)]

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<Value> = std::result::Result<Value, Error>;

#[async_trait::async_trait]
pub trait ForeignError {
    async fn request(&self) -> std::io::Result<()>;
}

#[async_trait::async_trait]
pub trait CrateError {
    async fn request(&self) -> Result<()>;
}

#[async_trait::async_trait]
pub trait ExpectedSendContract {
    #[cfg_attr(
        all(dylint_lib = "defra_code_shape", code_shape_ui),
        expect(crate_result)
    )]
    async fn explicit_result(&self) -> std::result::Result<(), Error>;

    #[cfg_attr(
        all(dylint_lib = "defra_code_shape", code_shape_ui),
        expect(
            crate_result,
            reason = "fixture for a narrow contract expectation after async_trait expansion"
        )
    )]
    async fn request(&self) -> std::io::Result<()>;
}

#[async_trait::async_trait(?Send)]
pub trait ExpectedLocalContract {
    #[cfg_attr(
        all(dylint_lib = "defra_code_shape", code_shape_ui),
        expect(crate_result)
    )]
    async fn explicit_result(&self) -> std::result::Result<(), Error>;

    #[cfg_attr(
        all(dylint_lib = "defra_code_shape", code_shape_ui),
        expect(
            crate_result,
            reason = "fixture for the non-Send contract shape used on wasm"
        )
    )]
    async fn request(&self) -> std::io::Result<()>;
}

#[async_trait::async_trait]
pub trait Extension {
    type Error: Send;

    async fn request(&self) -> std::result::Result<(), Self::Error>;
}

type Task<'request> = std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + 'request>>;

pub fn task() -> Task<'static> {
    Box::pin(async { Ok(()) })
}

fn main() {}
