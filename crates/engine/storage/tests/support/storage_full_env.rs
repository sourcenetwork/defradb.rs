use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use regolith::env::{
    Capabilities, Env, FileLock, FileMeta, JoinHandle, LogFile, ReadDir, ReadFile, StdEnv,
    WriteFile, WriteMode,
};

#[derive(Debug, Default)]
pub struct FullEnv {
    inner: StdEnv,
    pub full: Arc<AtomicBool>,
}

struct FullLog {
    inner: Box<dyn LogFile>,
    full: Arc<AtomicBool>,
}

impl LogFile for FullLog {
    fn write_all_at(&self, offset: u64, buf: &[u8]) -> io::Result<()> {
        self.inner.write_all_at(offset, buf)
    }
    fn write_all_vectored_at(&self, offset: u64, bufs: &[io::IoSlice<'_>]) -> io::Result<()> {
        self.inner.write_all_vectored_at(offset, bufs)
    }
    fn sync_data(&self) -> io::Result<()> {
        if self.full.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::StorageFull,
                "injected full storage",
            ));
        }
        self.inner.sync_data()
    }
    fn set_len(&self, len: u64) -> io::Result<()> {
        if self.full.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::StorageFull,
                "injected full storage during WAL rollback",
            ));
        }
        self.inner.set_len(len)
    }
    fn start_writeback(&self, offset: u64, len: u64) {
        self.inner.start_writeback(offset, len)
    }
}

impl Env for FullEnv {
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
        Ok(Box::new(FullLog {
            inner: self.inner.open_log(path, mode)?,
            full: Arc::clone(&self.full),
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
