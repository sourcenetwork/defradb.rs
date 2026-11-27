import Std

/-! A prefetched header's proof read can fail before any header is authenticated.
`prepend_head` in `crates/engine/vera-embedded/src/acp.rs` leaves queued
notifications intact, so a later attempt can authenticate the same header.
This abstracts the light client's certificate checks and assumes the queued
copy's authentication succeeds. -/
namespace HeaderRetry

-- The transport presents a prefetched head before every queued notification.
def offered (head : Nat) (queued : List Nat) : List Nat := head :: queued

-- The client consumes the first failed authentication and sees the queue.
theorem failed_head_keeps_queue (head : Nat) (queued : List Nat) :
    (offered head queued).drop 1 = queued := by
  rfl

-- If the queue held a copy, a failed first attempt leaves that copy to retry.
theorem retry_remains (head : Nat) (queued : List Nat) (present : head ∈ queued) :
    head ∈ (offered head queued).drop 1 := by
  simpa [offered] using present

-- The former filter lost the queued copy at the same height.
theorem old_filter_loses_copy (head : Nat) :
    (List.filter (fun h => h > head) [head]) = [] := by
  simp

end HeaderRetry
