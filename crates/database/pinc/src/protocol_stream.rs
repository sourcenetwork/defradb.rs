//! The two halves of an embedder protocol's stream.
//!
//! A pinc stream cannot leave the endpoint's thread, and an embedder's code
//! runs anywhere, so each half is a `Send` handle over a bounded channel to a
//! pump on the endpoint's thread that owns the pinc half. Dropping a send half
//! before it is finished resets the stream, so a reply cut short never reads
//! as a whole one; dropping a receive half stops the stream. Each half
//! signals its pump when it is dropped, so this holds while the pump waits
//! on the peer: a send pump blocked writing to a peer that does not read is
//! reset the moment its half goes, as at a handler's deadline.
//!
//! A handler that returns stops the stream's receive half with
//! [`STOP_HANDLED`], and only that stop lets the opener's finish hold: a
//! stream cancelled at its deadline stops with [`RESET_CANCELLED`], which
//! fails a finish that meets it, so data the handler never took does not
//! read as delivered.

use futures::FutureExt;
use pinc::{Hosted, RecvStream, SendStream};

use p2p::error::{Error, Result};
use p2p::rt::{mpsc, oneshot};

/// Code a protocol stream is reset or stopped with when its half is dropped
/// before it was finished: the handler or the opener gave up, or the node
/// cancelled the stream at its deadline, on a closed connection or at
/// shutdown.
pub const RESET_CANCELLED: u64 = 7;

/// Code the serving node stops a protocol stream with when its handler
/// returned before reading to the end: the handler took what it needed. An
/// opener's finish that meets this stop holds; a write after it fails with
/// [`Error::StreamReset`] and this code.
pub const STOP_HANDLED: u64 = 8;

/// Writes a send half may have waiting for its pump.
const SEND_QUEUE: usize = 4;
/// Chunks a receive half may have waiting to be read.
const RECV_QUEUE: usize = 4;

enum SendOp {
    Write(Vec<u8>),
    Finish,
}

/// The send half of a protocol stream. Dropping it unfinished resets the
/// stream with [`RESET_CANCELLED`].
pub struct ProtocolSendStream {
    ops: Option<mpsc::Sender<SendOp>>,
    /// The pump's answer to each op, in the order they were sent.
    done: mpsc::Receiver<Result<()>>,
    /// Ops sent whose answer was not read: a caller that stopped waiting on
    /// a write leaves its answer for the next op to read first.
    unanswered: usize,
    /// Dropped with this half, which tells its pump to reset the stream
    /// even while it waits on the peer.
    _dropped: oneshot::Sender<()>,
}

impl ProtocolSendStream {
    /// Writes all of `bytes`, returning once the stream has taken them.
    pub async fn write_all(&mut self, bytes: &[u8]) -> Result<()> {
        self.write_vec(bytes.to_vec()).await
    }

    /// [`write_all`](Self::write_all) of bytes the caller already owns,
    /// handed to the pump without a copy.
    pub async fn write_vec(&mut self, bytes: Vec<u8>) -> Result<()> {
        self.run(SendOp::Write(bytes)).await
    }

    /// Ends the stream after what was written. Nothing more can be written
    /// afterwards.
    pub async fn finish(&mut self) -> Result<()> {
        let finished = self.run(SendOp::Finish).await;
        self.ops = None;
        finished
    }

    async fn run(&mut self, op: SendOp) -> Result<()> {
        while self.unanswered > 0 {
            let answer = self.done.recv().await.ok_or(Error::ConnectionClosed)?;
            self.unanswered -= 1;
            answer?;
        }
        let ops = self.ops.as_ref().ok_or(Error::ConnectionClosed)?;
        ops.send(op).await.map_err(|_| Error::ConnectionClosed)?;
        self.unanswered += 1;
        let answer = self.done.recv().await.ok_or(Error::ConnectionClosed)?;
        self.unanswered -= 1;
        answer
    }
}

/// The receive half of a protocol stream. Dropping it stops the stream with
/// [`RESET_CANCELLED`].
pub struct ProtocolRecvStream {
    chunks: mpsc::Receiver<Result<Vec<u8>>>,
    pending: Vec<u8>,
    read: usize,
    /// The code its pump stops the stream with. Dropped unsent with this
    /// half, it tells the pump to stop with [`RESET_CANCELLED`] even while
    /// it waits on the peer.
    stop: Option<oneshot::Sender<u64>>,
}

impl ProtocolRecvStream {
    /// Reads into `buf`: how many bytes, or `None` once the stream ended.
    pub async fn read(&mut self, buf: &mut [u8]) -> Result<Option<usize>> {
        if self.read == self.pending.len() {
            match self.chunks.recv().await {
                Some(Ok(chunk)) => {
                    self.pending = chunk;
                    self.read = 0;
                }
                Some(Err(error)) => return Err(error),
                None => return Ok(None),
            }
        }
        let n = buf.len().min(self.pending.len() - self.read);
        buf[..n].copy_from_slice(&self.pending[self.read..self.read + n]);
        self.read += n;
        Ok(Some(n))
    }

    /// The next bytes as they arrived, without a copy: what
    /// [`read`](Self::read) would hand out next, or `None` once the stream
    /// ended.
    pub async fn read_chunk(&mut self) -> Result<Option<Vec<u8>>> {
        if self.read < self.pending.len() {
            let rest = self.pending.split_off(self.read);
            self.pending.clear();
            self.read = 0;
            return Ok(Some(rest));
        }
        match self.chunks.recv().await {
            Some(Ok(chunk)) => Ok(Some(chunk)),
            Some(Err(error)) => Err(error),
            None => Ok(None),
        }
    }

    /// Reads to the end of the stream, refusing more than `limit` bytes.
    pub async fn read_to_end(&mut self, limit: usize) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        while let Some(chunk) = self.read_chunk().await? {
            if out.is_empty() {
                out = chunk;
            } else {
                out.extend_from_slice(&chunk);
            }
            if out.len() > limit {
                return Err(Error::Codec(format!("stream is longer than {limit} bytes")));
            }
        }
        Ok(out)
    }

    /// Drops this half as a handler that returned: the stream stops with
    /// [`STOP_HANDLED`] rather than [`RESET_CANCELLED`].
    pub(super) fn handled(mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(STOP_HANDLED);
        }
    }
}

/// Bridges the pinc halves of a stream to `Send` handles: the returned pumps
/// run on the endpoint's thread until their half is done.
pub(super) fn bridge<H>(
    send: SendStream<H>,
    recv: RecvStream<H>,
) -> (
    ProtocolSendStream,
    ProtocolRecvStream,
    impl std::future::Future<Output = ()>,
    impl std::future::Future<Output = ()>,
)
where
    H: Hosted,
{
    let (ops, ops_rx) = mpsc::channel(SEND_QUEUE);
    let (done_tx, done) = mpsc::channel(SEND_QUEUE);
    let (chunks_tx, chunks) = mpsc::channel(RECV_QUEUE);
    let (send_dropped, send_dropped_rx) = oneshot::channel();
    let (stop, stop_rx) = oneshot::channel();
    (
        ProtocolSendStream {
            ops: Some(ops),
            done,
            unanswered: 0,
            _dropped: send_dropped,
        },
        ProtocolRecvStream {
            chunks,
            pending: Vec::new(),
            read: 0,
            stop: Some(stop),
        },
        pump_send(send, ops_rx, done_tx, send_dropped_rx),
        pump_recv(recv, chunks_tx, stop_rx),
    )
}

/// Run the send half's writes and its finish until it is done. A half
/// dropped unfinished, between two writes or in the middle of one, resets
/// the stream: the write in flight is abandoned with it.
async fn pump_send<H>(
    mut send: SendStream<H>,
    mut ops: mpsc::Receiver<SendOp>,
    done: mpsc::Sender<Result<()>>,
    dropped: oneshot::Receiver<()>,
) where
    H: Hosted,
{
    let mut dropped = dropped.fuse();
    loop {
        let op = futures::select_biased! {
            _ = dropped => None,
            op = ops.recv().fuse() => op,
        };
        match op {
            Some(SendOp::Write(bytes)) => {
                let written = futures::select_biased! {
                    _ = dropped => None,
                    written = send.write_all(&bytes).fuse() => Some(written.map_err(stream_error)),
                };
                let Some(written) = written else { break };
                let failed = written.is_err();
                let _ = done.send(written).await;
                if failed {
                    return;
                }
            }
            Some(SendOp::Finish) => {
                let finished = futures::select_biased! {
                    _ = dropped => None,
                    finished = send.finish().fuse() => Some(finished),
                };
                let Some(finished) = finished else { break };
                let _ = done.send(finish_outcome(finished)).await;
                return;
            }
            None => break,
        }
    }
    send.reset(RESET_CANCELLED).await;
}

/// What a finish that ended with `finished` tells the caller. A peer whose
/// handler returned before reading to the end stops the stream with
/// [`STOP_HANDLED`]: our side still ended where it meant to, so the finish
/// holds. Any other stop fails it: [`RESET_CANCELLED`] is the peer
/// cancelling the stream at its deadline, with what was written untaken,
/// and the rest (busy, too large) are the peer refusing.
fn finish_outcome(finished: pinc::Result<()>) -> Result<()> {
    match finished {
        Err(error) if matches!(error.kind(), pinc::ErrorKind::Stopped(STOP_HANDLED)) => Ok(()),
        other => other.map_err(stream_error),
    }
}

async fn pump_recv<H>(
    mut recv: RecvStream<H>,
    chunks: mpsc::Sender<Result<Vec<u8>>>,
    stop: oneshot::Receiver<u64>,
) where
    H: Hosted,
{
    let mut stop = stop.fuse();
    let code = loop {
        let mut chunk = Vec::new();
        let read = futures::select_biased! {
            code = stop => Err(code.unwrap_or(RESET_CANCELLED)),
            read = recv.read_with(|bytes| {
                chunk.extend_from_slice(bytes);
                bytes.len()
            }).fuse() => Ok(read),
        };
        match read {
            Err(code) => break code,
            Ok(Ok(Some(_))) => {
                if chunks.send(Ok(chunk)).await.is_err() {
                    // The half went, its stop code sent or dropped with it.
                    break (&mut stop).await.unwrap_or(RESET_CANCELLED);
                }
            }
            Ok(Ok(None)) => return,
            Ok(Err(error)) => {
                let _ = chunks.send(Err(stream_error(error))).await;
                return;
            }
        }
    };
    recv.stop(code).await;
}

fn stream_error(error: pinc::Error) -> Error {
    match error.kind() {
        pinc::ErrorKind::Stopped(code) | pinc::ErrorKind::Reset(code) => Error::StreamReset(code),
        pinc::ErrorKind::Closed { .. } | pinc::ErrorKind::Shutdown => Error::ConnectionClosed,
        _ => Error::Transport(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stopped(code: u64) -> pinc::Result<()> {
        Err(pinc::Error::new(pinc::ErrorKind::Stopped(code), "stopped"))
    }

    #[test]
    fn only_a_handled_stop_lets_a_finish_hold() {
        assert!(finish_outcome(Ok(())).is_ok());
        assert!(finish_outcome(stopped(STOP_HANDLED)).is_ok());
        assert!(matches!(
            finish_outcome(stopped(RESET_CANCELLED)),
            Err(Error::StreamReset(RESET_CANCELLED))
        ));
    }
}
