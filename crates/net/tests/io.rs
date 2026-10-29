//! hyper reads a stream through `Io`: every byte arrives in order across
//! reads that wait, and no read lends the stream or fills more than the
//! cursor has room for.

use std::collections::VecDeque;
use std::io;
use std::mem::MaybeUninit;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use futures_io::AsyncRead;
use hyper::rt::{Read, ReadBuf};
use net::rt::Io;

/// A stream that plays a script: `None` waits once, `Some(bytes)` hands
/// out bytes as far as each read has room. It notes the room it was lent.
struct Scripted {
    steps: VecDeque<Option<Vec<u8>>>,
    lent: usize,
}

impl AsyncRead for Scripted {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        this.lent = buf.len();
        match this.steps.front_mut() {
            None => Poll::Ready(Ok(0)),
            Some(None) => {
                this.steps.pop_front();
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Some(Some(bytes)) => {
                let taken = bytes.len().min(buf.len());
                buf[..taken].copy_from_slice(&bytes[..taken]);
                bytes.drain(..taken);
                if bytes.is_empty() {
                    this.steps.pop_front();
                }
                Poll::Ready(Ok(taken))
            }
        }
    }
}

/// A stream that claims more bytes than it was lent.
struct Overstating;

impl AsyncRead for Overstating {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(Ok(buf.len() + 1))
    }
}

/// `count` bytes that differ from their neighbours, starting at `from`.
fn pattern(from: usize, count: usize) -> Vec<u8> {
    (from..from + count).map(|at| (at % 251) as u8).collect()
}

#[test]
fn bytes_arrive_in_order_across_waits_and_never_past_the_cursor() {
    let sizes = [3, 40_000, 1, 16 * 1024, 16 * 1024 + 1, 100_000, 5];
    let mut steps = VecDeque::new();
    let mut sent = 0;
    for size in sizes {
        steps.push_back(None);
        steps.push_back(Some(pattern(sent, size)));
        sent += size;
    }
    steps.push_back(None);
    let mut io = Io::new(Scripted { steps, lent: 0 });
    let rooms = [1, 7, 4096, 16 * 1024, 64 * 1024, 200_000];
    let mut cx = Context::from_waker(Waker::noop());
    let (mut received, mut waits) = (Vec::new(), 0);

    for poll in 0.. {
        let mut storage = vec![MaybeUninit::uninit(); rooms[poll % rooms.len()]];
        let mut buf = ReadBuf::uninit(&mut storage);
        let room = buf.unfilled().remaining();
        let polled = Pin::new(&mut io).poll_read(&mut cx, buf.unfilled());
        assert!(io.get_ref().lent <= room, "lent more than the cursor holds");
        assert!(buf.filled().len() <= room, "filled past the cursor");
        match polled {
            Poll::Pending => {
                assert!(buf.filled().is_empty(), "a read that waits fills nothing");
                waits += 1;
            }
            Poll::Ready(result) => {
                result.expect("the scripted stream does not fail");
                if buf.filled().is_empty() {
                    break;
                }
                received.extend_from_slice(buf.filled());
            }
        }
    }

    assert_eq!(waits, sizes.len() + 1);
    assert_eq!(received.len(), sent);
    assert!(received == pattern(0, sent), "the bytes came out of order");
    assert!(io.into_inner().steps.is_empty());
}

#[test]
fn a_stream_that_claims_more_than_it_was_lent_is_an_error() {
    let mut io = Io::new(Overstating);
    let mut storage = vec![MaybeUninit::uninit(); 64];
    let mut buf = ReadBuf::uninit(&mut storage);
    let mut cx = Context::from_waker(Waker::noop());

    let polled = Pin::new(&mut io).poll_read(&mut cx, buf.unfilled());

    let Poll::Ready(Err(error)) = polled else {
        panic!("an overstated read must fail");
    };
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(buf.filled().is_empty());
}
