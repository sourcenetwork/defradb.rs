//! hyper's runtime hooks on nuclei: a `futures-io` stream as hyper's I/O,
//! an executor that spawns on the network edge, and a timer on nuclei's
//! clock.

use std::fmt;
use std::future::Future;
use std::io;
use std::io::IoSlice;
use std::pin::Pin;
use std::task::{ready, Context, Poll};
use std::time::{Duration, Instant};

use edge::Edge;
use futures_io::{AsyncRead, AsyncWrite};
use hyper::rt::ReadBufCursor;

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

/// Spawns hyper's background work (a connection, an HTTP/2 stream) as a
/// detached task on the network edge it was given. Each runs as long as the
/// client or server it belongs to holds its connection, or until the edge
/// stops accepting, so an idle pooled connection never holds up the edge's
/// shutdown.
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
        self.edge.spawn_service(future).detach();
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
