//! A worker's heartbeat: when its current poll started, which task it is,
//! and the task's name, written by the worker at every poll and read by the
//! monitor on another thread.
//!
//! It is a sequence lock over plain atomics: the worker bumps the sequence to
//! odd, writes the fields, and bumps it to even; a reader keeps a read only
//! if the sequence was even and unchanged across it, so the name's pointer
//! and length it keeps always come from the same `&'static str`.

use std::sync::atomic::{fence, AtomicPtr, AtomicU64, AtomicUsize, Ordering};

/// `start` while the worker is between polls.
const IDLE: u64 = u64::MAX;

/// One consistent reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Beat {
    /// When the poll started, on the pool's clock.
    pub start: u64,
    /// The task, as its key's bits, with [`REPORTED`] set when it was
    /// reported before.
    pub task: u64,
    pub name: &'static str,
}

/// Set in [`Beat::task`] for a task already reported as stuck.
pub(crate) const REPORTED: u64 = 1 << 63;

pub(crate) struct Heartbeat {
    seq: AtomicU64,
    start: AtomicU64,
    task: AtomicU64,
    name_ptr: AtomicPtr<u8>,
    name_len: AtomicUsize,
}

impl Heartbeat {
    pub(crate) fn new() -> Heartbeat {
        Heartbeat {
            seq: AtomicU64::new(0),
            start: AtomicU64::new(IDLE),
            task: AtomicU64::new(0),
            name_ptr: AtomicPtr::new("".as_ptr().cast_mut()),
            name_len: AtomicUsize::new(0),
        }
    }

    /// A poll of `task` named `name` starts at `now`. Only the worker writes.
    pub(crate) fn begin(&self, now: u64, task: u64, name: &'static str) {
        self.write(now, task, name);
    }

    /// The worker is between polls. Only the worker writes.
    pub(crate) fn idle(&self) {
        if self.start.load(Ordering::Relaxed) != IDLE {
            self.write(IDLE, 0, "");
        }
    }

    fn write(&self, start: u64, task: u64, name: &'static str) {
        let seq = self.seq.load(Ordering::Relaxed);
        self.seq.store(seq.wrapping_add(1), Ordering::Relaxed);
        fence(Ordering::Release);
        self.start.store(start, Ordering::Relaxed);
        self.task.store(task, Ordering::Relaxed);
        self.name_ptr
            .store(name.as_ptr().cast_mut(), Ordering::Relaxed);
        self.name_len.store(name.len(), Ordering::Relaxed);
        self.seq.store(seq.wrapping_add(2), Ordering::Release);
    }

    /// The poll running now, if the worker is in one and was not mid-write.
    pub(crate) fn read(&self) -> Option<Beat> {
        let before = self.seq.load(Ordering::Acquire);
        if before & 1 != 0 {
            return None;
        }
        let start = self.start.load(Ordering::Relaxed);
        let task = self.task.load(Ordering::Relaxed);
        let ptr = self.name_ptr.load(Ordering::Relaxed);
        let len = self.name_len.load(Ordering::Relaxed);
        fence(Ordering::Acquire);
        if self.seq.load(Ordering::Relaxed) != before || start == IDLE {
            return None;
        }
        // SAFETY: the sequence was even and unchanged across the reads, so
        // `ptr` and `len` were written together from one `&'static str` by
        // `write`, and name valid UTF-8 that lives for the whole program.
        let name = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(ptr, len)) };
        Some(Beat { start, task, name })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_poll_in_progress_and_nothing_when_idle() {
        let beat = Heartbeat::new();
        assert_eq!(beat.read(), None);
        beat.begin(7, 42 | REPORTED, "query");
        assert_eq!(
            beat.read(),
            Some(Beat {
                start: 7,
                task: 42 | REPORTED,
                name: "query"
            })
        );
        beat.idle();
        assert_eq!(beat.read(), None);
    }

    #[test]
    fn a_reader_racing_the_writer_never_sees_a_torn_name() {
        let beat = std::sync::Arc::new(Heartbeat::new());
        let writer = {
            let beat = beat.clone();
            std::thread::spawn(move || {
                for i in 0..20_000u64 {
                    if i % 2 == 0 {
                        beat.begin(i, i, "a-short-name");
                    } else {
                        beat.begin(i, i, "a-much-longer-task-name");
                    }
                }
            })
        };
        while !writer.is_finished() {
            if let Some(read) = beat.read() {
                assert_eq!(read.start, read.task);
                let expected = if read.start % 2 == 0 {
                    "a-short-name"
                } else {
                    "a-much-longer-task-name"
                };
                assert_eq!(read.name, expected);
            }
        }
        writer.join().unwrap();
    }
}
