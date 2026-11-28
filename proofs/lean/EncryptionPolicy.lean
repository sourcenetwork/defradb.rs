/-!
The story: a write updates one field of a document and must decide how that
field's next block is encrypted. It sees the write's config (maybe none) and
the document's heads: for every field written so far, the kinds of its latest
blocks, encrypted or plaintext. One function decides
(`field_encryption_source`):
  (a) a config naming the field wins: a fresh key;
  (b) a field with heads follows them, as Go does: it reuses the link of an
      encrypted head, or stays plaintext when none is encrypted;
  (c) a field with no heads gets a fresh document key only when the document
      has a field and every field is encrypted; otherwise it is plaintext.
The composite block links no encryption, so the docID is Go's. When one
request carries several writes, each is prepared before any is written, so a
later preparation counts the fields the earlier ones prepared as written,
each with one head of the kind its write prepared (`written_fields`).

What must never happen: a field with heads decides differently from Go; a
plaintext field is encrypted without a config; an empty document counts as
encrypted; a composite carries a link.

Rust owner: crates/database/db/src/block/builder/write.rs
`field_encryption_source`, `every_field_encrypted`, `DocScope::preparing`,
`doc_scope_fields` (the `pending` overlay), `build_document_blocks`
(the composite), `write_delete_block_with_inputs` (the tombstone);
crates/database/db/src/block/builder/compute.rs `compute_document_blocks`.
-/

namespace EncryptionPolicy

-- The kind of a head block: encrypted, or plaintext.
inductive Kind where
  | enc
  | plain
  deriving DecidableEq, Repr

-- Where the field's next block takes its encryption from.
inductive Source where
  | explicit (individual : Bool)  -- the config names it: a fresh key
  | inherit                       -- an encrypted head's link, reused
  | docScopeNew                   -- a fresh document key
  | plaintext                     -- no encryption
  deriving DecidableEq, Repr

-- A write's config: the whole document, and the fields listed one by one.
structure Config where
  encDoc : Bool
  encFields : List Nat

-- Whether a config encrypts field `f`: the whole document, or `f` listed.
def Config.covers (c : Config) (f : Nat) : Bool :=
  c.encDoc || c.encFields.contains f

-- A document's heads: the fields written so far, and each field's head kinds.
structure Snap where
  fields : List Nat
  heads : Nat → List Kind

-- A field is encrypted when one of its heads is (Go skips plaintext heads).
def encrypted (h : List Kind) : Bool := h.contains Kind.enc

-- Rule (c)'s premise: some field exists and every field is encrypted.
def docAll (s : Snap) : Bool :=
  !s.fields.isEmpty && s.fields.all (fun g => encrypted (s.heads g))

-- Rules (b) and (c): the decision once no config names the field.
def fallback (s : Snap) (f : Nat) : Source :=
  -- No heads: rule (c).
  if (s.heads f).isEmpty then (if docAll s then Source.docScopeNew else Source.plaintext)
  -- Heads, one encrypted: reuse its link.
  else if encrypted (s.heads f) then Source.inherit
  -- Only plaintext heads: plaintext.
  else Source.plaintext

-- The whole decision: rule (a) first, then the fallback.
def source (cfg : Option Config) (s : Snap) (f : Nat) : Source :=
  match cfg with
  -- A config that covers the field: explicit, individual when listed.
  | some c => if c.covers f then Source.explicit (c.encFields.contains f) else fallback s f
  -- No config at all: the fallback.
  | none => fallback s f

-- Go's `determineBlockEncryption`: config first, then the field's own heads.
def goSource (cfg : Option Config) (s : Snap) (f : Nat) : Source :=
  match cfg with
  -- A config that covers the field: the same explicit answer.
  | some c =>
    if c.covers f then Source.explicit (c.encFields.contains f)
    else if encrypted (s.heads f) then Source.inherit else Source.plaintext
  -- No config: follow the heads, plaintext when none is encrypted.
  | none => if encrypted (s.heads f) then Source.inherit else Source.plaintext

-- Whether a config is absent or does not cover the field.
def uncovered (cfg : Option Config) (f : Nat) : Prop :=
  ∀ c, cfg = some c → c.covers f = false

-- With no covering config, the decision is the fallback.
theorem source_uncovered (cfg : Option Config) (s : Snap) (f : Nat)
    (h : uncovered cfg f) : source cfg s f = fallback s f := by
  -- Split on whether there is a config at all.
  cases cfg with
  | none => rfl  -- no config: the fallback by definition
  | some c =>
    -- A config that does not cover `f` falls through to the fallback.
    simp [source, h c rfl]

-- A field with heads gets exactly Go's answer, whatever the config.
theorem with_heads_matches_go (cfg : Option Config) (s : Snap) (f : Nat)
    (h : (s.heads f).isEmpty = false) : source cfg s f = goSource cfg s f := by
  -- Both sides agree on the config branch; the fallback skips rule (c).
  cases cfg with
  | none => simp [source, goSource, fallback, h]
  | some c => simp [source, goSource, fallback, h]

-- Rule (c)'s premise, spelled out: some field, and every field encrypted.
theorem docAll_iff (s : Snap) :
    docAll s = true ↔ s.fields ≠ [] ∧ ∀ g ∈ s.fields, encrypted (s.heads g) = true := by
  -- Unfold the Bool conjunction and `List.all`.
  simp [docAll, List.isEmpty_iff]

-- A field with no heads and no covering config gets the document key exactly
-- when the document is non-empty and every field is encrypted.
theorem headless_doc_scope_iff (cfg : Option Config) (s : Snap) (f : Nat)
    (hc : uncovered cfg f) (hh : (s.heads f).isEmpty = true) :
    source cfg s f = Source.docScopeNew ↔
      (s.fields ≠ [] ∧ ∀ g ∈ s.fields, encrypted (s.heads g) = true) := by
  -- Reduce to the fallback, then to `docAll`.
  rw [source_uncovered cfg s f hc, ← docAll_iff]
  -- With no heads, the fallback is rule (c) alone.
  cases hd : docAll s <;> simp [fallback, hh, hd]

-- An unlisted field with plaintext heads stays plaintext: nothing but a
-- config encrypts it, whatever the other fields are.
theorem unlisted_plaintext_heads_stay_plaintext (cfg : Option Config) (s : Snap) (f : Nat)
    (hc : uncovered cfg f) (hh : (s.heads f).isEmpty = false)
    (hp : encrypted (s.heads f) = false) : source cfg s f = Source.plaintext := by
  -- The fallback with heads, none encrypted, is plaintext.
  rw [source_uncovered cfg s f hc]
  simp [fallback, hh, hp]

-- A field with an encrypted head is never written in plaintext.
theorem encrypted_heads_never_downgrade (cfg : Option Config) (s : Snap) (f : Nat)
    (he : encrypted (s.heads f) = true) : source cfg s f ≠ Source.plaintext := by
  -- An encrypted head means the head list is not empty.
  have hne : (s.heads f).isEmpty = false := by
    cases hl : s.heads f with
    | nil => simp [encrypted, hl] at he
    | cons _ _ => rfl
  -- Then the answer is Go's, which reuses the link or is explicit.
  rw [with_heads_matches_go cfg s f hne]
  cases cfg with
  | none => simp [goSource, he]
  | some c =>
    -- Explicit or inherited: either way not plaintext.
    simp only [goSource, he]
    split <;> simp

-- The empty document: a field with no heads and no config is plaintext.
theorem empty_doc_is_plaintext (s : Snap) (f : Nat)
    (hf : s.fields = []) (hh : (s.heads f).isEmpty = true) :
    source none s f = Source.plaintext := by
  -- `docAll` needs a field, and there is none.
  simp [source, fallback, docAll, hf, hh]

-- Case (iii), the accepted cost: `age` was the only field, listed in
-- `encryptedFields`; an update without config writing `name` first gets the
-- document key. Fields: 0 = age, 1 = name.
theorem listed_every_field_then_new_field_gets_doc_key :
    source none ⟨[0], fun g => if g = 0 then [Kind.enc] else []⟩ 1 = Source.docScopeNew := by
  -- Computed directly.
  decide

-- The premise is not monotone: a remote plaintext block for a field with no
-- heads (field 1) turns a wholly encrypted document into one that is not.
theorem every_field_encrypted_is_not_monotone :
    docAll ⟨[0], fun g => if g = 0 then [Kind.enc] else []⟩ = true ∧
    docAll ⟨[0, 1], fun g => if g = 0 then [Kind.enc] else [Kind.plain]⟩ = false := by
  -- Both computed directly.
  decide

-- The request's earlier writes, prepared but not committed yet: the fields
-- they wrote, and how each one left its field (`written_fields`).
structure Pending where
  fields : List Nat
  kind : Nat → Kind

-- The heads a later preparation of the same request decides on: a pending
-- field has one head, of the kind its write prepared; every other field
-- keeps its committed heads (`DocScope::preparing` with `pending`).
def overlay (s : Snap) (p : Pending) : Snap :=
  { fields := p.fields ++ s.fields.filter (fun g => !p.fields.contains g),
    heads := fun g => if p.fields.contains g then [p.kind g] else s.heads g }

-- A field an earlier write of the request prepared counts as written: with
-- no config it follows the kind that write left it with, never rule (c).
theorem pending_field_follows_its_write (s : Snap) (p : Pending) (g : Nat)
    (hg : g ∈ p.fields) :
    source none (overlay s p) g =
      (if p.kind g = Kind.enc then Source.inherit else Source.plaintext) := by
  -- The overlay gives `g` exactly one head, so the fallback follows it.
  cases hk : p.kind g <;> simp [source, fallback, overlay, encrypted, hg, hk]

-- A pending plaintext field ends rule (c): the document is not wholly
-- encrypted, as `every_field_encrypted`'s early return on `pending` says.
theorem pending_plaintext_blocks_doc_scope (s : Snap) (p : Pending) (g : Nat)
    (hg : g ∈ p.fields) (hk : p.kind g = Kind.plain) :
    docAll (overlay s p) = false := by
  -- `g` is one of the overlay's fields, and its one head is plaintext.
  have hall : (overlay s p).fields.all (fun f => encrypted ((overlay s p).heads f)) = false := by
    rw [List.all_eq_false]
    exact ⟨g, by simp [overlay, hg], by simp [overlay, encrypted, hg, hk]⟩
  -- So the "every field" half of `docAll` fails.
  simp [docAll, hall]

-- Create then update in one request, on a new document: the create encrypts
-- field 0 by config, the update writes field 1 with none. Counting the
-- create's pending field, field 1 takes the document key.
theorem create_then_update_takes_doc_key :
    source none (overlay ⟨[], fun _ => []⟩ ⟨[0], fun _ => Kind.enc⟩) 1 =
      Source.docScopeNew := by
  -- Computed directly.
  decide

-- The same update decided on the committed heads alone sees no field at
-- all and writes plaintext: why the preparation carries `written_fields`.
theorem committed_heads_alone_miss_the_create :
    source none ⟨[], fun _ => []⟩ 1 = Source.plaintext := by
  -- Computed directly.
  decide

-- A composite block: its encryption link, if any.
structure Composite where
  link : Option Nat

-- The composite a write builds, create, update or delete alike.
def composite (_cfg : Option Config) (_s : Snap) : Composite := { link := none }

-- No composite links an encryption block, so the docID matches Go's.
theorem composite_unlinked (cfg : Option Config) (s : Snap) :
    (composite cfg s).link = none := by
  -- Built with no link.
  rfl

end EncryptionPolicy
