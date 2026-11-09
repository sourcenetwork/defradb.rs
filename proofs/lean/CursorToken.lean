/-!
# CursorToken — cursor tokens

The story. A client pages through a query with opaque cursor tokens: it gets a
token from one page and hands it back to ask for the next.

Who acts: the client, which holds the token; the node, which writes and reads
it.

What must never happen:
* a token carries a member beyond the DocID and the order values, or loses one;
* a token an older build wrote, with an empty `o`, stops decoding.

Rust anchors: `crates/cursor/src/lib.rs` `Cursor::encode`, `Cursor::decode`.
-/

namespace CursorToken

/-! ## 1. The token's members -/

/-- What a token says: a DocID and the order values (field name, value). -/
structure Payload where
  -- the row's DocID
  doc : String
  -- the row's order field values; empty when the token has none
  keys : List (String × Nat)
  deriving DecidableEq

/-- One JSON member of the token's object. -/
inductive Member where
  -- `"d": <docID>`
  | d (doc : String)
  -- `"k": {…}`
  | k (keys : List (String × Nat))
  -- `"o": <text>`, which older builds wrote
  | o (text : String)
  deriving DecidableEq

/-- The members a token holds: `d`, then `k` only when there are keys. -/
def encode (p : Payload) : List Member :=
  -- `k` is omitted when empty (`omitempty`)
  if p.keys = [] then [.d p.doc] else [.d p.doc, .k p.keys]

/-- Read a token's members: the last `d` and `k` win, anything else is ignored. -/
def decode : List Member → Payload
  -- no members: no DocID, no keys
  | [] => ⟨"", []⟩
  -- a `d` sets the DocID unless a later one does
  | .d doc :: rest => let p := decode rest; ⟨if p.doc = "" then doc else p.doc, p.keys⟩
  -- a `k` sets the keys unless a later one does
  | .k keys :: rest => let p := decode rest; ⟨p.doc, if p.keys = [] then keys else p.keys⟩
  -- an unknown member is skipped
  | .o _ :: rest => decode rest

/-- A token never carries `o`. -/
theorem encode_has_no_o (p : Payload) (t : String) : Member.o t ∉ encode p := by
  -- both shapes are made of `d` and `k` only
  unfold encode; split <;> simp

/-- Decoding what was encoded gives back the payload. -/
theorem decode_encode (p : Payload) : decode (encode p) = p := by
  -- one case per shape of `encode`
  obtain ⟨doc, keys⟩ := p
  unfold encode
  split <;> simp_all [decode]

/-- An older token with an empty `o` after its members decodes the same. -/
theorem decode_legacy_o (p : Payload) : decode (encode p ++ [.o ""]) = p := by
  -- the trailing `o` adds nothing, so this is `decode_encode`
  obtain ⟨doc, keys⟩ := p
  unfold encode
  split <;> simp_all [decode]

/-- RED: the old encoder, which always wrote an empty `o`. -/
def encodeWithO (p : Payload) : List Member := encode p ++ [.o ""]

/-- RED: the old encoder wrote a member a token does not hold. -/
theorem neg_old_encoder_differs : ∃ p, encodeWithO p ≠ encode p :=
  -- any payload shows it: the old one is one member longer
  ⟨⟨"doc-1", []⟩, by decide⟩

end CursorToken

#print axioms CursorToken.encode_has_no_o
#print axioms CursorToken.decode_encode
#print axioms CursorToken.decode_legacy_o
#print axioms CursorToken.neg_old_encoder_differs
