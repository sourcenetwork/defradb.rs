/-!
# Node access control checks a document write in Go's order

The story, in plain words. With node access control (NAC) on, a caller asks
the node to add, update or delete a document, or to truncate a collection.
Go first reads the collection, a read that is checked as `get-collection`,
and only then checks the write's own permission. So the node keeps, for each
permission, the list of permissions checking it takes, in Go's order, and
checks them one by one: the first one the caller lacks is the one the
refusal names.

What this file proves: a collection write's list is `get-collection` and then
the write's own permission; every other permission's list is itself alone;
the check lets a caller through exactly when it holds every permission on
the list; and a caller holding neither is refused `get-collection`, as Go
refuses it.

Rust owner: crates/acp/src/nac/permission.rs `NodePermission::go_order`
(the list) and crates/db/src/access/nac.rs `DB::check_node_access_as`,
`DB::check_node_access_in_txn` (the check, first missing first), and, for a
write in an explicit transaction, crates/query/src/runner/turn.rs
`QueryRunner::lent` with `lent_node_permission`. A permission checked alone
includes the P2P adapters' gates, crates/p2p-adapter/src/libp2p.rs
`P2PAdapter::check_nac` and crates/p2p-adapter/src/iroh.rs
`IrohP2PAdapter::check_nac`, which run that check for every peer operation,
the `get-p2p-peer-info` reads among them. The Rust checks are the testo
scenarios `acp::nac::{add,update,delete}_document`,
`update_document_with_filter`, `truncate_collection` and `get_p2p_peer_info`.
-/

namespace NacOrder

-- The permissions that matter here: the collection read, the three writes
-- that read their collection first, and every other permission as one.
inductive Perm where
  -- `get-collection`
  | collectionGet
  -- `update-document`, also asked of an add
  | documentUpdate
  -- `delete-document`
  | documentDelete
  -- `truncate-collection`
  | collectionTruncate
  -- any other node permission
  | other
  deriving DecidableEq, Repr

-- Whether checking `p` reads its collection first, as Go's
-- `GetCollectionByName` does before these writes.
def readsCollection : Perm → Bool
  -- the three collection writes do
  | .documentUpdate | .documentDelete | .collectionTruncate => true
  -- nothing else does
  | _ => false

-- `NodePermission::go_order`: `get-collection` first when the write reads its
-- collection, then the permission itself.
def goOrder (p : Perm) : List Perm :=
  -- the collection read, when there is one, then `p`
  if readsCollection p then [.collectionGet, p] else [p]

-- The check: walk the list in order and stop at the first permission the
-- caller does not hold; `none` means it holds them all.
def firstMissing (holds : Perm → Bool) : List Perm → Option Perm
  -- nothing left to check: allowed
  | [] => none
  -- held: go on; not held: this is the refusal
  | q :: qs => if holds q then firstMissing holds qs else some q

-- A collection write's list is `get-collection`, then the write.
theorem goOrder_collection_write (p : Perm) (h : readsCollection p = true) :
    goOrder p = [.collectionGet, p] := by
  -- unfold the list and use that `p` reads its collection
  simp [goOrder, h]

-- Every other permission is checked alone, as before.
theorem goOrder_alone (p : Perm) (h : readsCollection p = false) :
    goOrder p = [p] := by
  -- unfold the list and use that `p` reads nothing first
  simp [goOrder, h]

-- The permission asked for is always on its own list.
theorem self_in_goOrder (p : Perm) : p ∈ goOrder p := by
  -- both shapes of the list end in `p`
  unfold goOrder; split <;> simp

-- The check lets a caller through exactly when it holds everything listed.
theorem firstMissing_none_iff (holds : Perm → Bool) (l : List Perm) :
    firstMissing holds l = none ↔ ∀ q ∈ l, holds q = true := by
  -- one permission at a time, along the list
  induction l with
  -- the empty list: allowed, and there is nothing to hold
  | nil => simp [firstMissing]
  -- one more permission: allowed iff it is held and the rest are allowed
  | cons q qs ih =>
    -- split on whether the first one is held
    by_cases hq : holds q = true <;> simp [firstMissing, hq, ih]

-- A refusal names a permission on the list that the caller lacks.
theorem firstMissing_some (holds : Perm → Bool) (l : List Perm) (q : Perm)
    (h : firstMissing holds l = some q) : q ∈ l ∧ holds q = false := by
  -- one permission at a time, along the list
  induction l with
  -- the empty list refuses nothing
  | nil => simp [firstMissing] at h
  -- one more permission: either it is the refusal or the rest refused
  | cons r rs ih =>
    -- split on whether the first one is held
    by_cases hr : holds r = true
    -- held: the refusal came from the rest of the list
    · simp [firstMissing, hr] at h
      exact ⟨List.mem_cons_of_mem r (ih h).1, (ih h).2⟩
    -- not held: it is the refusal itself
    · simp [firstMissing, hr] at h
      subst h
      exact ⟨List.mem_cons_self _ _, by simpa using hr⟩

-- Granted the collection read, every collection write needs that and itself.
theorem allowed_needs_both (holds : Perm → Bool) (p : Perm)
    (h : readsCollection p = true)
    (ok : firstMissing holds (goOrder p) = none) :
    holds .collectionGet = true ∧ holds p = true := by
  -- the list is the read then the write, and every listed permission is held
  rw [goOrder_collection_write p h] at ok
  have all := (firstMissing_none_iff holds _).1 ok
  exact ⟨all _ (by simp), all _ (by simp)⟩

-- A caller without `get-collection` is refused `get-collection` on every
-- collection write, whatever else it holds: Go's answer.
theorem refused_get_collection_first (holds : Perm → Bool) (p : Perm)
    (h : readsCollection p = true) (no : holds .collectionGet = false) :
    firstMissing holds (goOrder p) = some .collectionGet := by
  -- the read is first on the list and it is not held
  rw [goOrder_collection_write p h]
  simp [firstMissing, no]

/-!
## Reading past document access control

Go's `canDACBypass`: a caller reads past document access control only when
NAC is on and grants it `bypass-dac`. With NAC off a permission check says
yes to everything, so the bypass must ask whether NAC is on first. A caller
that names no identity never bypasses.

Rust owner: crates/db/src/read/lensed/fetcher/mod.rs
`LensedDocFetcher::dac_bypass`, called from crates/query/src/runner/turn.rs
`QueryRunner::lent`.
-/

-- The bypass: an identity is named, NAC is on, and it holds `bypass-dac`.
def dacBypass (named nacOn holdsBypass : Bool) : Bool :=
  -- all three must hold
  named && nacOn && holdsBypass

-- With NAC off nobody bypasses, whatever a permission check would say.
theorem no_bypass_when_off (named holds : Bool) : dacBypass named false holds = false := by
  -- the middle term is false
  simp [dacBypass]

-- A caller that names no identity never bypasses.
theorem no_bypass_anonymous (on holds : Bool) : dacBypass false on holds = false := by
  -- the first term is false
  simp [dacBypass]

-- With NAC on, a named caller bypasses exactly when it holds `bypass-dac`:
-- the node owner, who holds every node permission, reads every document.
theorem bypass_iff_held (holds : Bool) : dacBypass true true holds = holds := by
  -- the first two terms are true
  simp [dacBypass]

end NacOrder
