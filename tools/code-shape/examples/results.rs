#![allow(dead_code, async_fn_in_trait)]
#![deny(unfulfilled_lint_expectations)]

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T, Failure = Error> = std::result::Result<T, Failure>;

pub trait ErrorChoice {
    type Failure;
}
pub struct OwnedError;
impl ErrorChoice for OwnedError {
    type Failure = Error;
}

pub fn owned_projection() -> Result<(), <OwnedError as ErrorChoice>::Failure> {
    Ok(())
}
pub async fn owned_async_projection() -> Result<(), <OwnedError as ErrorChoice>::Failure> {
    Ok(())
}
pub fn owned_future_projection()
-> impl std::future::Future<Output = Result<(), <OwnedError as ErrorChoice>::Failure>> {
    async { Ok(()) }
}

fn internal() -> std::io::Result<()> {
    Ok(())
}

pub fn boundary() -> Result<()> {
    internal()?;
    Ok(())
}

pub async fn asynchronous() -> Result<()> {
    boundary()
}

#[cfg(not(test))]
#[cfg_attr(
    all(dylint_lib = "defra_code_shape", code_shape_ui),
    expect(crate_result)
)]
pub async fn explicit_async_result() -> std::result::Result<(), Error> {
    Ok(())
}

#[cfg(not(test))]
#[cfg_attr(
    all(dylint_lib = "defra_code_shape", code_shape_ui),
    expect(crate_result)
)]
pub fn explicit_future_result() -> impl std::future::Future<Output = std::result::Result<(), Error>>
{
    async { Ok(()) }
}

#[cfg(not(test))]
pub trait ExplicitAsyncApi {
    #[cfg_attr(
        all(dylint_lib = "defra_code_shape", code_shape_ui),
        expect(crate_result)
    )]
    async fn operation(&self) -> std::result::Result<(), Error>;
}

#[cfg(not(test))]
#[cfg_attr(
    all(dylint_lib = "defra_code_shape", code_shape_ui),
    expect(crate_result)
)]
pub fn explicit_boxed_result()
-> std::pin::Pin<Box<dyn std::future::Future<Output = std::result::Result<(), Error>>>> {
    Box::pin(async { Ok(()) })
}

type NamedFuture = std::future::Ready<std::result::Result<(), Error>>;

pub fn named_future() -> NamedFuture {
    std::future::ready(Ok(()))
}

#[cfg(not(test))]
#[cfg_attr(
    all(dylint_lib = "defra_code_shape", code_shape_ui),
    expect(crate_result)
)]
pub fn ready_with_explicit_result() -> std::future::Ready<std::result::Result<(), Error>> {
    std::future::ready(Ok(()))
}

pub fn ready_with_owned_result() -> std::future::Ready<Result<()>> {
    std::future::ready(Ok(()))
}

pub struct Completion<Value>(Value);

impl<Value> std::future::Future for Completion<Value> {
    type Output = Value;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        std::task::Poll::Pending
    }
}

#[cfg(not(test))]
#[cfg_attr(
    all(dylint_lib = "defra_code_shape", code_shape_ui),
    expect(crate_result)
)]
pub fn completion_with_explicit_result() -> Completion<std::result::Result<(), Error>> {
    Completion(Ok(()))
}

pub fn completion_with_owned_result() -> Completion<Result<()>> {
    Completion(Ok(()))
}

pub fn future_work<Work>(work: Work) -> Work
where
    Work: std::future::Future<Output = Result<()>>,
{
    work
}

type BorrowedFuture<'request> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + 'request>>;

pub fn borrowed_future<'request>(_: &'request ()) -> BorrowedFuture<'request> {
    Box::pin(async { Ok(()) })
}

pub trait BorrowedFutureApi {
    fn borrowed<'request>(&'request self) -> BorrowedFuture<'request>;
}

pub trait PublicApi {
    fn operation(&self) -> Result<()>;
    async fn asynchronous(&self) -> Result<()>;
}

pub struct Adapter;

impl Adapter {
    pub fn method(&self) -> Result<()> {
        boundary()
    }
}

impl TryFrom<()> for Adapter {
    type Error = std::io::Error;

    fn try_from(_: ()) -> std::result::Result<Self, Self::Error> {
        Ok(Self)
    }
}

#[test]
fn crate_result() -> Result<()> {
    boundary()
}

#[test]
fn unit() {
    assert!(boundary().is_ok());
}

#[cfg(not(test))]
#[cfg_attr(
    all(dylint_lib = "defra_code_shape", code_shape_ui),
    expect(crate_result)
)]
pub fn return_only_error<Failure>() -> std::result::Result<(), Failure>
where
    Failure: From<std::io::Error>,
{
    Err(std::io::Error::other("owned failure").into())
}

#[cfg(not(test))]
#[cfg_attr(
    all(dylint_lib = "defra_code_shape", code_shape_ui),
    expect(crate_result)
)]
pub async fn return_only_async_error<Failure>() -> std::result::Result<(), Failure>
where
    Failure: From<std::io::Error>,
{
    return_only_error()
}

#[cfg(not(test))]
#[cfg_attr(
    all(dylint_lib = "defra_code_shape", code_shape_ui),
    expect(crate_result)
)]
pub fn return_only_future_error<Failure>()
-> impl std::future::Future<Output = std::result::Result<(), Failure>>
where
    Failure: From<std::io::Error>,
{
    async { return_only_error() }
}

pub struct ReturnOnlyApi<Failure>(std::marker::PhantomData<Failure>);

impl<Failure> ReturnOnlyApi<Failure>
where
    Failure: From<std::io::Error>,
{
    #[cfg(not(test))]
    #[cfg_attr(
        all(dylint_lib = "defra_code_shape", code_shape_ui),
        expect(crate_result)
    )]
    pub fn direct() -> std::result::Result<(), Failure> {
        Err(std::io::Error::other("owned failure").into())
    }

    #[cfg(not(test))]
    #[cfg_attr(
        all(dylint_lib = "defra_code_shape", code_shape_ui),
        expect(crate_result)
    )]
    pub async fn asynchronous() -> std::result::Result<(), Failure> {
        Self::direct()
    }

    #[cfg(not(test))]
    #[cfg_attr(
        all(dylint_lib = "defra_code_shape", code_shape_ui),
        expect(crate_result)
    )]
    pub fn future() -> impl std::future::Future<Output = std::result::Result<(), Failure>> {
        async { Self::direct() }
    }
}

pub trait CallerErrorContract<Failure> {
    fn attempt() -> std::result::Result<(), Failure>;
}

pub trait FallibleWork<Failure>: FnOnce() -> std::result::Result<(), Failure> {}

impl<Work, Failure> FallibleWork<Failure> for Work where
    Work: FnOnce() -> std::result::Result<(), Failure>
{
}

pub fn caller_work<Work, Failure>(work: Work) -> std::result::Result<(), Failure>
where
    Work: FallibleWork<Failure>,
{
    work()
}

pub fn caller_future<Work, Failure>(work: Work) -> Work
where
    Work: std::future::Future<Output = std::result::Result<(), Failure>>,
    Failure: From<std::io::Error>,
{
    work
}

pub trait FutureFactory<Work>: FnOnce() -> Work {}

impl<Factory, Work> FutureFactory<Work> for Factory where Factory: FnOnce() -> Work {}

pub fn caller_future_factory<Factory, Work, Failure>(
    factory: Factory,
) -> impl std::future::Future<Output = std::result::Result<(), Failure>>
where
    Factory: FutureFactory<Work>,
    Work: std::future::Future<Output = std::result::Result<(), Failure>>,
{
    factory()
}

pub trait ErrorProvider {
    type Failure;
    fn provide() -> std::result::Result<(), Self::Failure>;
}

pub fn caller_provider<Provider, Failure>() -> std::result::Result<(), Failure>
where
    Provider: ErrorProvider<Failure = Failure>,
{
    Provider::provide()
}

pub struct ProviderApi<Provider, Failure>(std::marker::PhantomData<(Provider, Failure)>);

impl<Provider, Failure> ProviderApi<Provider, Failure>
where
    Provider: ErrorProvider<Failure = Failure>,
{
    pub fn provide() -> std::result::Result<(), Failure> {
        Provider::provide()
    }
}

pub struct ErrorHolder<Failure>(pub Failure);

impl<Failure> ErrorHolder<Failure> {
    pub fn return_error(self) -> std::result::Result<(), Failure> {
        Err(self.0)
    }
}

fn main() {}

pub fn optional_owned() -> Option<Result<()>> {
    Some(Ok(()))
}
pub fn owned_batch() -> Result<Vec<Result<()>>> {
    Ok(vec![Ok(())])
}
pub fn owned_pair() -> (Result<()>, Result<()>) {
    (Ok(()), Ok(()))
}
pub fn optional_caller<Failure>(error: Failure) -> Option<std::result::Result<(), Failure>> {
    Some(Err(error))
}

#[cfg(not(test))]
#[cfg_attr(
    all(dylint_lib = "defra_code_shape", code_shape_ui),
    expect(crate_result)
)]
pub fn foreign_optional() -> Option<std::io::Result<()>> {
    Some(Ok(()))
}

#[cfg(not(test))]
#[cfg_attr(
    all(dylint_lib = "defra_code_shape", code_shape_ui),
    expect(crate_result)
)]
pub fn foreign_batch() -> Vec<std::io::Result<()>> {
    vec![Ok(())]
}

#[cfg(not(test))]
#[cfg_attr(
    all(dylint_lib = "defra_code_shape", code_shape_ui),
    expect(crate_result)
)]
pub fn foreign_inner_result() -> Result<Vec<std::io::Result<()>>> {
    Ok(vec![Ok(())])
}

#[cfg(not(test))]
#[cfg_attr(
    all(dylint_lib = "defra_code_shape", code_shape_ui),
    expect(crate_result)
)]
pub fn spelled_optional() -> Option<std::result::Result<(), Error>> {
    Some(Ok(()))
}

#[cfg(not(test))]
#[cfg_attr(
    all(dylint_lib = "defra_code_shape", code_shape_ui),
    expect(crate_result)
)]
pub fn spelled_pair() -> (
    std::result::Result<(), Error>,
    std::result::Result<(), Error>,
) {
    (Ok(()), Ok(()))
}

#[cfg(not(test))]
#[cfg_attr(
    all(dylint_lib = "defra_code_shape", code_shape_ui),
    expect(crate_result)
)]
pub async fn foreign_async_optional() -> Option<std::io::Result<()>> {
    Some(Ok(()))
}

#[cfg(not(test))]
#[cfg_attr(
    all(dylint_lib = "defra_code_shape", code_shape_ui),
    expect(crate_result)
)]
pub fn foreign_future_optional() -> impl std::future::Future<Output = Option<std::io::Result<()>>> {
    async { Some(Ok(())) }
}

#[cfg(not(test))]
#[cfg_attr(
    all(dylint_lib = "defra_code_shape", code_shape_ui),
    expect(crate_result)
)]
pub fn return_only_optional<Failure>() -> Option<std::result::Result<(), Failure>>
where
    Failure: From<std::io::Error>,
{
    Some(Err(std::io::Error::other("owned failure").into()))
}

pub fn result_marker() -> std::marker::PhantomData<std::io::Result<()>> {
    std::marker::PhantomData
}
pub fn result_sender() -> std::sync::mpsc::Sender<std::io::Result<()>> {
    std::sync::mpsc::channel().0
}

pub struct MarkerCodec<Failure>(std::marker::PhantomData<Failure>);
impl<Failure> MarkerCodec<Failure>
where
    Failure: From<std::io::Error>,
{
    #[cfg(not(test))]
    #[cfg_attr(
        all(dylint_lib = "defra_code_shape", code_shape_ui),
        expect(
            crate_result,
            reason = "a marker receiver cannot supply a caller refusal"
        )
    )]
    pub fn decode(&self, _: &[u8]) -> std::result::Result<u32, Failure> {
        Err(std::io::Error::other("decoder failed").into())
    }
}

#[cfg(not(test))]
#[cfg_attr(
    all(dylint_lib = "defra_code_shape", code_shape_ui),
    expect(
        crate_result,
        reason = "a phantom argument cannot supply a caller refusal"
    )
)]
pub fn marker_argument<Failure>(
    _: &[u8],
    _: std::marker::PhantomData<Failure>,
) -> std::result::Result<u32, Failure>
where
    Failure: From<std::io::Error>,
{
    Err(std::io::Error::other("decoder failed").into())
}

pub struct OwnedCompletion;
impl std::future::Future for OwnedCompletion {
    type Output = crate::Result<()>;
    fn poll(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        std::task::Poll::Ready(Ok(()))
    }
}

pub fn sender_and_owned_completion() -> (
    std::sync::mpsc::Sender<std::io::Result<()>>,
    OwnedCompletion,
) {
    (std::sync::mpsc::channel().0, OwnedCompletion)
}

type OtherOwnedResult<T> = std::result::Result<T, Error>;
pub fn sender_of_owned_result() -> (
    std::sync::mpsc::Sender<OtherOwnedResult<()>>,
    OwnedCompletion,
) {
    (std::sync::mpsc::channel().0, OwnedCompletion)
}

#[cfg(not(test))]
#[cfg_attr(
    all(dylint_lib = "defra_code_shape", code_shape_ui),
    expect(
        crate_result,
        reason = "a borrowed future's visible alias must still be owned"
    )
)]
pub fn borrowed_foreign_alias<'request>(
    value: &'request (),
) -> (
    usize,
    std::pin::Pin<Box<dyn std::future::Future<Output = OtherOwnedResult<&'request ()>> + 'request>>,
) {
    (0, Box::pin(async move { Ok(value) }))
}

type CallerFuture<Value, Failure> =
    std::pin::Pin<Box<dyn std::future::Future<Output = std::result::Result<Value, Failure>>>>;
type CallerFactory<Failure> = Box<dyn FnOnce() -> CallerFuture<(), Failure>>;
type ErrorConsumer<Failure> = Box<dyn Fn(Failure)>;

pub struct Pending<Value, Failure> {
    future: CallerFuture<Value, Failure>,
}
impl<Value, Failure> Pending<Value, Failure> {
    pub async fn wait(self) -> std::result::Result<Value, Failure> {
        self.future.await
    }
}
pub fn forward_object_future<Failure>(
    work: CallerFuture<(), Failure>,
) -> CallerFuture<(), Failure> {
    work
}
pub fn forward_object_factory<Failure>(make: CallerFactory<Failure>) -> CallerFuture<(), Failure> {
    make()
}

pub trait ObjectErrorContract<Failure> {
    fn refusal(&self) -> std::result::Result<(), Failure>;
}
pub fn forward_object_contract<Failure>(
    work: &dyn ObjectErrorContract<Failure>,
) -> std::result::Result<(), Failure> {
    work.refusal()
}

#[cfg(not(test))]
#[cfg_attr(
    all(dylint_lib = "defra_code_shape", code_shape_ui),
    expect(
        crate_result,
        reason = "a consumed parameter is not a caller-produced refusal"
    )
)]
pub fn consume_error_only<Failure>(_: ErrorConsumer<Failure>) -> std::result::Result<(), Failure>
where
    Failure: From<std::io::Error>,
{
    Err(std::io::Error::other("owned refusal").into())
}

#[cfg(not(test))]
#[cfg_attr(
    all(dylint_lib = "defra_code_shape", code_shape_ui),
    expect(
        crate_result,
        reason = "a successful-only future does not supply the returned refusal"
    )
)]
pub fn successful_object_only<Failure>(
    _: std::pin::Pin<Box<dyn std::future::Future<Output = ()>>>,
) -> std::result::Result<(), Failure>
where
    Failure: From<std::io::Error>,
{
    Err(std::io::Error::other("owned refusal").into())
}

type Started<Message> = (std::sync::mpsc::Sender<Message>, Option<OwnedCompletion>);
pub fn started_sender_of_owned_result() -> Started<OtherOwnedResult<()>> {
    (std::sync::mpsc::channel().0, Some(OwnedCompletion))
}

type Tagged<Tag, Value> = (
    std::marker::PhantomData<Tag>,
    std::pin::Pin<Box<dyn std::future::Future<Output = Value>>>,
);
#[cfg(not(test))]
#[cfg_attr(
    all(dylint_lib = "defra_code_shape", code_shape_ui),
    expect(
        crate_result,
        reason = "a phantom alias cannot supply a future output's spelling"
    )
)]
pub fn tagged_misspelled_result() -> Tagged<crate::Result<()>, std::result::Result<(), Error>> {
    (std::marker::PhantomData, Box::pin(async { Ok(()) }))
}

pub trait EnabledArgument {}
impl EnabledArgument for OtherOwnedResult<()> {}

pub struct ConditionalCompletion<Marker> {
    marker: std::marker::PhantomData<Marker>,
}
impl<Marker> std::future::Future for ConditionalCompletion<Marker>
where
    Marker: EnabledArgument,
{
    type Output = crate::Result<()>;
    fn poll(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        std::task::Poll::Ready(Ok(()))
    }
}
impl std::future::Future for ConditionalCompletion<()> {
    type Output = ();

    fn poll(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        std::task::Poll::Ready(())
    }
}
pub fn conditional_owned_completion() -> ConditionalCompletion<OtherOwnedResult<()>> {
    ConditionalCompletion {
        marker: std::marker::PhantomData,
    }
}
type NestedConditional<Marker> = Option<ConditionalCompletion<Marker>>;
pub fn nested_conditional_owned_completion() -> NestedConditional<OtherOwnedResult<()>> {
    Some(conditional_owned_completion())
}

pub struct EventTag;
pub fn passive_foreign_event() -> (EventTag, std::ops::Bound<std::io::Result<()>>) {
    (EventTag, std::ops::Bound::Included(Ok(())))
}

#[cfg(not(test))]
#[cfg_attr(
    all(dylint_lib = "defra_code_shape", code_shape_ui),
    expect(
        crate_result,
        reason = "array and tuple wrappers retain an operation's foreign failure"
    )
)]
pub fn tagged_batch() -> [(EventTag, std::io::Result<()>); 2] {
    [(EventTag, Ok(())), (EventTag, Ok(()))]
}

type BoxedValue<Value> = std::pin::Pin<Box<dyn std::future::Future<Output = Value>>>;
pub trait ForeignBatchPort {
    #[cfg(not(test))]
    #[cfg_attr(
        all(dylint_lib = "defra_code_shape", code_shape_ui),
        expect(
            crate_result,
            reason = "an aliased future batch still delivers operation errors"
        )
    )]
    fn batch(&self) -> BoxedValue<Vec<std::io::Result<()>>> {
        Box::pin(async { vec![Ok(())] })
    }
}

pub struct ForeignOutputFuture;
impl std::future::Future for ForeignOutputFuture {
    type Output = std::io::Result<()>;
    fn poll(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        std::task::Poll::Ready(Ok(()))
    }
}
#[cfg(not(test))]
#[cfg_attr(
    all(dylint_lib = "defra_code_shape", code_shape_ui),
    expect(
        crate_result,
        reason = "an optional named future still delivers its foreign failure"
    )
)]
pub fn optional_foreign_future() -> Option<ForeignOutputFuture> {
    Some(ForeignOutputFuture)
}
