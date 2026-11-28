import Lean

/-! The story. A created document's genesis may ask for a reader when the
creator's provider cannot join the create's transaction: everyone (`public`),
or the DID the document names in an immutable field (`addressedBy`). This file
decides who the creator grants, and proves that only the document's valid DID
address receives that grant.

The same file fixes who registers a document owner. An owned local create
registers in its transaction, a shared provider records its receiver's
registration with the genesis, and an incoming genesis under Local ACP never
registers an owner on the receiver.

The code: `creator_reader` in
`crates/database/db/src/merge/merge_handler/genesis_access.rs`;
`register_created_doc` in `crates/database/db/src/write/autocommit/helpers.rs`;
`persist_merged_document` and `record_replica` in
`crates/database/db/src/merge/merge_handler/composite_persist.rs`. -/
namespace EnrollmentGenesis
open Lean

/-- How this node obtained a document. -/
inductive Origin where
  /-- This node owns the create. -/
  | localCreate
  /-- A peer's signed genesis was merged under Local ACP. -/
  | localIncoming
  /-- A peer's signed genesis was merged under a shared provider. -/
  | sharedIncoming
  deriving DecidableEq, Repr

/-- Whether an acquisition path registers a document owner. -/
def registersOwner : Origin → Bool
  -- An owned create registers its creator in the create's transaction.
  | .localCreate => true
  -- Local ACP stays node-local: the receiver registers no incoming owner.
  | .localIncoming => false
  -- A shared provider records the receiver's registration with the genesis.
  | .sharedIncoming => true

/-- An owned create registers its creator. -/
theorem local_create_registers : registersOwner .localCreate = true := by
  -- This is the definition of the local-create branch.
  rfl

/-- An incoming Local ACP genesis does not register an owner on the receiver. -/
theorem incoming_local_does_not_register : registersOwner .localIncoming = false := by
  -- This is the definition of the incoming-local branch.
  rfl

/-- The shared-provider merge records the receiver's registration. -/
theorem shared_incoming_registers : registersOwner .sharedIncoming = true := by
  -- This is the definition of the shared-incoming branch.
  rfl

/-- Only an owned create and a shared-provider genesis register an owner. -/
theorem registered_iff_origin (origin : Origin) :
    registersOwner origin = true ↔ origin ≠ .localIncoming := by
  -- Check the three origins.
  cases origin <;> simp [registersOwner]

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

-- The access kinds as the conformance test names them.
def accessName : Access → String
  -- No access.
  | .off => "off"
  -- Everyone.
  | .public => "public"
  -- The addressed node.
  | .addressedBy => "addressed"

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

/-- `lean --run EnrollmentGenesis.lean creator-cases` prints the creator's cases
as JSON. -/
def main (args : List String) : IO Unit := do
  -- Pick the command.
  if args == ["creator-cases"] then
    IO.println EnrollmentGenesis.creatorCases.compress
  else
    throw <| IO.userError "usage: EnrollmentGenesis creator-cases"
