/-!
An explicit grant names one peer/root pair. Native transactions order grants
and revocations. Cache pressure and subscription removal do not change it.
Rust: stores/peerstore/selected_car.rs; event_handler/car.rs.
-/
namespace SelectedCarAuthority
-- A scope contains the authenticated transport peer and exact root CID.
abbrev Scope := Nat × Nat
-- Serving asks for that exact pair, not either component separately.
def allows (grants : List Scope) (peer root : Nat) : Prop := (peer, root) ∈ grants
-- Revocation removes every stored occurrence of only the requested pair.
def revoke (grants : List Scope) (scope : Scope) : List Scope := grants.filter (· != scope)
-- A revoked scope cannot be served from the remaining permissions.
theorem revoked_denied (grants : List Scope) (scope : Scope) :
    scope ∉ revoke grants scope := by
  -- Filtering excludes precisely this pair.
  simp [revoke]
-- Revoking one root does not remove another peer/root's permission.
theorem other_preserved (grants : List Scope) (scope other : Scope) (h : other ≠ scope) :
    other ∈ revoke grants scope ↔ other ∈ grants := by
  -- The other pair passes the exact comparison unchanged.
  simp [revoke, h]
-- A single exact grant cannot authorize a different peer or root.
theorem exact_scope (scope other : Scope) (h : other ≠ scope) : other ∉ [scope] := by
  -- Membership in a singleton requires equality of both components.
  simp [h]
end SelectedCarAuthority
