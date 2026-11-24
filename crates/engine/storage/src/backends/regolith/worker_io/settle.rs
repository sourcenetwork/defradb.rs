//! Waiting out what a non-blocking regolith call hands back.

use regolith::{Error, TransactionError, WouldBlock};

/// Runs `call` until it stops asking to wait: on [`Error::WouldBlock`] it
/// awaits what the error carries (a read's `IoWait`, a stall's `StallWait`,
/// a key lock's `LockWait`) and calls again; any other outcome is returned.
/// The [`regolith::Error`] counterpart of `regolith::ready`.
pub(crate) async fn settle<T>(
    mut call: impl FnMut() -> regolith::Result<T>,
) -> regolith::Result<T> {
    loop {
        match call() {
            Err(Error::WouldBlock(would_block)) => wait_out(would_block).await?,
            other => return other,
        }
    }
}

/// [`settle`] for a call whose first run asked to wait on `would_block`.
pub(crate) async fn settle_after<T>(
    would_block: WouldBlock,
    call: impl FnMut() -> regolith::Result<T>,
) -> regolith::Result<T> {
    wait_out(would_block).await?;
    settle(call).await
}

/// `regolith::ready` for a call whose first run asked to wait on
/// `would_block`.
pub(crate) async fn ready_after<T>(
    would_block: WouldBlock,
    call: impl FnMut() -> Result<T, TransactionError>,
) -> Result<T, TransactionError> {
    match wait_out(would_block).await {
        Ok(()) => regolith::ready(call).await,
        Err(Error::WouldBlock(other)) => Err(TransactionError::WouldBlock(other)),
        Err(error) => Err(TransactionError::Engine(error)),
    }
}

/// Awaits what `would_block` asks to wait on; a kind this build does not know
/// is handed back as the error it came in.
pub(crate) async fn wait_out(would_block: WouldBlock) -> regolith::Result<()> {
    match would_block {
        WouldBlock::Io(wait) => wait.await,
        WouldBlock::Stall(wait) => wait.await,
        WouldBlock::Locked(wait) => wait.await,
        other => return Err(Error::WouldBlock(other)),
    }
    Ok(())
}

/// `settle(|| $call).await`, with the first call run in place: nearly every
/// call does not ask to wait, and only one that does builds the wait loop.
macro_rules! settled {
    ($call:expr) => {
        match $call {
            Err(regolith::Error::WouldBlock(would_block)) => {
                $crate::backends::regolith::worker_io::settle_after(would_block, || $call).await
            }
            done => done,
        }
    };
}

/// `regolith::ready(|| $call).await`, with the first call run in place, as
/// [`settled`] does.
macro_rules! readied {
    ($call:expr) => {
        match $call {
            Err(regolith::TransactionError::WouldBlock(would_block)) => {
                $crate::backends::regolith::worker_io::ready_after(would_block, || $call).await
            }
            done => done,
        }
    };
}

pub(crate) use {readied, settled};
