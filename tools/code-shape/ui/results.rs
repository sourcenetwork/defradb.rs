// compile-flags: --crate-type=lib
#![allow(dead_code, async_fn_in_trait)]

pub fn foreign_error() -> std::io::Result<()> {
    Ok(())
}

pub async fn asynchronous() -> std::io::Result<()> {
    Ok(())
}

pub trait PublicApi {
    fn operation(&self) -> std::io::Result<()>;
    async fn asynchronous(&self) -> std::io::Result<()>;
}

pub struct Adapter;

impl Adapter {
    pub fn method(&self) -> std::io::Result<()> {
        Ok(())
    }
}

impl TryFrom<()> for Adapter {
    type Error = std::io::Error;

    fn try_from(_: ()) -> std::result::Result<Self, Self::Error> {
        Ok(Self)
    }
}

fn internal() -> std::io::Result<()> {
    Ok(())
}

pub fn explicit_future() -> impl std::future::Future<Output = std::io::Result<()>> {
    async { Ok(()) }
}

pub fn boxed_future() -> std::pin::Pin<Box<dyn std::future::Future<Output = std::io::Result<()>>>> {
    Box::pin(async { Ok(()) })
}

type ForeignFuture<'request> = std::pin::Pin<Box<dyn std::future::Future<Output = std::io::Result<()>> + 'request>>;

pub fn aliased_future() -> ForeignFuture<'static> {
    Box::pin(async { Ok(()) })
}

pub trait FutureApi {
    fn request(&self) -> impl std::future::Future<Output = std::io::Result<()>>;
}

pub trait Extension {
    type Error;

    fn request(&self) -> std::result::Result<(), Self::Error>;
    fn future(&self) -> impl std::future::Future<Output = std::result::Result<(), Self::Error>>;
}

pub fn rejected_payload<Payload>(payload: Payload) -> std::result::Result<(), Payload> {
    Err(payload)
}

pub fn generic_future<Work>(work: Work) -> Work
where
    Work: std::future::Future<Output = std::io::Result<()>>,
{
    work
}

pub fn forwarded_future<Work, Failure>(work: Work) -> Work
where
    Work: std::future::Future<Output = std::result::Result<(), Failure>>,
{
    work
}

pub fn borrowed_future<'request>(_: &'request ()) -> ForeignFuture<'request> {
    Box::pin(async { Ok(()) })
}

pub trait BorrowedFutureApi {
    fn borrowed<'request>(&'request self) -> ForeignFuture<'request>;
}

type ForwardedFuture<'request, Failure> = std::pin::Pin<
    Box<dyn std::future::Future<Output = std::result::Result<(), Failure>> + 'request>,
>;

pub trait BorrowedExtension {
    type Error;

    fn forwarded<'request>(&'request self) -> ForwardedFuture<'request, Self::Error>;
}

pub fn concrete_projection() -> std::result::Result<(), <u32 as std::str::FromStr>::Err> {
    Ok(())
}

pub async fn asynchronous_projection() -> std::result::Result<(), <u32 as std::str::FromStr>::Err> {
    Ok(())
}

pub fn opaque_projection() -> impl std::future::Future<Output = std::result::Result<(), <u32 as std::str::FromStr>::Err>> {
    async { Ok(()) }
}

pub fn named_projection() -> ForwardedFuture<'static, <u32 as std::str::FromStr>::Err> {
    Box::pin(async { Ok(()) })
}

impl PublicApi for Adapter {
    fn operation(&self) -> std::io::Result<()> { Ok(()) }
    async fn asynchronous(&self) -> std::io::Result<()> { Ok(()) }
}

impl Extension for Adapter {
    type Error = std::io::Error;

    fn request(&self) -> std::result::Result<(), Self::Error> { Ok(()) }
    fn future(&self) -> impl std::future::Future<Output = std::result::Result<(), Self::Error>> {
        async { Ok(()) }
    }
}

impl BorrowedExtension for Adapter {
    type Error = std::io::Error;

    fn forwarded<'request>(&'request self) -> ForwardedFuture<'request, Self::Error> {
        Box::pin(async { Ok(()) })
    }
}

pub trait FutureChoice { type Chosen; }

impl FutureChoice for Adapter { type Chosen = ForeignFuture<'static>; }

impl Adapter {
    pub fn projected_future(&self) -> <Self as FutureChoice>::Chosen {
        Box::pin(async { Ok(()) })
    }
}

pub struct Rejected;

impl Rejected {
    pub fn recover<Value>(self) -> std::result::Result<Value, Self> { Err(self) }
    pub fn borrowed_rejection<Value>(&self) -> std::result::Result<Value, Self> { Err(Self) }
}

pub struct Wrapper<Inner> { inner: Inner }

impl<Inner> std::future::Future for Wrapper<Inner> {
    type Output = std::io::Result<()>;

    fn poll(self: std::pin::Pin<&mut Self>, _: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        std::task::Poll::Ready(Ok(()))
    }
}

pub fn wrapper_future() -> Wrapper<std::pin::Pin<Box<dyn std::future::Future<Output = ()>>>> {
    Wrapper { inner: Box::pin(async {}) }
}

mod hidden {
    pub struct Reachable;

    impl Reachable {
        pub fn fallible(&self) -> std::io::Result<()> { Ok(()) }
    }

    pub fn internal() -> std::io::Result<()> { Ok(()) }
}

pub fn reachable() -> hidden::Reachable { hidden::Reachable }
