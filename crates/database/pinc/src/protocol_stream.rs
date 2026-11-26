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

use futures::FutureExt;
use pinc::{Hosted, RecvStream, SendStream};

use p2p::error::{Error, Result};
use p2p::rt::{mpsc, oneshot};

/// Code a protocol stream is reset or stopped with when its half is dropped
/// before it was finished: the handler or the opener gave up, or the node
/// cancelled the stream at its deadline, on a closed connection or at
/// shutdown.
pub const RESET_CANCELLED: u64 = 7;

/// Writes a send half may have waiting for its pump.
const SEND_QUEUE: usize = 4;
/// Chunks a receive half may have waiting to be read.
const RECV_QUEUE: usize = 4;
/// The largest chunk a receive pump reads at once.
const RECV_CHUNK: usize = 64 * 1024;

enum SendOp {
    Write(Vec<u8>, oneshot::Sender<Result<()>>),
    Finish(oneshot::Sender<Result<()>>),
}

/// The send half of a protocol stream. Dropping it unfinished resets the
/// stream with [`RESET_CANCELLED`].
pub struct ProtocolSendStream {
    ops: Option<mpsc::Sender<SendOp>>,
    /// Dropped with this half, which tells its pump to reset the stream
    /// even while it waits on the peer.
    _dropped: oneshot::Sender<()>,
}

impl ProtocolSendStream {
    /// Writes all of `bytes`, returning once the stream has taken them.
    pub async fn write_all(&mut self, bytes: &[u8]) -> Result<()> {
        self.write_vec(bytes.to_vec()).await
    }

    /// [`write_all`](Self::write_all) of bytes the caller already owns.
    pub(super) async fn write_vec(&mut self, bytes: Vec<u8>) -> Result<()> {
        let ops = self.ops.as_ref().ok_or(Error::ConnectionClosed)?;
        let (done, wait) = oneshot::channel();
        ops.send(SendOp::Write(bytes, done))
            .await
            .map_err(|_| Error::ConnectionClosed)?;
        wait.await.map_err(|_| Error::ConnectionClosed)?
    }

    /// Ends the stream after what was written. Nothing more can be written
    /// afterwards.
    pub async fn finish(&mut self) -> Result<()> {
        let ops = self.ops.take().ok_or(Error::ConnectionClosed)?;
        let (done, wait) = oneshot::channel();
        ops.send(SendOp::Finish(done))
            .await
            .map_err(|_| Error::ConnectionClosed)?;
        wait.await.map_err(|_| Error::ConnectionClosed)?
    }
}

/// The receive half of a protocol stream. Dropping it stops the stream.
pub struct ProtocolRecvStream {
    chunks: mpsc::Receiver<Result<Vec<u8>>>,
    pending: Vec<u8>,
    read: usize,
    /// Dropped with this half, which tells its pump to stop the stream even
    /// while it waits on the peer.
    _dropped: oneshot::Sender<()>,
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

    /// Reads to the end of the stream, refusing more than `limit` bytes.
    pub async fn read_to_end(&mut self, limit: usize) -> Result<Vec<u8>> {
        let mut out = self.pending.split_off(self.read);
        self.pending.clear();
        self.read = 0;
        loop {
            if out.len() > limit {
                return Err(Error::Codec(format!("stream is longer than {limit} bytes")));
            }
            match self.chunks.recv().await {
                Some(Ok(chunk)) => out.extend_from_slice(&chunk),
                Some(Err(error)) => return Err(error),
                None => return Ok(out),
            }
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
    let (chunks_tx, chunks) = mpsc::channel(RECV_QUEUE);
    let (send_dropped, send_dropped_rx) = oneshot::channel();
    let (recv_dropped, recv_dropped_rx) = oneshot::channel();
    (
        ProtocolSendStream {
            ops: Some(ops),
            _dropped: send_dropped,
        },
        ProtocolRecvStream {
            chunks,
            pending: Vec::new(),
            read: 0,
            _dropped: recv_dropped,
        },
        pump_send(send, ops_rx, send_dropped_rx),
        pump_recv(recv, chunks_tx, recv_dropped_rx),
    )
}

/// Run the send half's writes and its finish until it is done. A half
/// dropped unfinished, between two writes or in the middle of one, resets
/// the stream: the write in flight is abandoned with it.
async fn pump_send<H>(
    mut send: SendStream<H>,
    mut ops: mpsc::Receiver<SendOp>,
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
            Some(SendOp::Write(bytes, done)) => {
                let written = futures::select_biased! {
                    _ = dropped => None,
                    written = send.write_all(&bytes).fuse() => Some(written.map_err(stream_error)),
                };
                let Some(written) = written else { break };
                let failed = written.is_err();
                let _ = done.send(written);
                if failed {
                    return;
                }
            }
            Some(SendOp::Finish(done)) => {
                let finished = futures::select_biased! {
                    _ = dropped => None,
                    finished = send.finish().fuse() => Some(finished),
                };
                let Some(finished) = finished else { break };
                // A peer whose handler took what it needed and dropped its
                // receive half stops the stream with RESET_CANCELLED: our
                // side still ended where it meant to, so the finish holds.
                // Any other stop (busy, too large) is the peer refusing.
                let finished = match finished {
                    Err(error)
                        if matches!(error.kind(), pinc::ErrorKind::Stopped(RESET_CANCELLED)) =>
                    {
                        Ok(())
                    }
                    other => other.map_err(stream_error),
                };
                let _ = done.send(finished);
                return;
            }
            None => break,
        }
    }
    send.reset(RESET_CANCELLED).await;
}

async fn pump_recv<H>(
    mut recv: RecvStream<H>,
    chunks: mpsc::Sender<Result<Vec<u8>>>,
    dropped: oneshot::Receiver<()>,
) where
    H: Hosted,
{
    let mut dropped = dropped.fuse();
    loop {
        let read = futures::select_biased! {
            _ = dropped => None,
            read = recv.read_chunk(RECV_CHUNK).fuse() => Some(read),
        };
        let delivered = match read {
            None => false,
            Some(Ok(Some(chunk))) => chunks.send(Ok(chunk)).await.is_ok(),
            Some(Ok(None)) => return,
            Some(Err(error)) => {
                let _ = chunks.send(Err(stream_error(error))).await;
                return;
            }
        };
        if !delivered {
            recv.stop(RESET_CANCELLED).await;
            return;
        }
    }
}

fn stream_error(error: pinc::Error) -> Error {
    match error.kind() {
        pinc::ErrorKind::Stopped(code) | pinc::ErrorKind::Reset(code) => Error::StreamReset(code),
        pinc::ErrorKind::Closed { .. } | pinc::ErrorKind::Shutdown => Error::ConnectionClosed,
        _ => Error::Transport(error.to_string()),
    }
}
