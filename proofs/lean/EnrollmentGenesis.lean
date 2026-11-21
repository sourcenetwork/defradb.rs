import Lean

/-! The story. A remote author signs a document's genesis and replicates it to
this node. The host may have asked that, for this collection, the genesis also
grants read access: to everyone (`public`), or to this node when the genesis
names it in an immutable field (`addressedBy`). This file decides who that
reader is, and proves that nobody else becomes one: not a node the genesis did
not name, not anyone when the host did not opt in, and nobody when the genesis
arrives again after the document was registered.

The code: `genesis_reader` and `register_signed_genesis` in
`crates/database/db/src/merge/merge_handler/genesis_access.rs`. -/
namespace EnrollmentGenesis
open Lean

-- What the host asked for this collection.
inductive Access where
  -- No read access comes with the genesis.
  | off
  -- Everyone may read: the reader is the wildcard.
  | public
  -- Only the node the genesis names in the host's field may read.
  | addressedBy
  deriving DecidableEq, Repr

-- The wildcard reader, the one spelling that means everyone.
def wildcard : String := "*"

/-- The reader a genesis asks for, before the first-registration guard.
`address` is the decoded addressed field (none when it is absent, mutable or
not a string), `receiver` this node's DID (none when the node has no identity). -/
def genesisReader (access : Access) (address receiver : Option String) : Option String :=
  -- Look at what the host asked for.
  match access with
  -- Nothing was asked: nobody.
  | .off => none
  -- A public collection: everyone.
  | .public => some wildcard
  -- An addressed collection: this node, and only when the genesis names it.
  | .addressedBy =>
    match address, receiver with
    -- Both are known: the node reads exactly when the names are equal.
    | some a, some r => if a = r then some r else none
    -- A missing address or a node without identity: nobody.
    | _, _ => none

/-- A collection the host did not opt in grants nothing. -/
theorem off_grants_nothing (address receiver : Option String) :
    genesisReader .off address receiver = none := by
  -- Unfold the definition; the `off` branch is `none`.
  rfl

/-- An addressed grant only ever names this node. -/
theorem addressed_names_only_receiver (address receiver : Option String) (r : String)
    (h : genesisReader .addressedBy address receiver = some r) : receiver = some r := by
  -- Split on both options and on the equality test; every branch that returns
  -- `some` returns the receiver itself.
  unfold genesisReader at h
  split at h <;> (try split at h) <;> simp_all

/-- A genesis that names some other node grants this node nothing. -/
theorem other_address_grants_nothing (a r : String) (h : a ≠ r) :
    genesisReader .addressedBy (some a) (some r) = none := by
  -- The names differ, so the equality test fails.
  simp [genesisReader, h]

/-- Without an address, an addressed collection stays private. -/
theorem ordinary_delivery_keeps_private (receiver : Option String) :
    genesisReader .addressedBy none receiver = none := by
  -- No address means the catch-all branch.
  cases receiver <;> rfl

/-- Local initial read access is registered only at the first registration of
a verified genesis that asks for a reader. -/
def initialReader (first verified : Bool) (reader : Option String) : Option String :=
  -- The guard holds only for a first registration of a signed genesis.
  if first && verified then reader else none

/-- A genesis that arrives again, after the document is registered, grants
nothing: an owner's revocation stands. -/
theorem replay_keeps_revocation (verified : Bool) (reader : Option String) :
    initialReader false verified reader = none := by
  -- `false && _` is `false`, so the guard fails.
  simp [initialReader]

/-- An unsigned genesis grants nothing. -/
theorem unsigned_grants_nothing (first : Bool) (reader : Option String) :
    initialReader first false reader = none := by
  -- `_ && false` is `false`, so the guard fails.
  simp [initialReader]

-- A lowercase hexadecimal digit, as a provider actor's digest is written.
def isLowerHex (c : Char) : Bool := c.isDigit || ('a' ≤ c && c ≤ 'f')

/-- Whether `s` is a DID a node can be named by: a `did:key` identity, or a
`did:opk` provider actor whose digest is 64 lowercase hexadecimal digits
(`Did::new` in `crates/building-blocks/identity/src/did.rs`). -/
def isDid (s : String) : Bool :=
  -- A key identity, or a provider actor with a well-formed digest.
  s.startsWith "did:key:" ||
    (s.startsWith "did:opk:" && ((s.drop 8).length == 64 && (s.drop 8).all isLowerHex))

/-- The wildcard is not a DID. -/
theorem wildcard_is_not_a_did : isDid wildcard = false := by
  -- Evaluate both prefix tests on "*".
  decide

/-- The reader a creator grants for its own genesis when its node's provider
cannot join the create's transaction (a remote provider, such as Vera): the
receivers share that provider and cannot grant themselves anything, so the
creator stages the grant in the create's transaction, with its registration.
`address` is the created document's addressed field (none when it is absent,
mutable or not a string). -/
def creatorReader (access : Access) (address : Option String) : Option String :=
  -- Look at what the host asked for.
  match access with
  -- Nothing was asked: nobody.
  | .off => none
  -- A public collection: everyone.
  | .public => some wildcard
  -- An addressed collection: exactly the node the document names, when the
  -- address is a DID; anything else, the wildcard included, names no node.
  | .addressedBy =>
    match address with
    -- An address: that node, if it is spelled as a DID.
    | some a => if isDid a then some a else none
    -- No address: nobody.
    | none => none

/-- A collection the host did not opt in makes the creator grant nothing. -/
theorem creator_off_grants_nothing (address : Option String) :
    creatorReader .off address = none := by
  -- The `off` branch is `none`.
  rfl

/-- An addressed grant by the creator names exactly the address, and so no
other node. -/
theorem creator_addressed_names_only_address (address : Option String) (r : String)
    (h : creatorReader .addressedBy address = some r) : address = some r := by
  -- Split on the address and on the DID test; the only `some` returned is
  -- the address itself.
  unfold creatorReader at h
  split at h <;> (try split at h) <;> simp_all

/-- An addressed collection never makes the creator grant everyone. -/
theorem creator_addressed_never_grants_everyone (address : Option String) :
    creatorReader .addressedBy address ≠ some wildcard := by
  -- The only address returned is a DID, and the wildcard is not one.
  intro h
  have hd := creator_addressed_names_only_address address wildcard h
  subst hd
  simp [creatorReader, wildcard_is_not_a_did] at h

/-- An address that is not a DID names nobody. -/
theorem creator_non_did_grants_nothing (a : String) (h : isDid a = false) :
    creatorReader .addressedBy (some a) = none := by
  -- The DID test fails.
  simp [creatorReader, h]

/-- Without an address, an addressed collection stays private on the provider. -/
theorem creator_without_address_grants_nothing :
    creatorReader .addressedBy none = none := by
  -- The address is the answer, and it is absent.
  rfl

/-- The creator grants exactly what the addressed node would grant itself
when it registers the genesis on a provider of its own: on a shared provider
the node the document names reads it as it would have locally. -/
theorem creator_matches_addressed_receiver (access : Access) (a : String) (h : isDid a = true) :
    creatorReader access (some a) = genesisReader access (some a) (some a) := by
  -- Every access kind gives the same answer on both sides, the address being
  -- a DID as the receiving node's own name is.
  cases access <;> simp [creatorReader, genesisReader, h]

-- The access kinds as the conformance test names them.
def accessName : Access → String
  -- No access.
  | .off => "off"
  -- Everyone.
  | .public => "public"
  -- The addressed node.
  | .addressedBy => "addressed"

/-- The executable cases the Rust conformance test compares against: every
access kind against a matching, a different and a missing address, with and
without a node identity. -/
def cases : Json :=
  -- The three inputs, each over its interesting values.
  let accesses := [Access.off, .public, .addressedBy]
  let addresses : List (Option String) := [some "did:key:receiver", some "did:key:other", none]
  let receivers : List (Option String) := [some "did:key:receiver", none]
  -- Every combination, with the reader the model decides.
  toJson <| accesses.flatMap fun access => addresses.flatMap fun address =>
    receivers.map fun receiver => Json.mkObj
      [("access", toJson (accessName access)), ("address", toJson address),
       ("receiver", toJson receiver), ("reader", toJson (genesisReader access address receiver))]

/-- The executable cases for the creator's grant: every access kind against
an address and no address. -/
def creatorCases : Json :=
  -- The two inputs, each over its interesting values.
  let accesses := [Access.off, .public, .addressedBy]
  let addresses : List (Option String) :=
    [some "did:key:receiver", some wildcard, some "not a did",
     some ("did:opk:" ++ String.mk (List.replicate 64 'a')),
     some ("did:opk:" ++ String.mk (List.replicate 63 'a')),
     some ("did:opk:" ++ String.mk (List.replicate 64 'A')), none]
  -- Every combination, with the reader the model decides.
  toJson <| accesses.flatMap fun access => addresses.map fun address =>
    Json.mkObj
      [("access", toJson (accessName access)), ("address", toJson address),
       ("reader", toJson (creatorReader access address))]

end EnrollmentGenesis

/-- `lean --run EnrollmentGenesis.lean cases` prints the receiver's cases as
JSON, `creator-cases` the creator's. -/
def main (args : List String) : IO Unit := do
  -- Pick the command.
  if args == ["cases"] then
    IO.println EnrollmentGenesis.cases.compress
  else if args == ["creator-cases"] then
    IO.println EnrollmentGenesis.creatorCases.compress
  else
    throw <| IO.userError "usage: EnrollmentGenesis cases | creator-cases"
