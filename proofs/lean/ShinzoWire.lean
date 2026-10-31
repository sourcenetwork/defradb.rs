/-!
# ShinzoWire: Go Shinzo's CAR messages and batch entries come back as sent

The story, in plain words. A Rust node and a Go `shinzo-develop` node send
each other three new shapes, each a CBOR map whose keys are the Go field
names, written in the order Go declares them:

* `CARRequest`: the usual signed envelope, then `CIDs`, the heads asked for;
* `CARReply`: the same envelope, then `CARs`, one entry per answered head;
* `DocumentInfo`: one document of a batched push: `DocID`, `CID`, `Block`,
  and `car` only when it is not empty.

The envelope is `Version`, `MessageID`, `SenderID`, `Pubkey`, then
`Signature` and `ErrMessage` only when they are set. A message is signed
over its encoding with no `Signature`, and the receiver checks the
signature by encoding what it decoded, again with no `Signature`.

What must never happen:
* a message reads back different from the one written
  (`decode_encode_request`, `decode_encode_reply`, `decode_encode_document`);
* the bytes a receiver checks the signature over differ from the bytes the
  sender signed (`verify_bytes_request`, `verify_bytes_reply`);
* an empty `car` is written, which Go never writes (`empty_car_not_written`).

Values are kept abstract: text, bytes, and lists of byte strings. How CBOR
lays each one out is the encoder's job and is pinned by byte fixtures from
Go's own encoder in the Rust tests, not here.

Rust anchors: `CarRequest`, `CarReply` in `crates/p2p/src/message/car_exchange.rs`;
`DocumentInfo` in `crates/p2p/src/message/pushlog.rs`.
-/

namespace ShinzoWire

-- A CBOR value as far as these messages need: text, a byte string, or a
-- list of byte strings. Bytes are natural numbers.
inductive Value where
  -- A text string.
  | text : String → Value
  -- A byte string.
  | bytes : List Nat → Value
  -- A list of byte strings.
  | list : List (List Nat) → Value
  deriving DecidableEq, Repr

-- A CBOR map, as its key/value pairs in written order.
abbrev Fields := List (String × Value)

-- The signed envelope every request and reply carries.
structure Envelope where
  -- The DefraDB message version.
  version : String
  -- The id a reply echoes back.
  messageId : String
  -- The sending peer.
  senderId : String
  -- The sender's public key.
  pubkey : List Nat
  -- The signature, absent while signing.
  signature : Option (List Nat)
  -- An error the replying node reports, absent when there is none.
  errMessage : Option String
  deriving DecidableEq, Repr

-- An optional field: written as one pair when present, not at all when not.
def opt (key : String) : Option Value → Fields
  -- Present: one pair.
  | some v => [(key, v)]
  -- Absent: nothing.
  | none => []

-- The envelope's fields, in Go's declaration order.
def encodeEnvelope (e : Envelope) : Fields :=
  [("Version", .text e.version), ("MessageID", .text e.messageId),
   ("SenderID", .text e.senderId), ("Pubkey", .bytes e.pubkey)]
  ++ opt "Signature" (e.signature.map .bytes)
  ++ opt "ErrMessage" (e.errMessage.map .text)

-- Read a text field; nothing if it is missing or not text.
def getText (fs : Fields) (k : String) : Option String :=
  match fs.lookup k with
  | some (.text t) => some t
  | _ => none

-- Read a byte-string field; nothing if it is missing or not bytes.
def getBytes (fs : Fields) (k : String) : Option (List Nat) :=
  match fs.lookup k with
  | some (.bytes b) => some b
  | _ => none

-- Read a list-of-byte-strings field; nothing if missing or not a list.
def getList (fs : Fields) (k : String) : Option (List (List Nat)) :=
  match fs.lookup k with
  | some (.list l) => some l
  | _ => none

-- Read the envelope back: the four fixed fields must be there; the two
-- optional ones are absent when missing.
def decodeEnvelope (fs : Fields) : Option Envelope := do
  let version ← getText fs "Version"
  let messageId ← getText fs "MessageID"
  let senderId ← getText fs "SenderID"
  let pubkey ← getBytes fs "Pubkey"
  pure { version, messageId, senderId, pubkey,
         signature := getBytes fs "Signature",
         errMessage := getText fs "ErrMessage" }

-- A CAR request: the envelope and the heads asked for.
structure CarRequest where
  -- The signed envelope.
  env : Envelope
  -- The heads, as CID bytes, in the order asked.
  cids : List (List Nat)
  deriving DecidableEq, Repr

-- A CAR reply: the envelope and one entry per answered head.
structure CarReply where
  -- The signed envelope.
  env : Envelope
  -- The CARs, parallel to a prefix of the asked heads; empty means "not served".
  cars : List (List Nat)
  deriving DecidableEq, Repr

-- Write a request: the envelope, then `CIDs`.
def encodeRequest (m : CarRequest) : Fields :=
  encodeEnvelope m.env ++ [("CIDs", .list m.cids)]

-- Write a reply: the envelope, then `CARs`.
def encodeReply (m : CarReply) : Fields :=
  encodeEnvelope m.env ++ [("CARs", .list m.cars)]

-- Read a request back.
def decodeRequest (fs : Fields) : Option CarRequest := do
  let env ← decodeEnvelope fs
  let cids ← getList fs "CIDs"
  pure { env, cids }

-- Read a reply back.
def decodeReply (fs : Fields) : Option CarReply := do
  let env ← decodeEnvelope fs
  let cars ← getList fs "CARs"
  pure { env, cars }

-- Promise: a request reads back exactly as written.
theorem decode_encode_request (m : CarRequest) :
    decodeRequest (encodeRequest m) = some m := by
  -- Every key is distinct, so each lookup finds the pair written for it;
  -- try each combination of the two optional fields.
  obtain ⟨⟨v, i, s, p, sig, err⟩, cids⟩ := m
  cases sig <;> cases err <;>
    simp [decodeRequest, encodeRequest, decodeEnvelope, encodeEnvelope, opt,
      getText, getBytes, getList, List.lookup]

-- Promise: a reply reads back exactly as written.
theorem decode_encode_reply (m : CarReply) :
    decodeReply (encodeReply m) = some m := by
  -- The same lookups as for a request, with `CARs` in place of `CIDs`.
  obtain ⟨⟨v, i, s, p, sig, err⟩, cars⟩ := m
  cases sig <;> cases err <;>
    simp [decodeReply, encodeReply, decodeEnvelope, encodeEnvelope, opt,
      getText, getBytes, getList, List.lookup]

-- The bytes a message is signed over: its encoding with no `Signature`.
def unsigned (e : Envelope) : Envelope := { e with signature := none }

-- Promise: the receiver, encoding what it decoded with the signature
-- removed, gets exactly the bytes the sender signed. So a valid signature
-- from either runtime checks out on the other.
theorem verify_bytes_request (m : CarRequest) :
    (decodeRequest (encodeRequest m)).map
        (fun r => encodeRequest { r with env := unsigned r.env })
      = some (encodeRequest { m with env := unsigned m.env }) := by
  -- It reads back as itself, so both sides encode the same message.
  rw [decode_encode_request]
  rfl

-- Promise: the same for a reply.
theorem verify_bytes_reply (m : CarReply) :
    (decodeReply (encodeReply m)).map
        (fun r => encodeReply { r with env := unsigned r.env })
      = some (encodeReply { m with env := unsigned m.env }) := by
  -- It reads back as itself, so both sides encode the same message.
  rw [decode_encode_reply]
  rfl

-- One document of a batched push.
structure DocumentInfo where
  -- The document id.
  docId : String
  -- The head CID.
  cid : List Nat
  -- The head block.
  block : List Nat
  -- The head's CAR, empty when not sent.
  car : List Nat
  deriving DecidableEq, Repr

-- Write a batch entry: `car` is left out when empty, as Go's `omitempty` does.
def encodeDocument (d : DocumentInfo) : Fields :=
  [("DocID", .text d.docId), ("CID", .bytes d.cid), ("Block", .bytes d.block)]
  ++ (if d.car = [] then [] else [("car", .bytes d.car)])

-- Read a batch entry back: a missing `car` is an empty one.
def decodeDocument (fs : Fields) : Option DocumentInfo := do
  let docId ← getText fs "DocID"
  let cid ← getBytes fs "CID"
  let block ← getBytes fs "Block"
  pure { docId, cid, block, car := (getBytes fs "car").getD [] }

-- Promise: a batch entry reads back exactly as written, empty `car` included.
theorem decode_encode_document (d : DocumentInfo) :
    decodeDocument (encodeDocument d) = some d := by
  -- Split on whether the CAR is empty; each lookup then finds its pair.
  obtain ⟨doc, cid, block, car⟩ := d
  by_cases h : car = []
  · subst h
    simp [decodeDocument, encodeDocument, getText, getBytes, List.lookup]
  · simp [decodeDocument, encodeDocument, getText, getBytes, List.lookup, h]

-- Promise: an empty `car` is never written, so the bytes match Go's.
theorem empty_car_not_written (d : DocumentInfo) (h : d.car = []) :
    (encodeDocument d).lookup "car" = none := by
  -- With the CAR empty only the three fixed pairs are written.
  simp [encodeDocument, h, List.lookup]

/-! ## The batch-signature Merkle tree

A batch signature signs the root of a Merkle tree over the batch's CIDs. Go
builds each layer by hashing neighbours in pairs, and pairs a last hash that
has no neighbour with itself. Rust must build the same layers, or a batch one
runtime signs would not verify on the other. What must never happen: a hash
reaches the next layer without being hashed again (`up_hashes_every_entry`),
or a layer fails to halve (`up_length`), which is what makes the climb end at
one root.

Rust anchor: `compute_merkle_root` in `crates/defra-core/src/batch_signing.rs`.
-/

section Merkle

-- `H a b` is SHA-256 of `a` followed by `b`; any two-argument function will do.
variable {α : Type} (H : α → α → α)

-- One layer up: hash neighbours in pairs; a last hash alone is paired with itself.
def up : List α → List α
  -- Two or more: hash the first pair, then do the rest.
  | a :: b :: rest => H a b :: up rest
  -- One left over: pair it with itself, as Go does.
  | [a] => [H a a]
  -- Nothing left: nothing more.
  | [] => []

-- Promise: a layer up is half as long, rounded up, so the climb ends at one root.
theorem up_length : ∀ l : List α, (up H l).length = (l.length + 1) / 2
  -- A pair: one hash for it, then the rest is half as long.
  | _ :: _ :: rest => by simp only [up, List.length_cons, up_length rest]; omega
  -- One left over: one hash.
  | [_] => by simp [up]
  -- Nothing: nothing.
  | [] => by simp [up]

-- Promise: every hash in the next layer is the hash of two hashes of this one;
-- none passes through unhashed.
theorem up_hashes_every_entry :
    ∀ (l : List α) (y : α), y ∈ up H l → ∃ a ∈ l, ∃ b ∈ l, y = H a b
  -- A pair: either the pair's own hash, or one from the rest.
  | a :: b :: rest, y, hy => by
    simp only [up, List.mem_cons] at hy
    rcases hy with rfl | hy
    · exact ⟨a, by simp, b, by simp, rfl⟩
    · obtain ⟨c, hc, d, hd, rfl⟩ := up_hashes_every_entry rest y hy
      exact ⟨c, by simp [hc], d, by simp [hd], rfl⟩
  -- One left over: its hash with itself.
  | [a], y, hy => by
    simp only [up, List.mem_singleton] at hy
    exact ⟨a, by simp, a, by simp, hy⟩
  -- Nothing: no hash to account for.
  | [], _, hy => by simp [up] at hy

-- Promise (a flaw, kept on purpose): in Go's tree, `[a, b, c]` and
-- `[a, b, c, c]` give the same layer above the leaves, so the same root.
-- This is why a verifier must check the signed CID count.
theorem go_odd_leaf_collides (a b c : α) : up H [a, b, c] = up H [a, b, c, c] := by
  -- Both layers are `[H a b, H c c]`.
  simp [up]

end Merkle

/-! ## The domain-separated Merkle tree

The fixed tree hashes a leaf as `SHA256(0x00 || cid)` and an inner node as
`SHA256(0x01 || left || right)`, and moves a layer's odd last node up
unchanged. Modelling the tagged hash as a tree that remembers its shape (a
collision-free hash), the root keeps every leaf, in order, and nothing else:
two different CID lists never share a root (`root_determines_cids`).

Rust anchor: `compute_merkle_root_with` with `MerkleScheme::DomainSeparated`
in `crates/defra-core/src/batch_signing.rs`.
-/

section DomainSeparated

variable {α : Type}

-- A node of the tree: a tagged leaf over one CID, or a tagged inner node.
inductive Tree (α : Type) where
  -- `SHA256(0x00 || cid)`.
  | leaf : α → Tree α
  -- `SHA256(0x01 || left || right)`.
  | node : Tree α → Tree α → Tree α

-- The CIDs under a node, left to right.
def Tree.leaves : Tree α → List α
  -- A leaf holds its CID.
  | .leaf a => [a]
  -- An inner node holds its left side's CIDs, then its right side's.
  | .node l r => l.leaves ++ r.leaves

-- One layer up: pair neighbours; a last node alone moves up unchanged.
def upDS : List (Tree α) → List (Tree α)
  -- Two or more: join the first pair, then do the rest.
  | a :: b :: rest => .node a b :: upDS rest
  -- One left over: it moves up as it is, with no copy of itself.
  | [a] => [a]
  -- Nothing left: nothing more.
  | [] => []

-- Promise: a layer up keeps every CID, in order, and adds none.
theorem upDS_leaves :
    ∀ l : List (Tree α), ((upDS l).map Tree.leaves).flatten = (l.map Tree.leaves).flatten
  -- A pair: the joined node holds both sides; the rest keeps its CIDs.
  | a :: b :: rest => by
    simp [upDS, Tree.leaves, upDS_leaves rest, List.append_assoc]
  -- One left over: unchanged.
  | [_] => by simp [upDS]
  -- Nothing: nothing.
  | [] => by simp [upDS]

-- Climb `n` layers.
def climb : List (Tree α) → Nat → List (Tree α)
  -- No more layers: this one.
  | l, 0 => l
  -- One more: go up, then climb the rest.
  | l, n + 1 => climb (upDS l) n

-- Promise: however many layers are climbed, every CID stays, in order.
theorem climb_leaves (n : Nat) :
    ∀ l : List (Tree α), ((climb l n).map Tree.leaves).flatten = (l.map Tree.leaves).flatten := by
  -- One layer at a time.
  induction n with
  -- No layers: nothing changes.
  | zero => intro l; rfl
  -- One more layer keeps the CIDs, then the rest does too.
  | succ n ih => intro l; simp only [climb]; rw [ih, upDS_leaves]

-- Promise: the leaf layer over a CID list holds exactly that list.
theorem leaves_of_leaf_layer (xs : List α) :
    ((xs.map Tree.leaf).map Tree.leaves).flatten = xs := by
  -- One CID at a time.
  induction xs with
  -- No CIDs: nothing.
  | nil => rfl
  -- A CID's leaf holds just it, then the rest holds the rest.
  | cons x xs ih =>
    simp only [List.map_cons, List.flatten_cons, Tree.leaves, ih, List.singleton_append]

-- Promise: if two CID lists climb to the same single root, they are the
-- same list. So no CID can be added, dropped, repeated or reordered without
-- changing the root.
theorem root_determines_cids (xs ys : List α) (n m : Nat) (t : Tree α)
    -- Given `xs` climbs to the single root `t`,
    (hx : climb (xs.map Tree.leaf) n = [t])
    -- and `ys` climbs to the same root,
    (hy : climb (ys.map Tree.leaf) m = [t]) :
    -- the two lists are equal.
    xs = ys := by
  -- Both lists are the CIDs under `t`.
  have ex := climb_leaves n (xs.map Tree.leaf)
  have ey := climb_leaves m (ys.map Tree.leaf)
  rw [hx, leaves_of_leaf_layer] at ex
  rw [hy, leaves_of_leaf_layer] at ey
  exact ex.symm.trans ey

end DomainSeparated

end ShinzoWire
