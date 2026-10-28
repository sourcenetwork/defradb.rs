//! hyper's runtime hooks on nuclei: a `futures-io` stream as hyper's I/O,
//! an executor that spawns on nuclei, and a timer on nuclei's clock.

use std::future::Future;
use std::io;
use std::io::IoSlice;
use std::mem::MaybeUninit;
use std::pin::Pin;
use std::task::{ready, Context, Poll};
use std::time::{Duration, Instant};

use futures_io::{AsyncRead, AsyncWrite};
use hyper::rt::ReadBufCursor;

/// A `futures-io` stream (a nuclei socket, TLS over one) as hyper's I/O.
#[derive(Debug)]
pub struct Io<S> {
    inner: S,
}

impl<S> Io<S> {
    pub fn new(inner: S) -> Self {
        Self { inner }
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
        // SAFETY: the unfilled region is zeroed before it is lent to the
        // reader as initialized bytes, and the cursor then advances by no
        // more than the reader reports it wrote into that region.
        let read = unsafe {
            let region = buf.as_mut();
            region.fill(MaybeUninit::new(0));
            let bytes = &mut *(region as *mut [MaybeUninit<u8>] as *mut [u8]);
            ready!(Pin::new(&mut self.inner).poll_read(cx, bytes))?
        };
        // SAFETY: as above, `read` bytes of the region were written.
        unsafe { buf.advance(read) };
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
/// detached task on the nuclei runtime the caller runs on.
#[derive(Debug, Clone, Copy, Default)]
pub struct Executor;

impl<F> hyper::rt::Executor<F> for Executor
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    fn execute(&self, future: F) {
        nuclei::spawn(future).detach();
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
