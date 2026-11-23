/-!
The story. A node walks the children of a root block to learn which blocks of
its DAG are still missing. Each unmerged child is read once: the read finds
the block, finds nothing, or fails. What must never happen: the walk answers
"nothing missing" although some child was never read, and a failed walk is
taken by a caller as a complete DAG.

Rust: crates/database/p2p/src/sync/manager/links.rs `find_all_missing_links`
(one `get` per unmerged child) and `frontier_or_unknown` (a failed walk is the
frontier `[root]`). The orderings between callers are modeled by
`proofs/tla/DagReadiness.tla`.
-/
namespace DagReadiness

-- What one read of a child finds.
inductive Read where
  -- the child was already merged, so its subtree is done
  | merged
  -- the block is stored and the read returned it
  | present
  -- the block is not stored (never fetched, or deleted since it was linked)
  | absent
  -- the read failed (a storage fault, a conflict, corrupt bytes)
  | failed
  deriving DecidableEq

-- A walk answers with the missing children, or fails.
inductive Walk where
  -- the walk read every child and these are missing
  | ok (missing : List Nat)
  -- some read failed, so the frontier is unknown
  | err
  deriving DecidableEq

-- The walk over children named by numbers, with `read c` what reading `c` finds.
-- A failure stops the walk; an absent child joins the missing list.
def walk (read : Nat → Read) : List Nat → Walk
  -- no children left: nothing more is missing
  | [] => .ok []
  -- look at the first child, then at the rest
  | c :: cs =>
    match read c, walk read cs with
    -- a failed read fails the whole walk
    | .failed, _ => .err
    -- a failure further on also fails the walk
    | _, .err => .err
    -- an absent child is missing, in front of the rest's missing ones
    | .absent, .ok m => .ok (c :: m)
    -- a merged or present child adds nothing missing
    | _, .ok m => .ok m

-- The frontier a caller acts on: the missing list, or the root when the walk failed.
def frontierOrUnknown (root : Nat) : Walk → List Nat
  -- a successful walk gives its missing children
  | .ok m => m
  -- a failed walk gives the root itself
  | .err => [root]

-- A DAG is ready exactly when its frontier is empty.
def ready (root : Nat) (w : Walk) : Bool := (frontierOrUnknown root w).isEmpty

-- A walk that answers "nothing missing" read every child as merged or present.
theorem ok_nil_implies_all_read (read : Nat → Read) (cs : List Nat)
    (h : walk read cs = .ok []) : ∀ c ∈ cs, read c = .merged ∨ read c = .present := by
  -- go through the children one by one
  induction cs with
  -- no children: nothing to show
  | nil => intro c hc; cases hc
  | cons d ds ih =>
    -- look at how the first child read and how the rest walked
    intro c hc
    simp only [walk] at h
    cases hd : read d <;> cases hw : walk read ds <;> rw [hd, hw] at h <;> simp at h
    -- the remaining cases: d merged or present, and the rest walked to `ok []`
    all_goals
      subst h
      rcases List.mem_cons.mp hc with rfl | hc
      -- c is the first child, which read as merged or present
      · simp [hd]
      -- c is further on, and the rest's walk covers it
      · exact ih hw c hc

-- A child that reads as absent is always in the missing list of a successful walk.
theorem absent_is_missing (read : Nat → Read) (cs : List Nat) (m : List Nat)
    (h : walk read cs = .ok m) : ∀ c ∈ cs, read c = .absent → c ∈ m := by
  -- go through the children one by one
  induction cs generalizing m with
  -- no children: nothing to show
  | nil => intro c hc; cases hc
  | cons d ds ih =>
    -- look at how the first child read and how the rest walked
    intro c hc hread
    simp only [walk] at h
    cases hd : read d <;> cases hw : walk read ds <;> rw [hd, hw] at h <;> simp at h
    -- each case: either c is d, or c is further on and in the rest's list
    all_goals
      subst h
      rcases List.mem_cons.mp hc with rfl | hc
      -- c is the first child: it is missing, or it did not read as absent
      · first | simp | simp_all
      -- c is further on: the rest's missing list has it
      · first
        | exact List.mem_cons_of_mem _ (ih _ hw c hc hread)
        | exact ih _ hw c hc hread

-- A failed read anywhere makes the whole walk fail.
theorem failed_fails (read : Nat → Read) (cs : List Nat) (c : Nat)
    (hc : c ∈ cs) (hf : read c = .failed) : walk read cs = .err := by
  -- go through the children one by one
  induction cs with
  -- no children: c cannot be among them
  | nil => cases hc
  | cons d ds ih =>
    -- either the failing child is the first one, or it is further on
    simp only [walk]
    rcases List.mem_cons.mp hc with rfl | hc
    · simp [hf]
    · rw [ih hc]
      cases read d <;> rfl

-- The frontier of a failed walk is never empty.
theorem unknown_never_empty (root : Nat) : frontierOrUnknown root .err ≠ [] := by
  -- it is the one-element list [root]
  simp [frontierOrUnknown]

-- A failed walk never makes a DAG ready.
theorem error_never_ready (root : Nat) : ready root .err = false := by
  -- its frontier is [root], which is not empty
  simp [ready, frontierOrUnknown]

-- A DAG is ready only when the walk read every child as merged or present.
theorem ready_implies_read (read : Nat → Read) (cs : List Nat) (root : Nat)
    (h : ready root (walk read cs) = true) : ∀ c ∈ cs, read c = .merged ∨ read c = .present := by
  -- a ready walk did not fail, and its missing list is empty
  cases hw : walk read cs with
  | err => simp [hw, ready, frontierOrUnknown] at h
  | ok m =>
    simp [hw, ready, frontierOrUnknown] at h
    subst h
    exact ok_nil_implies_all_read read cs hw

end DagReadiness
