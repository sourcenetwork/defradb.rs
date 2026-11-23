/-!
The ledger a DefraDB node rebuilds its Vera deployment from.

The node records every registration, grant and revocation the deployment
confirmed to it. Each lives in a slot: one object's owner, or one relationship.
A slot holds up to three records: what the deployment confirmed, and at most
two changes the node has asked for and not heard back about (a crash can leave
one behind): one that takes access away, one that gives it.

After a takeover the node's journal cannot cover, a recovery pass walks the
slots in pages, a fixed number of them at a time, and remembers how far it got.

What must never happen:
  * an unanswered revocation is ignored: once the node asked to take access
    away, the pass must read the slot as taken away, whatever else it holds;
  * an unanswered grant is taken as proof: the pass may owe again only what
    the deployment confirmed;
  * a change the deployment refused is left looking confirmed or pending;
  * a change a stop left unanswered overrides what the deployment answered
    later for the same slot;
  * the pass says it is over while slots are still unread, or reads a slot
    twice or not at all.

What this file proves:
  * a change that takes access away wins a slot (`deny_wins`), and a slot with
    nothing unanswered reads as what was confirmed (`settled_reads_confirmed`);
  * what a slot reads as counts as confirmed only when the deployment
    confirmed exactly that: always once nothing is unanswered
    (`settled_is_proven`), never for a grant asked over a different or missing
    confirmation (`unanswered_grant_is_unproven`);
  * a refused change leaves the slot exactly as it was (`refused_restores`),
    and an applied one is what the slot reads as (`applied_reads_fact`);
  * the deployment's newest answer replaces whatever a stop left unanswered
    (`confirmed_reads_fact`);
  * pages read one after the other, each starting where the one before it
    ended, hand over every slot exactly once and in order, and the pass ends
    exactly at the last one (`walk_covers`, `last_iff_end`).

Rust owner: `resolve`, `RecoveryLedger::confirm`, `RecoveryLedger::prepare`,
`RecoveryLedger::settle` and `RecoveryLedger::page` in
`crates/engine/acp/src/recovery.rs`, and the walk in `RecoverySource::next` in
`crates/engine/acp/src/external/recovery.rs`. Pages are counted in slots here
and in bytes in the code; either way a page ends between two slots. Stops part
way and a second takeover are `proofs/tla/FollowDeploymentRecovery.tla`.
-/
namespace VeraRecoveryLedger

/-- One slot's records: what the deployment confirmed, an unanswered change
that takes access away, and an unanswered change that gives it. -/
structure Slot (α : Type) where
  -- What the deployment confirmed, if anything.
  confirmed : Option α
  -- A delete or an unregistration asked for and not answered.
  denying : Option α
  -- A grant or a registration asked for and not answered.
  allowing : Option α

/-- What a slot reads as: an unanswered change stands for what it asked, and
one that takes access away comes first. -/
def resolve {α : Type} (slot : Slot α) : Option α :=
  -- Look at the two unanswered changes.
  match slot.denying, slot.allowing with
  -- A change that takes access away wins.
  | some denial, _ => some denial
  -- Otherwise a change that gives access stands.
  | none, some grant => some grant
  -- With nothing unanswered, what was confirmed.
  | none, none => slot.confirmed

/-- Whether what a slot reads as is what the deployment confirmed. A pass owes
a grant again only when this holds. -/
def proven {α : Type} [DecidableEq α] (slot : Slot α) : Bool :=
  -- Compare what the slot reads as with the confirmed record.
  decide (resolve slot = slot.confirmed)

/-- The node is about to ask for `fact`; `denies` says it takes access away. -/
def prepare {α : Type} (denies : Bool) (fact : α) (slot : Slot α) : Slot α :=
  -- The change is written in its own place, beside what was confirmed.
  if denies then { slot with denying := some fact } else { slot with allowing := some fact }

/-- The deployment confirmed `fact`: it is the slot's newest word, so it
replaces what was confirmed before and whatever was still unanswered. -/
def confirm {α : Type} (fact : α) (_slot : Slot α) : Slot α :=
  -- The confirmed record is written, both unanswered ones are dropped.
  { confirmed := some fact, denying := none, allowing := none }

/-- The deployment answered the change `fact`: it is confirmed when it was
applied, and no longer unanswered either way. -/
def settle {α : Type} (denies : Bool) (fact : α) (applied : Bool) (slot : Slot α) : Slot α :=
  -- An applied change becomes what the deployment confirmed.
  if applied then confirm fact slot
  -- A refused one only clears its own unanswered record.
  else if denies then { slot with denying := none } else { slot with allowing := none }

/-- A change that takes access away is what the slot reads as, whatever else
the slot holds. -/
theorem deny_wins {α : Type} (fact : α) (slot : Slot α) :
    resolve (prepare true fact slot) = some fact := by
  -- Preparing a denial fills the first place `resolve` looks at.
  simp [resolve, prepare]

/-- A slot with nothing unanswered reads as what the deployment confirmed. -/
theorem settled_reads_confirmed {α : Type} (slot : Slot α)
    (noDenial : slot.denying = none) (noGrant : slot.allowing = none) :
    resolve slot = slot.confirmed := by
  -- With both unanswered places empty, `resolve` takes its last case.
  simp [resolve, noDenial, noGrant]

/-- A slot with nothing unanswered is proven: it reads as what was confirmed. -/
theorem settled_is_proven {α : Type} [DecidableEq α] (slot : Slot α)
    (noDenial : slot.denying = none) (noGrant : slot.allowing = none) :
    proven slot = true := by
  -- It reads as the confirmed record, which is what `proven` compares.
  simp [proven, settled_reads_confirmed slot noDenial noGrant]

/-- A grant the deployment never answered is not proven, unless the deployment
had confirmed that very grant before. -/
theorem unanswered_grant_is_unproven {α : Type} [DecidableEq α] (grant : α) (slot : Slot α)
    (noDenial : slot.denying = none) (differs : slot.confirmed ≠ some grant) :
    proven (prepare false grant slot) = false := by
  -- Take the slot apart into its three records.
  cases slot with
  | mk confirmed denying allowing =>
    -- No denial is unanswered, so the slot reads as the grant.
    simp at noDenial differs
    -- The grant is not the confirmed record, so the two differ.
    simp [proven, prepare, resolve, noDenial]
    -- Turn the inequality around to match.
    exact fun same => differs same.symm

/-- A change the deployment refused leaves the slot exactly as it was before
the node asked. -/
theorem refused_restores {α : Type} (denies : Bool) (fact : α) (slot : Slot α)
    (free : if denies then slot.denying = none else slot.allowing = none) :
    settle denies fact false (prepare denies fact slot) = slot := by
  -- Take the slot apart into its three records.
  cases slot with
  | mk confirmed denying allowing =>
    -- Look at each kind of change in turn.
    cases denies with
    | true =>
      -- The denial was written and cleared; its place was empty before.
      simp at free
      simp [settle, prepare, free]
    | false =>
      -- The grant was written and cleared; its place was empty before.
      simp at free
      simp [settle, prepare, free]

/-- What the deployment confirmed is what the slot reads as, whatever was
unanswered before: a change a stop left behind cannot outlive a later answer. -/
theorem confirmed_reads_fact {α : Type} (fact : α) (slot : Slot α) :
    resolve (confirm fact slot) = some fact := by
  -- A confirmation leaves nothing unanswered, so `resolve` takes its last case.
  simp [resolve, confirm]

/-- A change the deployment applied is what the slot reads as. -/
theorem applied_reads_fact {α : Type} (denies : Bool) (fact : α) (slot : Slot α) :
    resolve (settle denies fact true (prepare denies fact slot)) = some fact := by
  -- An applied change is a confirmation.
  simp [settle, confirmed_reads_fact]

/-- The page that starts after `cursor` slots: the next `size` of them. -/
def page {α : Type} (size cursor : Nat) (slots : List α) : List α :=
  -- Skip what was handed over, take one page.
  (slots.drop cursor).take size

/-- Whether the page after `cursor` slots reaches the ledger's end. -/
def last {α : Type} (size cursor : Nat) (slots : List α) : Bool :=
  -- Nothing is left past this page.
  decide (slots.length ≤ cursor + size)

/-- Everything a pass hands over from `cursor` on, reading at most `pages`
pages: each page, then the next unless this one was the last. -/
def walk {α : Type} (size : Nat) (slots : List α) : Nat → Nat → List α
  -- No page left to read.
  | 0, _ => []
  -- One page, and the rest of the pass unless the ledger ended.
  | pages + 1, cursor =>
    page size cursor slots ++
      (if last size cursor slots then [] else walk size slots pages (cursor + size))

/-- The pass ends at a page exactly when no slot lies past it. -/
theorem last_iff_end {α : Type} (size cursor : Nat) (slots : List α) :
    last size cursor slots = true ↔ slots.drop (cursor + size) = [] := by
  -- Dropping leaves nothing exactly when the list is that short.
  simp [last, List.drop_eq_nil_iff]

/-- From any cursor, a pass with enough pages hands over every slot after the
cursor, once and in order. -/
theorem walk_from {α : Type} (size : Nat) (slots : List α) :
    ∀ pages cursor : Nat, slots.length ≤ cursor + pages * size →
      walk size slots pages cursor = slots.drop cursor := by
  intro pages
  -- By induction on how many pages may still be read.
  induction pages with
  | zero =>
    intro cursor enough
    -- No page may be read, so the cursor is already at the end.
    have atEnd : slots.length ≤ cursor := by simpa using enough
    -- Nothing is left to hand over.
    simp [walk, List.drop_eq_nil_iff.mpr atEnd]
  | succ pages ih =>
    intro cursor enough
    -- Read one page and ask whether it was the last.
    by_cases isLast : slots.length ≤ cursor + size
    · -- It was: the page is everything left.
      have short : (slots.drop cursor).length ≤ size := by
        -- What is left after the cursor fits in one page.
        simp [List.length_drop]
        omega
      -- So taking one page takes it all.
      simp [walk, page, last, isLast, List.take_of_length_le short]
    · -- It was not: the rest of the pass starts where this page ended.
      have rest : slots.length ≤ cursor + size + pages * size := by
        -- One page fewer is still enough from one page further.
        have spent : (pages + 1) * size = pages * size + size := Nat.succ_mul pages size
        omega
      -- The rest hands over everything after this page.
      have tail := ih (cursor + size) rest
      -- A page and what follows it are what was left.
      have glued : (slots.drop cursor).take size ++ slots.drop (cursor + size)
          = slots.drop cursor := by
        -- Dropping twice is dropping the sum.
        rw [← List.drop_drop]
        -- A list is its first part followed by the rest.
        exact List.take_append_drop size (slots.drop cursor)
      -- Put the page and the rest together.
      simp [walk, page, last, isLast, tail, glued]

/-- A pass that starts at the first slot hands over the whole ledger, once and
in order, whatever the page size, as long as a page holds at least one slot. -/
theorem walk_covers {α : Type} (size : Nat) (slots : List α) (positive : 0 < size) :
    walk size slots slots.length 0 = slots := by
  -- One slot per page is the slowest pass, and it still ends.
  have enough : slots.length ≤ 0 + slots.length * size := by
    -- A page holds at least one slot.
    have atLeast : slots.length * 1 ≤ slots.length * size := Nat.mul_le_mul_left _ positive
    omega
  -- The pass from the first slot hands over everything after no slot.
  simpa using walk_from size slots slots.length 0 enough

end VeraRecoveryLedger
