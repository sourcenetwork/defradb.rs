//! Opening and closing a store costs the same however many stores are open.
//!
//! The export of engine tickers once copied the list of every live engine on
//! each open and each close, so a process holding many stores paid for all of
//! them on every one. This counts the bytes one export and its drop allocate
//! on this thread, with no other store open and with thousands open.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::Arc;

struct Counting;

thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
    static BYTES: Cell<usize> = const { Cell::new(0) };
}

// SAFETY: forwards every call to the system allocator with the same layout;
// the thread-locals it touches are const-initialized and own no destructor,
// so counting never allocates.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNTING.with(Cell::get) {
            BYTES.with(|bytes| bytes.set(bytes.get() + layout.size()));
        }
        // SAFETY: the caller's contract, passed through.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller's contract, passed through.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// The fewest bytes one export and its drop allocated over a few tries, so a
/// reclamation pass that happens to land in one try does not count.
fn export_and_drop_bytes(statistics: &Arc<regolith::Statistics>) -> usize {
    (0..8)
        .map(|_| {
            BYTES.with(|bytes| bytes.set(0));
            COUNTING.with(|on| on.set(true));
            drop(telemetry::export_engine_tickers(statistics));
            COUNTING.with(|on| on.set(false));
            BYTES.with(Cell::get)
        })
        .min()
        .unwrap_or(usize::MAX)
}

#[test]
fn an_export_allocates_the_same_with_thousands_of_stores_open() {
    let statistics = Arc::new(regolith::Statistics::new());
    let alone = export_and_drop_bytes(&statistics);
    let open: Vec<_> = (0..4096)
        .map(|_| telemetry::export_engine_tickers(&statistics))
        .collect();
    let crowded = export_and_drop_bytes(&statistics);
    drop(open);
    assert!(
        crowded <= alone + 1024,
        "one export and its drop allocated {crowded} bytes with 4096 stores open, \
         {alone} with none"
    );
}
