/-!
The story: node access control (NAC) is on, temporarily off, or never set up.
The store does not keep that status as one value: it keeps the node policy,
the owner relationship and, while NAC is off, a `_disabled` relationship, and
the status is read back from those. A caller asks to turn NAC off or back on.
The manager reads the status, decides in Go's order (not set up, then for
disable "already off" before the caller's permission, and for re-enable the
caller's permission before "already on"), and writes the change, all in one
transaction. When two callers race, the loser's transaction fails and its
decision runs again on what the winner committed.

What must never happen: a change written for a caller without the
permission, or on a node where NAC was never set up; a change that leaves the
status where it was; a status change that touches any other relationship; a
loser of the race told anything but Go's "already".

Rust owners:
  crates/acp/src/nac/node_acp/mod.rs        NodeACP::load (decode)
  crates/acp/src/nac/node_acp/lifecycle.rs  NodeACP::disable, NodeACP::re_enable (the writes)
  crates/db/src/nac/toggle.rs               NacManager::decide_toggle (decide),
                                            NacManager::toggle_as (the re-run)
-/

namespace NacToggle

-- NAC's status, as `NacStatus` names it.
inductive Status where
  -- never set up, or purged
  | notConfigured
  -- on: every node operation is checked
  | enabled
  -- temporarily off: relationships kept for a re-enable
  | disabled
  deriving DecidableEq, Repr

-- What the store holds that decides the status, and everything else.
structure Stored where
  -- the node policy is stored
  policy : Bool
  -- the owner relationship's subject, if there is one (a DID, as a number)
  owner : Option Nat
  -- a `_disabled` relationship is stored
  disabledMark : Bool
  -- every other relationship on the node
  grants : List Nat
  deriving DecidableEq, Repr

-- `NodeACP::load`: no policy or no owner reads as never set up; otherwise
-- the `_disabled` mark decides between off and on.
def decode (s : Stored) : Status :=
  -- a policy must be stored for NAC to be set up
  if s.policy then
    match s.owner with
    -- with an owner, the mark says off, its absence on
    | some _ => if s.disabledMark then .disabled else .enabled
    -- without an owner nothing was set up
    | none => .notConfigured
  else .notConfigured

-- `NodeACP::disable`'s write: store the `_disabled` mark, with the owner as
-- its subject; with no owner there is nothing to write.
def writeDisable (s : Stored) : Stored :=
  match s.owner with
  -- set the mark, keep everything else
  | some _ => { s with disabledMark := true }
  -- no owner: unchanged
  | none => s

-- `NodeACP::re_enable`'s write: delete the `_disabled` mark.
def writeReEnable (s : Stored) : Stored :=
  match s.owner with
  -- clear the mark, keep everything else
  | some _ => { s with disabledMark := false }
  -- no owner: unchanged
  | none => s

-- Disabling NAC that reads as on leaves it reading as off.
theorem disable_turns_off (s : Stored) (on : decode s = .enabled) :
    decode (writeDisable s) = .disabled := by
  -- take the stored state apart and check every case
  rcases s with ⟨policy, owner, mark, grants⟩
  -- the cases that do not read as on contradict `on`
  cases policy <;> cases owner <;> cases mark <;> simp_all [decode, writeDisable]

-- Re-enabling NAC that reads as off leaves it reading as on.
theorem re_enable_turns_on (s : Stored) (off : decode s = .disabled) :
    decode (writeReEnable s) = .enabled := by
  -- take the stored state apart and check every case
  rcases s with ⟨policy, owner, mark, grants⟩
  -- the cases that do not read as off contradict `off`
  cases policy <;> cases owner <;> cases mark <;> simp_all [decode, writeReEnable]

-- Turning NAC off and back on restores exactly what was stored.
theorem re_enable_undoes_disable (s : Stored) (on : decode s = .enabled) :
    writeReEnable (writeDisable s) = s := by
  -- take the stored state apart and check every case
  rcases s with ⟨policy, owner, mark, grants⟩
  -- only "policy, owner, no mark" reads as on, and there the mark comes back off
  cases policy <;> cases owner <;> cases mark <;> simp_all [decode, writeDisable, writeReEnable]

-- A status change never touches any other relationship.
theorem toggles_keep_grants (s : Stored) :
    (writeDisable s).grants = s.grants ∧ (writeReEnable s).grants = s.grants := by
  -- take the stored state apart; the owner decides whether anything is written
  rcases s with ⟨policy, owner, mark, grants⟩
  -- either way the other relationships are the same list
  cases owner <;> simp [writeDisable, writeReEnable]

-- Which way a status change goes.
inductive Toggle where
  -- Go's `DisableNAC`
  | disable
  -- Go's `ReEnableNAC`
  | reEnable
  deriving DecidableEq, Repr

-- How a decision ends: one of Go's refusals, or the write.
inductive Outcome where
  -- "node acp is not configured"
  | notConfigured
  -- "node acp is already disabled"
  | alreadyDisabled
  -- "node acp is already enabled"
  | alreadyEnabled
  -- "not authorized to perform operation"
  | notAuthorized
  -- the status change is written
  | write
  deriving DecidableEq, Repr

-- `NacManager::decide_toggle`. `allowed` is what `NacManager::require`
-- answered: the node itself, or the caller's live permission for disable and
-- its stored permission for re-enable.
def decideToggle (t : Toggle) (st : Status) (allowed : Bool) : Outcome :=
  -- not set up is refused first, for both
  if st = .notConfigured then .notConfigured else
  match t with
  -- disable: already off, then the caller
  | .disable =>
    if st = .disabled then .alreadyDisabled
    else if allowed then .write else .notAuthorized
  -- re-enable: the caller, then already on
  | .reEnable =>
    if allowed then (if st = .enabled then .alreadyEnabled else .write)
    else .notAuthorized

-- The write a status change makes.
def apply : Toggle → Stored → Stored
  -- disable stores the mark
  | .disable, s => writeDisable s
  -- re-enable deletes it
  | .reEnable, s => writeReEnable s

-- Go's "already" refusal for each change.
def already : Toggle → Outcome
  -- disabling twice
  | .disable => .alreadyDisabled
  -- re-enabling twice
  | .reEnable => .alreadyEnabled

-- A change is written only for a caller that is allowed.
theorem writes_only_when_allowed (t : Toggle) (st : Status) (allowed : Bool)
    (w : decideToggle t st allowed = .write) : allowed = true := by
  -- every case either writes with `allowed` true or refuses
  cases t <;> cases st <;> cases allowed <;> simp_all [decideToggle]

-- A change is never written on a node where NAC was never set up.
theorem writes_only_when_configured (t : Toggle) (st : Status) (allowed : Bool)
    (w : decideToggle t st allowed = .write) : st ≠ .notConfigured := by
  -- the not-set-up case refuses before anything else
  cases t <;> cases st <;> cases allowed <;> simp_all [decideToggle]

-- A disable is written only from on, so it is never a silent no-op.
theorem disable_writes_only_from_on (st : Status) (allowed : Bool)
    (w : decideToggle .disable st allowed = .write) : st = .enabled := by
  -- every other starting status is refused
  cases st <;> cases allowed <;> simp_all [decideToggle]

-- A re-enable is written only from off, so it is never a silent no-op.
theorem re_enable_writes_only_from_off (st : Status) (allowed : Bool)
    (w : decideToggle .reEnable st allowed = .write) : st = .disabled := by
  -- every other starting status is refused
  cases st <;> cases allowed <;> simp_all [decideToggle]

-- Re-enable asks about the caller before "already on": a caller without the
-- permission on an enabled node hears not authorized, as in Go.
theorem re_enable_checks_the_caller_first :
    decideToggle .reEnable .enabled false = .notAuthorized := by
  -- the permission is decided before the status is looked at
  rfl

-- The race: if one caller's decision wrote its change, a second caller with
-- the permission that decides again on what the first committed hears Go's
-- "already". This is what the re-run after a lost commit gives the loser.
theorem rerun_after_a_win_hears_already (t : Toggle) (s : Stored) (allowed : Bool)
    (won : decideToggle t (decode s) allowed = .write) :
    decideToggle t (decode (apply t s)) true = already t := by
  -- take the stored state apart and check every case of the first decision
  rcases s with ⟨policy, owner, mark, grants⟩
  -- the winning cases leave the other status stored; the rest contradict `won`
  cases t <;> cases policy <;> cases owner <;> cases mark <;> cases allowed <;>
    simp_all [decideToggle, decode, apply, already, writeDisable, writeReEnable]

end NacToggle
