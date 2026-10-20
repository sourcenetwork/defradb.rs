/-!
# ServiceRecord.Validation: what a node accepts into its record book

The story, in plain words. Every node signs a small record listing the
services it offers and hands it to other nodes. A node receiving one checks
it before keeping it: the body must be small enough, the signature must be
the sender's, it must not have expired, it must not claim to stay valid for
too long, and it must not list too many services or overlong names. The
book keeps one record per node, and a record only replaces one with a lower
sequence number.

What must never happen:
* a record that breaks any limit is accepted (`validate_sound`), or a record
  that keeps every limit is refused (`validate_complete`);
* the book steps back to a lower sequence number (`offer_never_lowers`);
* the book takes a record that fails validation (`offer_keeps_only_valid`);
* an expired record is handed out (`live_not_expired`).

The signature check itself is cryptography and is not proved here: a record
carries whether its signature verified, and validation requires that it did.

Rust anchors: `verify` in `crates/p2p/src/iroh/records/verify.rs`;
`Book::offer`, `Book::live` in `crates/p2p/src/iroh/records/book.rs`.
-/

namespace ServiceRecord

-- The limits a record is checked against, as the Rust constants set them.
structure Limits where
  -- `MAX_RECORD_BYTES`: the longest encoded body accepted.
  maxBody : Nat
  -- `MAX_SERVICES`: the most services one record may list.
  maxServices : Nat
  -- `MAX_NAME_BYTES`: the longest name (custom service, relay URL, collection id).
  maxName : Nat
  -- `MAX_TTL`: the furthest into the future a record may expire.
  maxTtl : Nat

-- One service a record lists. Only the length of a name matters here.
inductive Service where
  -- A service with a name: a custom service, a relay URL, a plain collection id.
  | named (name : List Nat)
  -- A service with no name: replicator, SE query, KMS, a blinded collection.
  | unnamed

-- A received record, as validation sees it.
structure Record where
  -- The sender's sequence number; higher means newer.
  seq : Nat
  -- When it stops being valid, in seconds.
  expiresAt : Nat
  -- The services it lists.
  services : List Service
  -- How many bytes its encoded body took.
  bodyLen : Nat
  -- Whether its signature verified against the sender's key.
  sigOk : Bool

-- A service's name fits the name limit; a service without one always fits.
def nameFits (l : Limits) : Service → Bool
  -- A named service: its name must be within the limit;
  | .named name => decide (name.length ≤ l.maxName)
  -- an unnamed one has nothing to check.
  | .unnamed => true

-- The whole check, in the order the Rust runs it.
def validate (l : Limits) (now : Nat) (r : Record) : Bool :=
  -- The body is small enough to decode,
  decide (r.bodyLen ≤ l.maxBody) &&
  -- the signature is the sender's,
  r.sigOk &&
  -- it has not expired,
  decide (now < r.expiresAt) &&
  -- it does not claim to last longer than allowed,
  decide (r.expiresAt ≤ now + l.maxTtl) &&
  -- it does not list too many services,
  decide (r.services.length ≤ l.maxServices) &&
  -- and every name fits.
  r.services.all (nameFits l)

-- Promise: a record that passes keeps every limit.
theorem validate_sound (l : Limits) (now : Nat) (r : Record)
    -- Given the record passed validation,
    (h : validate l now r = true) :
    -- each limit holds.
    r.bodyLen ≤ l.maxBody ∧ r.sigOk = true ∧ now < r.expiresAt ∧
      r.expiresAt ≤ now + l.maxTtl ∧ r.services.length ≤ l.maxServices ∧
      ∀ s ∈ r.services, nameFits l s = true := by
  -- Split the chain of checks into its parts.
  simp only [validate, Bool.and_eq_true, decide_eq_true_eq, List.all_eq_true] at h
  obtain ⟨⟨⟨⟨⟨hb, hs⟩, he⟩, ht⟩, hn⟩, hall⟩ := h
  exact ⟨hb, hs, he, ht, hn, hall⟩

-- Promise: a record that keeps every limit passes.
theorem validate_complete (l : Limits) (now : Nat) (r : Record)
    -- Given each limit holds,
    (hb : r.bodyLen ≤ l.maxBody) (hs : r.sigOk = true) (he : now < r.expiresAt)
    (ht : r.expiresAt ≤ now + l.maxTtl) (hn : r.services.length ≤ l.maxServices)
    (hall : ∀ s ∈ r.services, nameFits l s = true) :
    -- the record passes.
    validate l now r = true := by
  -- Every check in the chain is one of the given facts.
  simp only [validate, Bool.and_eq_true, decide_eq_true_eq, List.all_eq_true]
  exact ⟨⟨⟨⟨⟨hb, hs⟩, he⟩, ht⟩, hn⟩, hall⟩

-- The book's slot for one node: take the new record only if it passes and
-- is newer than what the slot holds; otherwise keep what is there.
def offer (l : Limits) (now : Nat) (held : Option Record) (r : Record) : Option Record :=
  if validate l now r = true ∧ held.all (fun old => old.seq < r.seq) = true then some r
  else held

-- Promise: offering a record never lowers the slot's sequence number.
theorem offer_never_lowers (l : Limits) (now : Nat) (old r : Record) :
    -- Whatever is offered, the slot still holds a record at least as new.
    ∃ kept, offer l now (some old) r = some kept ∧ old.seq ≤ kept.seq := by
  unfold offer
  -- The record is taken only when it is newer;
  by_cases h : validate l now r = true ∧ (some old).all (fun o => o.seq < r.seq) = true
  · simp only [h, and_self, ↓reduceIte, Option.some.injEq, exists_eq_left']
    simp only [Option.all_some, decide_eq_true_eq] at h
    omega
  -- otherwise the old record stays.
  · simp only [h, ↓reduceIte, Option.some.injEq, exists_eq_left', Nat.le_refl]

-- Promise: whatever the slot holds after an offer is either the record it
-- held before or a record that passed validation.
theorem offer_keeps_only_valid (l : Limits) (now : Nat) (held : Option Record) (r kept : Record)
    -- Given the slot now holds `kept`,
    (h : offer l now held r = some kept) :
    -- it was there already, or it passed.
    held = some kept ∨ validate l now kept = true := by
  unfold offer at h
  split at h
  -- Taken: it is the offered record, which passed.
  · rename_i hok
    cases h
    exact Or.inr hok.1
  -- Not taken: the slot is unchanged.
  · exact Or.inl h

-- What the book hands out for a node: its record, only while unexpired.
def live (now : Nat) (held : Option Record) : Option Record :=
  held.filter (fun r => decide (now < r.expiresAt))

-- Promise: a record the book hands out has not expired.
theorem live_not_expired (now : Nat) (held : Option Record) (r : Record)
    -- Given the book handed out `r`,
    (h : live now held = some r) :
    -- it is still valid now.
    now < r.expiresAt := by
  -- Only an unexpired record passes the filter.
  unfold live at h
  cases held with
  | none => cases h
  | some x =>
    simp only [Option.filter_some, decide_eq_true_eq] at h
    split at h
    · rename_i hx
      cases h
      exact hx
    · cases h

end ServiceRecord
