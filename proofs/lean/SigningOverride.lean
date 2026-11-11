/-!
# SigningOverride — a write's own signing choice

The story. A caller writes documents and may say, for that one write, whether
its blocks are signed: Go's `EnableSigning`, sent as `enableSigning`. The node
has its own setting (it signs, or it was started with `--no-signing`), and
there may be a key to sign with: the request identity's, the node's, or none.

Who acts: the caller, who makes the choice or leaves it out; the edge, which
picks the key the write signs with before the write runs. The key is picked
once and travels in the request's context, so a retried write signs the same
way; nothing here is temporal.

What must never happen:
* a write whose caller said `false` is signed;
* a write whose caller said `true` is left unsigned while there is a key;
* a write whose caller said nothing signs other than as the node's setting says.

Rust anchors: `crates/defra-core/src/request_context.rs`
`RequestContext::signing`; `crates/defra-core/src/signing.rs`
`resolve_signing_config_with_flag`; `crates/http/src/query_context.rs`
`request_context`; `crates/ffi/src/document/run.rs` `DocumentCall::begin`.
-/

namespace SigningOverride

/-! ## 1. The keys and the choice -/

/-- The keys there are to sign with. A key is a number standing for it. -/
structure Keys where
  -- the request identity's key, when that identity has one registered
  request : Option Nat
  -- the node identity's key, when the node has an identity
  node : Option Nat

/-- Whether the write signs: its caller's choice, else the node's setting. -/
def enabled (choice : Option Bool) (nodeSigns : Bool) : Bool :=
  -- an absent choice falls back to the node's setting
  choice.getD nodeSigns

/-- The key to use when signing is on: the request's, else the node's. -/
def pick (keys : Keys) : Option Nat :=
  -- look at the request identity's key first
  match keys.request with
  -- the request has a key: it signs
  | some key => some key
  -- it has none: the node's key signs, if the node has one
  | none => keys.node

/-- The key the edge resolves (`resolve_signing_config_with_flag`): none when
signing is off, else the picked key. -/
def resolve (on : Bool) (keys : Keys) : Option Nat :=
  -- off resolves no key at all; on picks one
  if on then pick keys else none

/-- The key the write signs with (`RequestContext::signing` over the context
the edge built): the choice decides the flag the key is resolved with, and
`false` also hides any key the context carries. -/
def signer (choice : Option Bool) (nodeSigns : Bool) (keys : Keys) : Option Nat :=
  -- look at the caller's choice
  match choice with
  -- `false`: no key, whatever the context carries
  | some false => none
  -- `true` or nothing: the key resolved with the chosen flag
  | _ => resolve (enabled choice nodeSigns) keys

/-! ## 2. What it promises -/

/-- A write whose caller said `false` is never signed. -/
theorem false_never_signs (nodeSigns : Bool) (keys : Keys) :
    signer (some false) nodeSigns keys = none := by
  -- the first branch of `signer` answers `none`
  rfl

/-- A write whose caller said `true` signs with the picked key, whatever the
node's setting: on a node that does not sign too. -/
theorem true_signs_with_the_picked_key (nodeSigns : Bool) (keys : Keys) :
    signer (some true) nodeSigns keys = pick keys := by
  -- the choice makes the flag `true`, so the key is the picked one
  rfl

/-- A write whose caller said `true` is signed whenever there is a key. -/
theorem true_signs_when_there_is_a_key (nodeSigns : Bool) (keys : Keys)
    (h : keys.request.isSome ∨ keys.node.isSome) :
    (signer (some true) nodeSigns keys).isSome := by
  -- reduce to the picked key
  rw [true_signs_with_the_picked_key]
  -- split on whether the request has a key
  cases hr : keys.request with
  -- it has one: the request's key is picked
  | some key => simp [pick, hr]
  -- it has none: the node's key is picked, and the hypothesis says it exists
  | none =>
    -- the request side of the hypothesis is false
    simp [hr] at h
    -- so the node's key is there and is the one picked
    simp [pick, hr, h]

/-- A write whose caller said nothing signs exactly as the node's setting says. -/
theorem absent_follows_the_node (nodeSigns : Bool) (keys : Keys) :
    signer none nodeSigns keys = resolve nodeSigns keys := by
  -- the second branch of `signer`, with the flag falling back to the node
  rfl

/-! ## 3. The bug this replaces -/

/-- The code before the fix: the key was resolved with the node's flag, and the
choice could only hide it. -/
def oldSigner (choice : Option Bool) (nodeSigns : Bool) (keys : Keys) : Option Nat :=
  -- look at the caller's choice
  match choice with
  -- `false` hid the key, as now
  | some false => none
  -- anything else ignored the choice and used the node's flag
  | _ => resolve nodeSigns keys

/-- RED witness: on a node that does not sign but holds a key, the old code
left a `true` write unsigned, and the new code signs it. -/
theorem old_left_true_unsigned :
    oldSigner (some true) false ⟨none, some 1⟩ = none ∧
      signer (some true) false ⟨none, some 1⟩ = some 1 := by
  -- both sides compute to concrete values
  exact ⟨rfl, rfl⟩

end SigningOverride
