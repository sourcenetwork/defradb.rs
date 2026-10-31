/-!
# TxnAdmission.Key: who may use an explicit transaction

The story, in plain words. A caller begins an explicit transaction and later
sends requests that name it. The registry that began it remembers an owner key,
and lets a request in only when the request's key is the same. What the key is
depends on the registry's ownership mode:
* identity mode, every registry's default: the key is the identity the request
  acts as, or nobody;
* embedder mode, which the embedded node behind the C API uses: every request
  has the same key, because the one process embedding the node is the only
  caller, whatever user it acts for in a request.

What must never happen:
* a registry in embedder mode turns a request away, so a transaction the
  embedding process began is unusable as any other identity, as in issue #2335
  (`embedder_admits_every`);
* a registry in identity mode lets a request in that acts as another identity
  than the one that began the transaction, or turns that identity away
  (`byIdentity_admits_iff`, `byIdentity_turns_away_stranger`);
* the caller that began a transaction is turned away from it, in either mode
  (`admits_beginner`);
* a registry that asks for no mode lets every request in: the default must be
  identity mode, never embedder mode (`default_is_byIdentity`).

Rust anchors: `TxnOwnership` and `DbTransactionRegistry::caller` in
`crates/db/src/txn/registry/mod.rs`, and `Table::admits` in
`crates/db/src/txn/owner/table.rs`, which compares the two keys for equality.
An identity is `Option<String>` in Rust and `Option String` here.
-/

namespace TxnAdmission

-- `TxnOwnership`: how a registry keys the transactions it begins.
inductive Ownership where
  -- Each request is keyed by the identity it acts as.
  | byIdentity
  -- Every request has the same key.
  | embedder
  deriving DecidableEq

-- `TxnOwnership::default()`: the mode of a registry that asks for none.
def Ownership.default : Ownership := .byIdentity

-- `DbTransactionRegistry::caller`: the owner key of a request acting as
-- `identity`, in a registry of mode `mode`.
def key : Ownership → Option String → Option String
  -- Identity mode: the identity itself is the key.
  | .byIdentity, identity => identity
  -- Embedder mode: one key for every request.
  | .embedder, _ => none

-- `Table::admits`: a request acting as `request` reaches a transaction begun
-- acting as `beganBy` when their two keys are the same.
def admits (mode : Ownership) (beganBy request : Option String) : Prop :=
  -- The same key, the same owner.
  key mode beganBy = key mode request

-- Promise: in embedder mode, every request reaches every transaction.
theorem embedder_admits_every (beganBy request : Option String) :
    admits .embedder beganBy request := by
  -- Both keys are the one embedder key, so they are equal.
  rfl

-- Promise: in identity mode, a request reaches a transaction exactly when it
-- acts as the identity that began it.
theorem byIdentity_admits_iff (beganBy request : Option String) :
    admits .byIdentity beganBy request ↔ beganBy = request := by
  -- In identity mode each key is its identity, so the keys are equal exactly
  -- when the identities are.
  exact Iff.rfl

-- Promise: in identity mode, a request acting as another identity than the
-- one that began a transaction is turned away.
theorem byIdentity_turns_away_stranger (beganBy request : Option String)
    -- Given the two identities differ,
    (h : beganBy ≠ request) :
    -- the request does not reach the transaction.
    ¬ admits .byIdentity beganBy request := by
  -- Admission would mean the identities are equal, which they are not.
  exact fun admitted => h ((byIdentity_admits_iff beganBy request).mp admitted)

-- Promise: in either mode, the caller that began a transaction reaches it.
theorem admits_beginner (mode : Ownership) (beganBy : Option String) :
    admits mode beganBy beganBy := by
  -- A key is always equal to itself.
  rfl

-- Promise: a registry that asks for no mode keys by identity, so only a
-- registry that asks for embedder mode lets every request in.
theorem default_is_byIdentity : Ownership.default = .byIdentity := by
  -- That is how the default is defined.
  rfl

end TxnAdmission
