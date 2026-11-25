//! A worker's heartbeat: which poll it is in, written by the worker and read
//! by the monitor on another thread.
//!
//! The poll is one word. A task poll's word holds when it started; a hook
//! poll's word holds its number, and the monitor times it from when it first
//! sees it, so the worker reads no clock for it. Moving from a task to the
//! hook, or to idle, is one store of that word.
//!
//! A task poll also publishes the task and its name. Those are written under
//! a sequence lock: the worker bumps the sequence to odd, writes the fields,
//! and bumps it to even; a reader keeps a read only if the sequence was even
//! and unchanged across it, so the name's pointer and length it keeps always
//! come from the same `&'static str`. A word stored outside the lock (the
//! hook's, or idle) never comes with a name the reader uses.

use std::sync::atomic::{fence, AtomicPtr, AtomicU64, AtomicUsize, Ordering};

/// The word while the worker is between polls.
const IDLE: u64 = u64::MAX;

/// Set in a hook poll's word.
const HOOK_BIT: u64 = 1;

/// The name a hook poll carries in reports.
pub(crate) const HOOK_NAME: &str = "io-hook";

/// The word of a task poll that started at `start`.
pub(crate) fn task_word(start: u64) -> u64 {
    start << 1
}

/// The word of the hook poll numbered `number`.
pub(crate) fn hook_word(number: u64) -> u64 {
    ((number & (u64::MAX >> 2)) << 1) | HOOK_BIT
}

/// One consistent reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Beat {
    /// The poll's word: the same for the whole poll, and no other poll on
    /// this worker has it while it runs.
    pub word: u64,
    /// The task, as its key's bits, with [`REPORTED`] set when it was
    /// reported before; 0 for a hook poll.
    pub task: u64,
    pub name: &'static str,
}

impl Beat {
    /// The poll is the worker's I/O hook, not a task.
    pub(crate) fn hook(&self) -> bool {
        self.word & HOOK_BIT != 0
    }

    /// When a task poll started, on the pool's clock.
    pub(crate) fn start(&self) -> u64 {
        self.word >> 1
    }

    /// A hook poll's number.
    pub(crate) fn number(&self) -> u64 {
        self.word >> 1
    }
}

/// Set in [`Beat::task`] for a task already reported as stuck.
pub(crate) const REPORTED: u64 = 1 << 63;

pub(crate) struct Heartbeat {
    seq: AtomicU64,
    word: AtomicU64,
    task: AtomicU64,
    name_ptr: AtomicPtr<u8>,
    name_len: AtomicUsize,
}

impl Heartbeat {
    pub(crate) fn new() -> Heartbeat {
        Heartbeat {
            seq: AtomicU64::new(0),
            word: AtomicU64::new(IDLE),
            task: AtomicU64::new(0),
            name_ptr: AtomicPtr::new("".as_ptr().cast_mut()),
            name_len: AtomicUsize::new(0),
        }
    }

    /// A poll of `task` named `name` starts at `now`. Only the worker writes.
    pub(crate) fn begin(&self, now: u64, task: u64, name: &'static str) {
        let seq = self.seq.load(Ordering::Relaxed);
        self.seq.store(seq.wrapping_add(1), Ordering::Relaxed);
        // Each field is a release store, not a release fence before plain
        // ones: a reader that sees any new field then sees the odd sequence
        // (it reads the sequence again after its acquire fence) and drops
        // the read, and a release store is cheaper than a full barrier.
        self.word.store(task_word(now), Ordering::Release);
        self.task.store(task, Ordering::Release);
        self.name_ptr
            .store(name.as_ptr().cast_mut(), Ordering::Release);
        self.name_len.store(name.len(), Ordering::Release);
        self.seq.store(seq.wrapping_add(2), Ordering::Release);
    }

    /// The hook poll whose word is `word` starts. Only the worker writes.
    pub(crate) fn begin_hook(&self, word: u64) {
        self.word.store(word, Ordering::Relaxed);
    }

    /// The worker is between polls. Only the worker writes.
    pub(crate) fn idle(&self) {
        if self.word.load(Ordering::Relaxed) != IDLE {
            self.word.store(IDLE, Ordering::Relaxed);
        }
    }

    /// The word shown now; `None` between polls.
    pub(crate) fn word(&self) -> Option<u64> {
        Some(self.word.load(Ordering::Relaxed)).filter(|word| *word != IDLE)
    }

    /// The poll running now, if the worker is in one and was not mid-write.
    pub(crate) fn read(&self) -> Option<Beat> {
        let before = self.seq.load(Ordering::Acquire);
        if before & 1 != 0 {
            return None;
        }
        let word = self.word.load(Ordering::Relaxed);
        let task = self.task.load(Ordering::Relaxed);
        let ptr = self.name_ptr.load(Ordering::Relaxed);
        let len = self.name_len.load(Ordering::Relaxed);
        fence(Ordering::Acquire);
        if self.seq.load(Ordering::Relaxed) != before || word == IDLE {
            return None;
        }
        if word & HOOK_BIT != 0 {
            return Some(Beat {
                word,
                task: 0,
                name: HOOK_NAME,
            });
        }
        // SAFETY: the sequence was even and unchanged across the reads, so
        // `ptr` and `len` were written together from one `&'static str` by
        // `begin`, and name valid UTF-8 that lives for the whole program.
        let name = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(ptr, len)) };
        Some(Beat { word, task, name })
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
        let read = beat.read().unwrap();
        assert_eq!(
            (read.start(), read.task, read.name),
            (7, 42 | REPORTED, "query")
        );
        assert!(!read.hook());
        beat.idle();
        assert_eq!(beat.read(), None);
        assert_eq!(beat.word(), None);
    }

    #[test]
    fn a_hook_poll_reads_as_the_hook_and_names_no_task() {
        let beat = Heartbeat::new();
        beat.begin(3, 42, "query");
        beat.begin_hook(hook_word(9));
        let read = beat.read().unwrap();
        assert!(read.hook());
        assert_eq!((read.number(), read.task, read.name), (9, 0, HOOK_NAME));
        assert_eq!(beat.word(), Some(hook_word(9)));
        beat.idle();
        assert_eq!(beat.read(), None);
    }

    #[test]
    fn task_and_hook_words_never_meet_idle_or_each_other() {
        for n in [0, 1, u64::MAX >> 2, u64::MAX] {
            assert_ne!(hook_word(n), IDLE);
            assert_ne!(hook_word(n) & HOOK_BIT, 0);
            assert_eq!(task_word(n) & HOOK_BIT, 0);
        }
    }

    #[test]
    fn a_reader_racing_the_writer_never_sees_a_torn_name() {
        let beat = std::sync::Arc::new(Heartbeat::new());
        let writer = {
            let beat = beat.clone();
            std::thread::spawn(move || {
                for i in 0..20_000u64 {
                    if i % 3 == 2 {
                        beat.begin_hook(hook_word(i));
                    } else if i % 2 == 0 {
                        beat.begin(i, i, "a-short-name");
                    } else {
                        beat.begin(i, i, "a-much-longer-task-name");
                    }
                }
            })
        };
        while !writer.is_finished() {
            if let Some(read) = beat.read() {
                if read.hook() {
                    assert_eq!(read.name, HOOK_NAME);
                    continue;
                }
                assert_eq!(read.start(), read.task);
                let expected = if read.start() % 2 == 0 {
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
