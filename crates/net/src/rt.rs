//! hyper's runtime hooks on nuclei: a `futures-io` stream as hyper's I/O,
//! an executor that spawns on the network edge, and a timer on nuclei's
//! clock.

use std::fmt;
use std::future::Future;
use std::io;
use std::io::IoSlice;
use std::pin::{pin, Pin};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{ready, Context, Poll};
use std::time::{Duration, Instant};

use edge::Edge;
use futures_io::{AsyncRead, AsyncWrite};
use hyper::rt::ReadBufCursor;
use regolith::sync::Notify;

/// The most one read takes: one of the buffer classes nuclei's driver reads
/// ahead into, a whole TLS record's plaintext, and a default-size HTTP/2
/// frame. Every open connection holds one.
const SCRATCH: usize = 16 * 1024;

/// A `futures-io` stream (a nuclei socket, TLS over one) as hyper's I/O.
pub struct Io<S> {
    inner: S,
    /// What a read lends the stream, allocated on the first read: hyper's
    /// cursor is uninitialized memory, and the stream takes only `&mut [u8]`.
    scratch: Box<[u8]>,
}

impl<S> fmt::Debug for Io<S>
where
    S: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Io")
            .field("inner", &self.inner)
            .finish_non_exhaustive()
    }
}

impl<S> Io<S> {
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            scratch: Box::default(),
        }
    }

    pub fn get_ref(&self) -> &S {
        &self.inner
    }

    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<S> hyper::rt::Read for Io<S>
where
    S: AsyncRead + Unpin,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        mut buf: ReadBufCursor<'_>,
    ) -> Poll<io::Result<()>> {
        let this = &mut *self;
        if this.scratch.is_empty() {
            this.scratch = vec![0; SCRATCH].into_boxed_slice();
        }
        let room = buf.remaining().min(this.scratch.len());
        let read = ready!(Pin::new(&mut this.inner).poll_read(cx, &mut this.scratch[..room]))?;
        if read > room {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the connection reported more bytes than it was given room for",
            )));
        }
        buf.put_slice(&this.scratch[..read]);
        Poll::Ready(Ok(()))
    }
}

impl<S> hyper::rt::Write for Io<S>
where
    S: AsyncWrite + Unpin,
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_close(cx)
    }

    fn is_write_vectored(&self) -> bool {
        true
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write_vectored(cx, bufs)
    }
}

/// Spawns hyper's background work for a server (an HTTP/2 stream) as a task
/// of the network edge it was given. The edge's drain waits for it, bounded
/// by the drain's deadline, so a stream in flight finishes when the server
/// stops accepting.
#[derive(Debug, Clone)]
pub struct Executor {
    edge: Edge,
}

impl Executor {
    pub fn new(edge: Edge) -> Self {
        Self { edge }
    }
}

impl<F> hyper::rt::Executor<F> for Executor
where
    F: Future<Output = ()> + Send + 'static,
{
    fn execute(&self, future: F) {
        drop(self.edge.spawn(future));
    }
}

/// The requests a client has in flight: each from the moment it is sent to
/// the end of its response, dropped or read. Cloning shares the count.
#[derive(Debug, Clone, Default)]
pub struct Inflight(Arc<Open>);

#[derive(Debug, Default)]
struct Open {
    count: AtomicUsize,
    ended: Notify,
}

impl Inflight {
    /// One more request in flight, until the guard is dropped.
    pub fn begin(&self) -> Busy {
        self.0.count.fetch_add(1, Ordering::SeqCst);
        Busy(Arc::clone(&self.0))
    }

    /// Completes once no request is in flight.
    async fn idle(&self) {
        loop {
            let ended = self.0.ended.notified();
            if self.0.count.load(Ordering::SeqCst) == 0 {
                return;
            }
            ended.await;
        }
    }
}

/// A request in flight. Dropping it, when its response ended however it
/// ended, tells the client's connection drivers the request is over.
#[derive(Debug)]
pub struct Busy(Arc<Open>);

impl Drop for Busy {
    fn drop(&mut self) {
        if self.0.count.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.0.ended.notify_waiters();
        }
    }
}

/// Spawns hyper's background work for a client (a connection driver, the
/// pool's idle sweep) as a task of the network edge it was given. Once the
/// edge stops accepting, a driver goes on while any request of the client is
/// in flight, so its response completes, and ends when none is: an idle
/// pooled connection never holds up the edge's shutdown.
#[derive(Debug, Clone)]
pub struct ClientExecutor {
    edge: Edge,
    inflight: Inflight,
}

impl ClientExecutor {
    pub fn new(edge: Edge, inflight: Inflight) -> Self {
        Self { edge, inflight }
    }
}

impl<F> hyper::rt::Executor<F> for ClientExecutor
where
    F: Future<Output = ()> + Send + 'static,
{
    fn execute(&self, future: F) {
        let closing = self.edge.closing();
        let inflight = self.inflight.clone();
        drop(self.edge.spawn(async move {
            let mut driver = pin!(future);
            nuclei::select! {
                () = driver.as_mut() => return,
                () = closing => {}
            }
            nuclei::select! {
                () = driver.as_mut() => {}
                () = inflight.idle() => {}
            }
        }));
    }
}

/// A response body that keeps its request in flight until the body is
/// dropped.
#[derive(Debug)]
pub struct Tracked<B> {
    body: B,
    _busy: Busy,
}

impl<B> Tracked<B> {
    pub fn new(body: B, busy: Busy) -> Self {
        Self { body, _busy: busy }
    }
}

impl<B> http_body::Body for Tracked<B>
where
    B: http_body::Body + Unpin,
{
    type Data = B::Data;
    type Error = B::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        Pin::new(&mut self.body).poll_frame(cx)
    }

    fn is_end_stream(&self) -> bool {
        self.body.is_end_stream()
    }

    fn size_hint(&self) -> http_body::SizeHint {
        self.body.size_hint()
    }
}

/// hyper's timers on nuclei's clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct Timer;

impl hyper::rt::Timer for Timer {
    fn sleep(&self, duration: Duration) -> Pin<Box<dyn hyper::rt::Sleep>> {
        Box::pin(Sleep(nuclei::time::sleep(duration)))
    }

    fn sleep_until(&self, deadline: Instant) -> Pin<Box<dyn hyper::rt::Sleep>> {
        Box::pin(Sleep(nuclei::time::sleep_until(
            nuclei::time::Instant::from_std(deadline),
        )))
    }

    fn reset(&self, sleep: &mut Pin<Box<dyn hyper::rt::Sleep>>, deadline: Instant) {
        match sleep.as_mut().downcast_mut_pin::<Sleep>() {
            Some(ours) => ours.reset(deadline),
            None => *sleep = self.sleep_until(deadline),
        }
    }
}

struct Sleep(nuclei::time::Sleep);

impl Sleep {
    fn reset(self: Pin<&mut Self>, deadline: Instant) {
        Pin::new(&mut self.get_mut().0).reset(nuclei::time::Instant::from_std(deadline));
    }
}

impl Future for Sleep {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        Pin::new(&mut self.get_mut().0).poll(cx)
    }
}

impl hyper::rt::Sleep for Sleep {}
