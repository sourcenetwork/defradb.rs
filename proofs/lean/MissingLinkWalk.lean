/-!
The story. A node walks the children of a root block to learn which blocks
of its DAG are still missing. The walk counts the children it looks at; at
every page boundary (each `page`-th child) it gives its core away, and a
fetch past its quantum may stop it there. What must never happen: a stopped
walk answers "nothing missing"; a walk with no deadline answers differently
from the plain walk; a walk stops anywhere but at a page boundary.

Rust: crates/database/p2p/src/sync/manager/links.rs `walk_pages` (the page
counter, the deadline checked only at `visits.is_multiple_of(MISSING_LINK_WALK_PAGE)`),
`walk_missing_links` (`WalkEnd::Preempted` or `WalkEnd::Frontier`) and
`find_all_missing_links` (no deadline). The dispatch-level ordering (when a
fetch passes a deadline) is modeled by `proofs/tla/MissingLinkWalk.tla`.
-/
namespace MissingLinkWalk

-- What one read of a child finds.
inductive Read where
  -- the child was already merged, so its subtree is done
  | merged
  -- the block is stored and the read returned it
  | present
  -- the block is not stored
  | absent
  -- the read failed
  | failed
  deriving DecidableEq

-- The plain walk answers with the missing children, or fails.
inductive Walk where
  -- every child was read and these are missing
  | ok (missing : List Nat)
  -- some read failed
  | err
  deriving DecidableEq

-- The paged walk can also be stopped.
inductive Outcome where
  -- every child was read and these are missing (`WalkEnd::Frontier`)
  | frontier (missing : List Nat)
  -- some read failed (`Err`)
  | err
  -- the deadline passed at a page boundary (`WalkEnd::Preempted`)
  | preempted
  deriving DecidableEq

-- The plain walk, as `find_all_missing_links` walked before #2599: a failure
-- anywhere fails it, an absent child joins the missing list.
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

-- The plain walk's answer, seen as a paged outcome.
def ofWalk : Walk → Outcome
  -- a successful walk is a frontier
  | .ok m => .frontier m
  -- a failed walk is a failure
  | .err => .err

-- The paged walk. `visits` children were looked at before these; `stop v`
-- says whether the deadline has passed when the `v`-th child comes up. The
-- deadline is asked only when `v` is a multiple of `page`.
def walkPaged (read : Nat → Read) (stop : Nat → Bool) (page : Nat) :
    Nat → List Nat → Outcome
  -- no children left: nothing more is missing
  | _, [] => .frontier []
  -- the next child is child number `v + 1`
  | v, c :: cs =>
    -- at a page boundary past the deadline the walk stops
    if (v + 1) % page = 0 ∧ stop (v + 1) = true then .preempted
    else
      match read c, walkPaged read stop page (v + 1) cs with
      -- a failed read fails the whole walk
      | .failed, _ => .err
      -- a failure or a stop further on ends the walk the same way
      | _, .err => .err
      | _, .preempted => .preempted
      -- an absent child is missing, in front of the rest's missing ones
      | .absent, .frontier m => .frontier (c :: m)
      -- a merged or present child adds nothing missing
      | _, .frontier m => .frontier m

-- A DAG is ready exactly when the outcome is an empty frontier.
def ready : Outcome → Bool
  -- an empty frontier: nothing missing
  | .frontier [] => true
  -- anything else: still missing something, or not known
  | _ => false

-- With no deadline the paged walk answers exactly what the plain walk does:
-- paging and yielding change nothing about the result.
theorem no_deadline_is_walk (read : Nat → Read) (page : Nat) (cs : List Nat) :
    ∀ v, walkPaged read (fun _ => false) page v cs = ofWalk (walk read cs) := by
  -- go through the children one by one
  induction cs with
  -- no children: both answer an empty frontier
  | nil => intro v; rfl
  | cons d ds ih =>
    -- the rest answers what the plain walk of the rest does
    intro v
    simp only [walkPaged, walk, ih (v + 1)]
    -- compare case by case on the first read and the rest's walk
    cases read d <;> cases walk read ds <;> simp [ofWalk]

-- A stopped walk never makes a DAG ready.
theorem preempted_never_ready : ready .preempted = false := by
  -- it is not an empty frontier
  rfl

-- A failed walk never makes a DAG ready.
theorem err_never_ready : ready .err = false := by
  -- it is not an empty frontier
  rfl

-- A walk stops only at a page boundary whose deadline has passed.
theorem stops_only_at_boundary (read : Nat → Read) (stop : Nat → Bool) (page : Nat)
    (cs : List Nat) :
    ∀ v, walkPaged read stop page v cs = .preempted →
      ∃ w, w % page = 0 ∧ stop w = true := by
  -- go through the children one by one
  induction cs with
  -- no children: the walk answers a frontier, never a stop
  | nil => intro v h; simp [walkPaged] at h
  | cons d ds ih =>
    intro v h
    simp only [walkPaged] at h
    -- either this child's boundary stopped it, or the rest did
    by_cases hb : (v + 1) % page = 0 ∧ stop (v + 1) = true
    · exact ⟨v + 1, hb.1, hb.2⟩
    · rw [if_neg hb] at h
      cases hd : read d <;> cases hw : walkPaged read stop page (v + 1) ds <;>
        rw [hd, hw] at h <;> simp at h
      -- the only way to a stop is the rest stopping
      all_goals exact ih (v + 1) hw

-- A ready paged walk is a walk the deadline never touched: it answers what
-- the plain walk does, so it read every child as merged or present
-- (DagReadiness.ready_implies_read covers the plain walk).
theorem ready_is_complete (read : Nat → Read) (stop : Nat → Bool) (page : Nat)
    (cs : List Nat) :
    ∀ v, ready (walkPaged read stop page v cs) = true →
      walk read cs = .ok [] := by
  -- go through the children one by one
  induction cs with
  -- no children: the plain walk is `ok []`
  | nil => intro v _; rfl
  | cons d ds ih =>
    intro v h
    simp only [walkPaged] at h
    -- a stop at this boundary is not ready
    by_cases hb : (v + 1) % page = 0 ∧ stop (v + 1) = true
    · rw [if_pos hb] at h; simp [ready] at h
    · rw [if_neg hb] at h
      -- otherwise the rest was ready too, and this child added nothing
      cases hd : read d <;> cases hw : walkPaged read stop page (v + 1) ds <;>
        rw [hd, hw] at h <;> simp [ready] at h
      -- the rest was ready (`h` says so), so its plain walk is `ok []`
      all_goals
        have hr := ih (v + 1) (by rw [hw]; exact h)
        simp [walk, hd, hr]

end MissingLinkWalk
