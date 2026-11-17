/-!
The story: a caller asks to give someone a relation on the node (add) or to
take it away (delete). The target is a DID, `*` for everyone, something that
is no DID at all, or the empty string. The manager checks, in Go's order: NAC
set up, the caller's permission, NAC on, an identity acting, a target given,
the relation, then (for add) that the target is a DID or `*`. Then it changes
the relationship. A delete whose target is no DID finds nothing to take.

What must never happen: an empty target reaching the change, on add or on
delete, so it can never touch the relationship that `*` holds; an empty target
refused with another error than Go's "acp relationship target actor must not
be empty" once the caller, NAC and the identity have passed; a caller with no
identity told about the target instead of "requires identity".

Rust owners:
  crates/db/src/nac/write.rs        NacManager::decide_relationship (decide)
  crates/db/src/access/nac_ops/mod.rs DB::change_nac_relationship (parses the target)

Every route that changes one relationship (HTTP, the CLI, FFI, the embedded
node) reaches `decide`, so this order holds for all of them. The one exception
is the batch HTTP route, which Go does not have: crates/http/src/handlers/nac.rs
validate_add_relationship checks each entry's target and relation before any
of the batch is written, and so before the identity.
-/

namespace NacRelationship

-- Whether the relationship is given or taken.
inductive Change where
  -- Go's `AddNACActorRelationship`
  | add
  -- Go's `DeleteNACActorRelationship`
  | delete
  deriving DecidableEq, Repr

-- What the target string turned out to be.
inductive Target where
  -- the empty string: no target at all
  | empty
  -- a DID, or `*` for everyone
  | actor
  -- something given that is no DID and not `*`
  | notADid
  deriving DecidableEq, Repr

-- How a decision ends: one of Go's refusals, nothing to take, or the change.
inductive Outcome where
  -- "node acp is not configured"
  | notConfigured
  -- "not authorized to perform operation"
  | notAuthorized
  -- "operation requires ACP, but ACP not available"
  | notEnabled
  -- "node acp relationship operation requires identity"
  | requiresIdentity
  -- "acp relationship target actor must not be empty"
  | targetEmpty
  -- "relation not in resource"
  | relationNotInResource
  -- "actor must be a valid did"
  | invalidActor
  -- a delete for a target that is no DID: nothing to take, nothing refused
  | notFound
  -- the relationship is given or taken
  | change
  deriving DecidableEq, Repr

-- What the checks before the target found: each `true` means it passed.
structure Checks where
  -- NAC was set up
  configured : Bool
  -- the caller holds the permission for this change
  permitted : Bool
  -- NAC is on
  enabled : Bool
  -- someone acts in the request
  identity : Bool
  -- the relation is one the node's policy has
  relation : Bool
  deriving DecidableEq, Repr

-- `NacManager::decide_relationship`, in Go's order.
def decide (c : Change) (k : Checks) (t : Target) : Outcome :=
  -- not set up is refused first
  if !k.configured then .notConfigured
  -- then the caller's permission
  else if !k.permitted then .notAuthorized
  -- then NAC must be on
  else if !k.enabled then .notEnabled
  -- then someone must act
  else if !k.identity then .requiresIdentity
  -- then a target must be given, for add and delete alike
  else if t = .empty then .targetEmpty
  -- then the relation must be one the policy has
  else if !k.relation then .relationNotInResource
  else
    match c, t with
    -- add needs a DID or `*`
    | .add, .notADid => .invalidActor
    -- a delete for no DID finds nothing to take
    | .delete, .notADid => .notFound
    -- otherwise the relationship changes
    | _, _ => .change

-- An empty target never reaches the change, on add or on delete.
theorem empty_target_never_changes (c : Change) (k : Checks) :
    decide c k .empty ≠ .change := by
  -- every branch that can be reached with an empty target refuses
  rcases k with ⟨configured, permitted, enabled, identity, relation⟩
  cases c <;> cases configured <;> cases permitted <;> cases enabled <;>
    cases identity <;> cases relation <;> simp [decide]

-- Once NAC is set up and on, the caller permitted and an identity acting, an
-- empty target is refused with Go's empty-target error, whatever the relation.
theorem empty_target_is_refused_as_go (c : Change) (k : Checks)
    (hc : k.configured = true) (hp : k.permitted = true) (he : k.enabled = true)
    (hi : k.identity = true) :
    decide c k .empty = .targetEmpty := by
  -- the checks before the target all pass, so the target check answers
  simp [decide, hc, hp, he, hi]

-- A caller with no identity hears "requires identity", even when the target
-- is empty too: the identity is checked first (on every single-relationship
-- route; the batch route checks its entries before anything else).
theorem identity_is_checked_before_the_target (c : Change) (k : Checks) (t : Target)
    (hc : k.configured = true) (hp : k.permitted = true) (he : k.enabled = true)
    (hi : k.identity = false) :
    decide c k t = .requiresIdentity := by
  -- the identity check fails before the target is looked at
  simp [decide, hc, hp, he, hi]

-- A delete whose target is given but is no DID still finds nothing to take,
-- as before: only an empty target is refused.
theorem delete_of_a_non_did_finds_nothing (k : Checks)
    (hc : k.configured = true) (hp : k.permitted = true) (he : k.enabled = true)
    (hi : k.identity = true) (hr : k.relation = true) :
    decide .delete k .notADid = .notFound := by
  -- every check passes and the target is no DID
  simp [decide, hc, hp, he, hi, hr]

end NacRelationship
