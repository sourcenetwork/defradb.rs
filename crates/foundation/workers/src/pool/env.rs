//! A regolith [`Env`] that places the threads regolith starts through it.
//! regolith spawns its compaction threads (and the opt-in log writeback
//! thread) through `Env::spawn`, so wrapping the env the database opens with
//! places them by the pool's plan, on the CPUs workers leave free and at the
//! compaction priority, with no change to regolith. Every other call goes to
//! the wrapped env unchanged, its default methods included, so what that env
//! does for them is what runs.

use std::io;
use std::path::Path;
use std::sync::Arc;

use regolith::env::{
    Capabilities, DiskSpace, FileLock, FileMeta, JoinHandle, LogFile, ReadDir, ReadFile, WriteFile,
    WriteMode,
};
use regolith::Env;

use super::placer::Placer;
use crate::config::ThreadClass;

/// An [`Env`] whose spawned threads are placed as compaction threads. Made
/// by [`Pool::env`](crate::Pool::env).
pub struct PlacedEnv {
    inner: Arc<dyn Env>,
    placer: Arc<Placer>,
}

impl PlacedEnv {
    pub(crate) fn new(inner: Arc<dyn Env>, placer: Arc<Placer>) -> PlacedEnv {
        PlacedEnv { inner, placer }
    }
}

impl std::fmt::Debug for PlacedEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlacedEnv")
            .field("inner", &self.inner)
            .finish_non_exhaustive()
    }
}

impl Env for PlacedEnv {
    fn spawn(
        &self,
        name: &str,
        body: Box<dyn FnOnce() + Send + 'static>,
    ) -> io::Result<Box<dyn JoinHandle>> {
        let placer = self.placer.clone();
        self.inner.spawn(
            name,
            Box::new(move || {
                placer.place(ThreadClass::Compaction);
                body();
            }),
        )
    }

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
        self.inner.open_log(path, mode)
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

    fn disk_space(&self, path: &Path) -> io::Result<Option<DiskSpace>> {
        self.inner.disk_space(path)
    }

    fn exists(&self, path: &Path) -> bool {
        self.inner.exists(path)
    }

    fn is_dir(&self, path: &Path) -> bool {
        self.inner.is_dir(path)
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.inner.read(path)
    }

    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        self.inner.write(path, data)
    }

    fn drop_page_cache(&self, path: &Path) {
        self.inner.drop_page_cache(path)
    }
}
