use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use regolith::env::{
    Capabilities, Env, FileLock, FileMeta, JoinHandle, LogFile, ReadDir, ReadFile, StdEnv,
    WriteFile, WriteMode,
};

/// The failure an armed [`FaultEnv`] gives every write-ahead-log sync.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    /// The device is full: `ENOSPC`, `ErrorKind::StorageFull`.
    StorageFull,
    /// The device failed the write: `EIO`.
    Io,
}

const OFF: u8 = 0;
const FULL: u8 = 1;
const EIO: u8 = 2;

/// The local filesystem, except that once armed every log sync and log
/// truncation fails with the armed [`Fault`]. A store that sees the failure
/// latches it until it is reopened, as regolith does after any failed sync.
#[derive(Debug, Default)]
pub struct FaultEnv {
    inner: StdEnv,
    armed: Arc<AtomicU8>,
}

impl FaultEnv {
    /// Fail every log sync from now on with `fault`.
    pub fn arm(&self, fault: Fault) {
        let code = match fault {
            Fault::StorageFull => FULL,
            Fault::Io => EIO,
        };
        self.armed.store(code, Ordering::Release);
    }

    /// Let log syncs through again.
    pub fn disarm(&self) {
        self.armed.store(OFF, Ordering::Release);
    }
}

fn injected(armed: &AtomicU8, what: &str) -> io::Result<()> {
    match armed.load(Ordering::Acquire) {
        FULL => Err(io::Error::new(
            io::ErrorKind::StorageFull,
            format!("injected full storage{what}"),
        )),
        // EIO on every unix defradb.rs runs on.
        EIO => Err(io::Error::from_raw_os_error(5)),
        _ => Ok(()),
    }
}

struct FaultLog {
    inner: Box<dyn LogFile>,
    armed: Arc<AtomicU8>,
}

impl LogFile for FaultLog {
    fn write_all_at(&self, offset: u64, buf: &[u8]) -> io::Result<()> {
        self.inner.write_all_at(offset, buf)
    }
    fn write_all_vectored_at(&self, offset: u64, bufs: &[io::IoSlice<'_>]) -> io::Result<()> {
        self.inner.write_all_vectored_at(offset, bufs)
    }
    fn sync_data(&self) -> io::Result<()> {
        injected(&self.armed, "")?;
        self.inner.sync_data()
    }
    fn set_len(&self, len: u64) -> io::Result<()> {
        injected(&self.armed, " during WAL rollback")?;
        self.inner.set_len(len)
    }
    fn start_writeback(&self, offset: u64, len: u64) {
        self.inner.start_writeback(offset, len)
    }
}

impl Env for FaultEnv {
    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        self.inner.create_dir_all(path)
    }
    fn read_dir(&self, path: &Path) -> io::Result<ReadDir<'_>> {
        self.inner.read_dir(path)
    }
    fn open_read(&self, path: &Path) -> io::Result<Box<dyn ReadFile>> {
        self.inner.open_read(path)
    }
    fn open_write(&self, path: &Path, mode: WriteMode) -> io::Result<Box<dyn WriteFile>> {
        self.inner.open_write(path, mode)
    }
    fn open_log(&self, path: &Path, mode: WriteMode) -> io::Result<Box<dyn LogFile>> {
        Ok(Box::new(FaultLog {
            inner: self.inner.open_log(path, mode)?,
            armed: Arc::clone(&self.armed),
        }))
    }
    fn metadata(&self, path: &Path) -> io::Result<FileMeta> {
        self.inner.metadata(path)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        self.inner.remove_file(path)
    }
    fn rename(&self, src: &Path, dst: &Path) -> io::Result<()> {
        self.inner.rename(src, dst)
    }
    fn hard_link(&self, src: &Path, dst: &Path) -> io::Result<()> {
        self.inner.hard_link(src, dst)
    }
    fn sync_dir(&self, path: &Path) -> io::Result<()> {
        self.inner.sync_dir(path)
    }
    fn lock_file(&self, path: &Path, exclusive: bool) -> io::Result<Box<dyn FileLock>> {
        self.inner.lock_file(path, exclusive)
    }
    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }
    fn now_micros(&self) -> Option<u64> {
        self.inner.now_micros()
    }
    fn unix_secs(&self) -> Option<u64> {
        self.inner.unix_secs()
    }
    fn spawn(
        &self,
        name: &str,
        body: Box<dyn FnOnce() + Send + 'static>,
    ) -> io::Result<Box<dyn JoinHandle>> {
        self.inner.spawn(name, body)
    }
}
