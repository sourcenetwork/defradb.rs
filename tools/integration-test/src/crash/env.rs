use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;

use regolith::env::{
    Capabilities, DiskSpace, Env, FileLock, FileMeta, JoinHandle, LogFile, ReadDir, ReadFile,
    StdEnv, WriteFile, WriteMode,
};

use super::{kill_self, CrashPoint};

/// The standard environment, except that once a [`CrashPoint`] is armed the
/// write-ahead log kills the process when it reaches that point.
#[derive(Debug, Default)]
pub struct CrashEnv {
    inner: StdEnv,
    trigger: Arc<Trigger>,
}

#[derive(Debug, Default)]
struct Trigger {
    /// The armed point's code; 0 while nothing is armed.
    armed: AtomicU8,
    /// A log write has run since the point was armed.
    written: AtomicBool,
}

impl CrashEnv {
    /// From now on, the next commit's log kills the process at `point`.
    pub fn arm(&self, point: CrashPoint) {
        self.trigger.written.store(false, Ordering::Release);
        self.trigger.armed.store(point.code(), Ordering::Release);
    }
}

impl Trigger {
    fn armed(&self) -> Option<CrashPoint> {
        CrashPoint::of(self.armed.load(Ordering::Acquire))
    }
}

/// A write-ahead log file that checks the trigger around each write and sync.
struct CrashLog {
    inner: Box<dyn LogFile>,
    trigger: Arc<Trigger>,
}

impl CrashLog {
    fn before_write(&self, offset: u64, bytes: &[u8]) -> io::Result<()> {
        match self.trigger.armed() {
            Some(CrashPoint::BeforeLogWrite) => kill_self(),
            Some(CrashPoint::TornLogWrite) => {
                self.inner.write_all_at(offset, &bytes[..bytes.len() / 2])?;
                self.inner.sync_data()?;
                kill_self()
            }
            _ => {
                self.trigger.written.store(true, Ordering::Release);
                Ok(())
            }
        }
    }
}

impl LogFile for CrashLog {
    fn write_all_at(&self, offset: u64, buf: &[u8]) -> io::Result<()> {
        self.before_write(offset, buf)?;
        self.inner.write_all_at(offset, buf)
    }

    fn write_all_vectored_at(&self, offset: u64, bufs: &[io::IoSlice<'_>]) -> io::Result<()> {
        if self.trigger.armed().is_some() {
            let bytes: Vec<u8> = bufs.iter().flat_map(|buf| buf.iter().copied()).collect();
            self.before_write(offset, &bytes)?;
        }
        self.inner.write_all_vectored_at(offset, bufs)
    }

    fn sync_data(&self) -> io::Result<()> {
        self.inner.sync_data()?;
        if self.trigger.armed() == Some(CrashPoint::AfterLogSync)
            && self.trigger.written.load(Ordering::Acquire)
        {
            kill_self()
        }
        Ok(())
    }

    fn set_len(&self, len: u64) -> io::Result<()> {
        self.inner.set_len(len)
    }

    fn start_writeback(&self, offset: u64, len: u64) {
        self.inner.start_writeback(offset, len)
    }
}

/// Whether `path` is a write-ahead log file: one in the store's `wal`
/// directory.
fn is_wal(path: &Path) -> bool {
    path.parent()
        .and_then(Path::file_name)
        .is_some_and(|name| name == "wal")
}

impl Env for CrashEnv {
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
        let inner = self.inner.open_log(path, mode)?;
        if !is_wal(path) {
            return Ok(inner);
        }
        Ok(Box::new(CrashLog {
            inner,
            trigger: Arc::clone(&self.trigger),
        }))
    }

    fn metadata(&self, path: &Path) -> io::Result<FileMeta> {
        self.inner.metadata(path)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        self.inner.remove_file(path)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.inner.rename(from, to)
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

    fn disk_space(&self, path: &Path) -> io::Result<Option<DiskSpace>> {
        self.inner.disk_space(path)
    }

    fn drop_page_cache(&self, path: &Path) {
        self.inner.drop_page_cache(path)
    }
}
