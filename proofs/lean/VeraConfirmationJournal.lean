/-!
The journal of submissions a DefraDB node saw its Vera deployment confirm.

The node sends registrations and relationship changes to Vera, and each is
confirmed in a finalized block at some height. The journal keeps the newest of
these confirmations, oldest first; every one is at the same height as the one
before it or higher, because a later request of the node's worker is executed
after an earlier one.

A deployment runs on several devices, one active at a time. When another device
takes over without a handover, every block above the takeover's cut is void.
The node then asks the deployment, newest first, whether it still holds each
journaled submission, and sends again the ones it does not.

What must never happen:
  * a submission the takeover voided is taken as kept because an older or a
    newer one was asked about instead: the ones the deployment no longer holds
    must be exactly the ones the node sends again;
  * a submission the journal pruned is taken as kept while the takeover voided
    it.

What this file proves:
  * the void submissions are exactly a suffix of the journal: everything after
    the first one the deployment no longer holds, and nothing before it
    (`missing_is_suffix`), so asking newest first and stopping at the first
    held one finds them all (`scan_finds_missing`);
  * the newest pruned submission vouches for every older pruned one: if the
    deployment holds it, it holds them all (`floor_covers_pruned`).

Rust owner: `reconcile` and `uncovered` in
`crates/engine/vera/src/vera_rs/follow/mod.rs`, and `Journal::record` and
`Journal::prune` in `crates/engine/vera/src/vera_rs/follow/journal.rs`. The
model is over heights: a submission is held exactly when it was confirmed at
or below the cut.

Scope: these theorems are about the scan of a journal no replay has touched
since the term it is checked against, where every entry still carries the
height it was first confirmed at. A replay records new heights entry by
entry, so a journal whose replay was cut short need not be ordered. The code
never scans such a journal for the same term: it writes down where its replay
starts before the first replay (`Journal::begin_replay`), and a check run
again sends everything from there on again without asking (`reconcile`);
under a later term it starts no later than that mark. That side, with stops
part way, is `proofs/tla/FollowDeployment.tla`.
-/
namespace VeraConfirmationJournal

/-- Whether a submission confirmed at `height` survives a takeover whose cut is
`cut`: blocks at or below the cut are kept. -/
def held (cut height : Nat) : Bool :=
  -- Above the cut the block is void.
  decide (height ≤ cut)

/-- The journal's heights, oldest first, never go down. -/
def Ordered (journal : List Nat) : Prop :=
  -- Every height is at most every later one.
  journal.Pairwise (· ≤ ·)

/-- The submissions the deployment no longer holds after the takeover. -/
def missing (cut : Nat) (journal : List Nat) : List Nat :=
  -- Keep the ones above the cut, in order.
  journal.filter (fun height => !held cut height)

/-- The submissions from the first one the deployment no longer holds on. -/
def suffix (cut : Nat) (journal : List Nat) : List Nat :=
  -- Skip the held ones at the front.
  journal.dropWhile (fun height => held cut height)

/-- In an ordered journal, the submissions a takeover voided are exactly its
suffix from the first void one: no held submission follows a void one. -/
theorem missing_is_suffix (cut : Nat) :
    ∀ journal : List Nat, Ordered journal → missing cut journal = suffix cut journal := by
  intro journal
  -- By induction on the journal, oldest entry first.
  induction journal with
  | nil =>
    -- An empty journal has nothing missing and no suffix.
    intro _
    rfl
  | cons first rest ih =>
    intro ordered
    -- The first entry is at most every later one, and the rest is ordered.
    have below : ∀ later ∈ rest, first ≤ later := (List.pairwise_cons.mp ordered).1
    have rest_ordered : Ordered rest := (List.pairwise_cons.mp ordered).2
    by_cases kept : first ≤ cut
    · -- The first entry is held: both sides skip it and go on with the rest.
      have h : held cut first = true := by simp [held, kept]
      simp [missing, suffix, List.filter_cons, List.dropWhile_cons, h]
      exact ih rest_ordered
    · -- The first entry is void, so every later one is void too: both sides
      -- keep the whole journal.
      have h : held cut first = false := by simp [held, kept]
      have all_void : ∀ later ∈ rest, (!held cut later) = true := by
        intro later mem
        have := below later mem
        simp [held]
        omega
      simp [missing, suffix, List.filter_cons, List.dropWhile_cons, h]
      intro later mem
      simpa using all_void later mem

/-- What asking newest first finds: the newest entries up to, and not
including, the first one the deployment holds. -/
def scan (cut : Nat) (journal : List Nat) : List Nat :=
  -- Walk the journal backwards while entries are void, then put them back in order.
  (journal.reverse.takeWhile (fun height => !held cut height)).reverse

/-- Read newest first, a journal's heights never go up. On such a list,
taking entries while they are void takes exactly the void ones. -/
theorem takeWhile_is_filter (cut : Nat) :
    ∀ newestFirst : List Nat, newestFirst.Pairwise (· ≥ ·) →
      newestFirst.takeWhile (fun height => !held cut height) =
        newestFirst.filter (fun height => !held cut height) := by
  intro newestFirst
  -- By induction from the newest entry.
  induction newestFirst with
  | nil => intro _; rfl
  | cons newest older ih =>
    intro ordered
    -- Every older entry is at most the newest, and the older ones are ordered too.
    have below : ∀ x ∈ older, newest ≥ x := (List.pairwise_cons.mp ordered).1
    have older_ordered := (List.pairwise_cons.mp ordered).2
    by_cases kept : newest ≤ cut
    · -- The newest is held: taking stops, and every older entry is held too,
      -- so none is void.
      have h : held cut newest = true := by simp [held, kept]
      simp [List.takeWhile_cons, List.filter_cons, h]
      intro x mem
      have := below x mem
      simp [held]
      omega
    · -- The newest is void: both take it and go on with the older ones.
      have h : held cut newest = false := by simp [held, kept]
      simp [List.takeWhile_cons, List.filter_cons, h]
      exact ih older_ordered

/-- Asking newest first and stopping at the first held submission finds every
void one, and only those, in their order. -/
theorem scan_finds_missing (cut : Nat) (journal : List Nat) (ordered : Ordered journal) :
    scan cut journal = missing cut journal := by
  -- Read backwards, the ordered journal never goes up.
  have reversed : journal.reverse.Pairwise (· ≥ ·) := by
    rw [List.pairwise_reverse]
    exact ordered
  unfold scan missing
  rw [takeWhile_is_filter cut journal.reverse reversed, List.filter_reverse, List.reverse_reverse]

/-- The newest entry the journal pruned vouches for every older pruned one: in
an ordered history of pruned entries followed by the journal, if the deployment
holds the newest pruned one it holds every pruned one. -/
theorem floor_covers_pruned (cut floor : Nat) (pruned journal : List Nat)
    (ordered : Ordered (pruned ++ [floor] ++ journal))
    (floor_held : held cut floor = true) :
    ∀ older ∈ pruned, held cut older = true := by
  intro older mem
  -- The pruned entries come before the floor, so none is higher than it.
  have parts := List.pairwise_append.mp ordered
  have front := List.pairwise_append.mp parts.1
  have below : older ≤ floor := front.2.2 older mem floor (List.mem_singleton_self floor)
  simp [held] at floor_held ⊢
  omega

end VeraConfirmationJournal
