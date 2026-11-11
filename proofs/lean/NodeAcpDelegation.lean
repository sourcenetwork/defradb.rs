import Lean

/-!
The story. A node's access control (NAC) lists who may do what on the node.
The owner and the admins may do everything. Anyone else holds only the node
permissions granted to them, directly or through a chain of named sources.

Who acts: one requester asks to give a subject a relation on the node, or to
take one away. The relation is `admin` or one node permission, and the edge
is a direct grant or one edge of a source chain.

What can happen. An admin (or the owner) may change any relation, as before.
A requester who is not an admin is a delegate. A delegate may change one
permission edge when all of these hold:
- it holds the operation it performs (`add-nac-relation` to add,
  `delete-nac-relation` to delete);
- it holds the permission the edge carries;
- that permission is not in the admin class;
- the edge's subject is a concrete DID or a source, never `*`;
- when deleting, the subject is not the owner or an admin.

What must never happen. A delegate never gives anyone a permission it does
not hold itself. A delegate never changes who holds an admin-class
permission, never changes `admin` or `owner`, and never grants to everyone.
An admin's authority is exactly what it was before delegation existed.

Rust code it models:
- `crates/acp/src/nac/node_acp/operations.rs`: `NodeACP::authorize_change`,
  the one gate for `add_admin`, `remove_admin`, `add_permission_grant` and
  `remove_permission_grant`.
- `crates/acp/src/nac/node_acp/source.rs`: `NodeACP::change_source_permission`,
  through the same gate.
- `crates/acp/src/nac/delegation.rs`: `DelegationFacts::admits`, the decision
  this file's `admits` states.
-/
namespace NodeAcpDelegation
open Lean

/-- The admin class: the permissions that widen authority or switch
enforcement off, by wire name.

- `bypass-dac` skips document access control altogether.
- `add-nac-relation` and `delete-nac-relation` are the delegation authority
  itself. A delegate that could hand them out would create new delegates
  without bound.
- `disable-nac`, `re-enable-nac` and `purge-nac` switch node enforcement off,
  back on with the stored grants, or erase it.
- `enable-dac`, `disable-dac` and `purge-dac` do the same for document
  enforcement.

No other permission is in the class. Every other permission acts inside an
enforcement that checks it again (a document relation still needs its DAC
manager, a P2P or collection operation still needs its own node permission),
so holding it cannot widen what any other permission allows. -/
def adminClass : List String :=
  ["bypass-dac", "add-nac-relation", "delete-nac-relation", "disable-nac",
   "re-enable-nac", "purge-nac", "enable-dac", "disable-dac", "purge-dac"]

-- Whether permission `p` is in the admin class.
def isAdminClass (p : String) : Bool := adminClass.contains p

-- The operation a change performs: adding needs `add-nac-relation`, deleting
-- needs `delete-nac-relation`.
def operation (delete : Bool) : String :=
  if delete then "delete-nac-relation" else "add-nac-relation"

/-- Who a relationship names: one DID, everyone (`*`), or one named source
(a userset `node:source.<hex>#<permission>`). -/
inductive Subject where
  | did (name : String)
  | wildcard
  | source (name : String)
  deriving BEq, DecidableEq, Repr

-- One stored edge of a permission: the object it sits on (the node singleton
-- or a source) and the subject it names.
structure Tuple where
  object : String
  subject : Subject
  deriving BEq, DecidableEq, Repr

/-- The relation a change targets. `owner` has no constructor: the native
parser refuses it, so no request can name it. -/
inductive Relation where
  | admin
  | permission (name : String)
  deriving BEq, DecidableEq, Repr

/-- The stored NAC relationships: the owner, the direct admin tuples, and the
edges stored under each permission's relation.

External premise (`crates/acp/src/nac/policy.rs`, `create_node_policy`, and
`crates/acp/src/nac/node_acp/source.rs`, `source_relationship`): every
permission relation is `this + owner + admin`, and every source userset a
permission's edge names carries that same permission. So whether a DID holds
permission `p` depends only on the owner, the admin tuples and the edges
stored under `p`. -/
structure State where
  owner : String
  admins : List Subject
  edges : String → List Tuple

-- An admin is the owner, a DID with a direct admin tuple, or anyone while
-- `*` is an admin.
def State.isAdmin (s : State) (d : String) : Bool :=
  s.owner == d || s.admins.contains (.did d) || s.admins.contains .wildcard

/-- How a permission's own edges decide a DID: the Zanzibar walk from the
singleton through any chain of sources. Left abstract: every theorem holds
for any such walk, whatever the source graph looks like. -/
abbrev Reach := List Tuple → String → Bool

-- DID `d` holds permission `p`: as an admin, or through `p`'s own edges.
def holds (reach : Reach) (s : State) (p d : String) : Bool :=
  s.isAdmin d || reach (s.edges p) d

-- One requested change: the relation, the edge, and whether it deletes.
structure Change where
  relation : Relation
  tuple : Tuple
  delete : Bool

-- Whether a subject names one party, not everyone.
def concrete : Subject → Bool
  | .wildcard => false
  | _ => true

-- Whether a subject is the owner or an admin, whose grants are implied.
def privileged (s : State) : Subject → Bool
  | .did d => s.isAdmin d
  | .wildcard => true
  | .source _ => false

/-- The delegate rule. A permission edge only, never `admin`; a permission
outside the admin class that the requester holds; the operation held too;
a concrete subject; and no deletion of an owner's or admin's tuple.

External premise: grants carry no provenance (a relationship stores only
resource, object, relation and subject). So a delegate holding `p` may
revoke someone else's direct `p` grant, whoever wrote it. -/
def delegated (reach : Reach) (s : State) (r : String) (c : Change) : Bool :=
  match c.relation with
  | .admin => false
  | .permission p =>
      !isAdminClass p && holds reach s (operation c.delete) r && holds reach s p r &&
        concrete c.tuple.subject && !(c.delete && privileged s c.tuple.subject)

-- A change is authorized for an admin, as before, or under the delegate rule.
def authorized (reach : Reach) (s : State) (r : String) (c : Change) : Bool :=
  s.isAdmin r || delegated reach s r c

-- Applying a change adds or removes exactly its one tuple.
def apply (s : State) (c : Change) : State :=
  match c.relation with
  | .admin =>
      { s with admins :=
          if c.delete then s.admins.filter (· != c.tuple.subject)
          else c.tuple.subject :: s.admins }
  | .permission p =>
      { s with edges := fun q =>
          if q = p then
            (if c.delete then (s.edges p).filter (· != c.tuple) else c.tuple :: s.edges p)
          else s.edges q }

-- The gate: an authorized change is applied; any other is refused.
def step (reach : Reach) (s : State) (r : String) (c : Change) : Option State :=
  if authorized reach s r c then some (apply s c) else none

-- The rule before delegation: only an admin changes anything.
def adminStep (s : State) (r : String) (c : Change) : Option State :=
  if s.isAdmin r then some (apply s c) else none

-- A permission change leaves the owner and the admin tuples alone.
theorem apply_permission_frame (s : State) (c : Change) (p : String)
    (h : c.relation = .permission p) :
    (apply s c).owner = s.owner ∧ (apply s c).admins = s.admins := by
  simp [apply, h]

-- A permission change leaves every other permission's edges alone.
theorem apply_other_edges (s : State) (c : Change) (p q : String)
    (h : c.relation = .permission p) (hq : q ≠ p) :
    (apply s c).edges q = s.edges q := by
  simp [apply, h, hq]

-- A permission change leaves who is an admin alone.
theorem apply_permission_isAdmin (s : State) (c : Change) (p d : String)
    (h : c.relation = .permission p) : (apply s c).isAdmin d = s.isAdmin d := by
  simp [State.isAdmin, apply, h]

/-- An accepted delegated change was a permission change that met every
condition of the delegate rule. -/
theorem delegated_facts (reach : Reach) (s s' : State) (r : String) (c : Change)
    (notAdmin : s.isAdmin r = false) (hs : step reach s r c = some s') :
    ∃ p, c.relation = .permission p ∧ isAdminClass p = false ∧
      holds reach s (operation c.delete) r = true ∧ holds reach s p r = true ∧
      concrete c.tuple.subject = true ∧
      (c.delete && privileged s c.tuple.subject) = false ∧ s' = apply s c := by
  unfold step authorized at hs
  rw [notAdmin, Bool.false_or] at hs
  split at hs
  · rename_i ok
    cases hc : c.relation with
    | admin => simp [delegated, hc] at ok
    | permission p =>
      simp [delegated, hc] at ok
      obtain ⟨⟨⟨⟨a, b⟩, d⟩, e⟩, f⟩ := ok
      refine ⟨p, rfl, a, b, d, e, ?_, ?_⟩
      · cases hd : c.delete <;> simp_all
      · injection hs with hs; exact hs.symm
  · cases hs

/-- No escalation: after a delegated change, every permission any DID holds
was held by that DID before, or was held by the requester. Every admin-class
permission is held by exactly the same DIDs as before. -/
theorem delegated_no_escalation (reach : Reach) (s s' : State) (r : String) (c : Change)
    (notAdmin : s.isAdmin r = false) (hs : step reach s r c = some s') :
    (∀ d q, holds reach s' q d = true → holds reach s q d = true ∨ holds reach s q r = true) ∧
    (∀ d q, isAdminClass q = true → holds reach s' q d = holds reach s q d) := by
  obtain ⟨p, hp, notClass, _, heldP, _, _, rfl⟩ := delegated_facts reach s s' r c notAdmin hs
  constructor
  · intro d q held
    by_cases hq : q = p
    · subst hq; exact Or.inr heldP
    · left
      simpa [holds, apply_permission_isAdmin s c p d hp, apply_other_edges s c p q hp hq]
        using held
  · intro d q inClass
    have hq : q ≠ p := by
      intro eq; subst eq; rw [notClass] at inClass; cases inClass
    simp [holds, apply_permission_isAdmin s c p d hp, apply_other_edges s c p q hp hq]

/-- A delegated change never touches `admin` or `owner`: the owner and the
admin tuples are exactly as before. -/
theorem delegated_never_touches_admin (reach : Reach) (s s' : State) (r : String) (c : Change)
    (notAdmin : s.isAdmin r = false) (hs : step reach s r c = some s') :
    s'.owner = s.owner ∧ s'.admins = s.admins := by
  obtain ⟨p, hp, _, _, _, _, _, rfl⟩ := delegated_facts reach s s' r c notAdmin hs
  exact apply_permission_frame s c p hp

-- A delegate's request to change `admin` is refused.
theorem delegate_admin_refused (reach : Reach) (s : State) (r : String) (c : Change)
    (notAdmin : s.isAdmin r = false) (h : c.relation = .admin) : step reach s r c = none := by
  simp [step, authorized, delegated, notAdmin, h]

-- A delegate cannot change a permission it does not hold.
theorem delegate_unheld_refused (reach : Reach) (s : State) (r p : String) (c : Change)
    (notAdmin : s.isAdmin r = false) (h : c.relation = .permission p)
    (unheld : holds reach s p r = false) : step reach s r c = none := by
  simp [step, authorized, delegated, notAdmin, h, unheld]

-- A delegate cannot change an admin-class permission, even one it holds.
theorem delegate_admin_class_refused (reach : Reach) (s : State) (r p : String) (c : Change)
    (notAdmin : s.isAdmin r = false) (h : c.relation = .permission p)
    (inClass : isAdminClass p = true) : step reach s r c = none := by
  simp [step, authorized, delegated, notAdmin, h, inClass]

-- A delegate without the operation it performs is refused.
theorem delegate_operation_refused (reach : Reach) (s : State) (r : String) (c : Change)
    (notAdmin : s.isAdmin r = false) (noOp : holds reach s (operation c.delete) r = false) :
    step reach s r c = none := by
  cases h : c.relation <;> simp [step, authorized, delegated, notAdmin, h, noOp]

-- A delegate never grants to, or revokes from, everyone.
theorem delegate_wildcard_refused (reach : Reach) (s : State) (r : String) (c : Change)
    (notAdmin : s.isAdmin r = false) (h : c.tuple.subject = .wildcard) :
    step reach s r c = none := by
  cases hr : c.relation <;> simp [step, authorized, delegated, concrete, notAdmin, hr, h]

-- A delegate cannot delete an owner's or an admin's tuple.
theorem delegate_privileged_delete_refused (reach : Reach) (s : State) (r : String)
    (c : Change) (notAdmin : s.isAdmin r = false) (del : c.delete = true)
    (priv : privileged s c.tuple.subject = true) : step reach s r c = none := by
  cases hr : c.relation <;> simp [step, authorized, delegated, notAdmin, hr, del, priv]

/-- Why those tuples are off limits: the owner and every admin hold every
permission without them, so deleting one would change no decision today and
only matters once that admin is removed. -/
theorem privileged_holds_everything (reach : Reach) (s : State) (p d : String)
    (h : s.isAdmin d = true) : holds reach s p d = true := by
  simp [holds, h]

-- A delegate holding the operation and a non-admin-class permission may add
-- that permission's edge for a concrete subject.
theorem delegated_add_allowed (reach : Reach) (s : State) (r p : String) (c : Change)
    (h : c.relation = .permission p) (notClass : isAdminClass p = false)
    (op : holds reach s (operation false) r = true) (held : holds reach s p r = true)
    (subject : concrete c.tuple.subject = true) (add : c.delete = false) :
    step reach s r c = some (apply s c) := by
  simp_all [step, authorized, delegated]

-- A delegate holding the operation and the permission may delete that
-- permission's edge from a concrete, unprivileged subject.
theorem delegated_delete_allowed (reach : Reach) (s : State) (r p : String) (c : Change)
    (h : c.relation = .permission p) (notClass : isAdminClass p = false)
    (op : holds reach s (operation true) r = true) (held : holds reach s p r = true)
    (subject : concrete c.tuple.subject = true) (del : c.delete = true)
    (plain : privileged s c.tuple.subject = false) :
    step reach s r c = some (apply s c) := by
  simp_all [step, authorized, delegated]

/-- Existing admin behavior is preserved: an admin's change is applied exactly
as under the admin-only rule. -/
theorem admin_behavior_preserved (reach : Reach) (s : State) (r : String) (c : Change)
    (h : s.isAdmin r = true) : step reach s r c = adminStep s r c := by
  simp [step, adminStep, authorized, h]

-- Every change the admin-only rule accepted is still accepted, unchanged.
theorem admin_acceptances_kept (reach : Reach) (s s' : State) (r : String) (c : Change)
    (h : adminStep s r c = some s') : step reach s r c = some s' := by
  unfold adminStep at h
  split at h
  · rename_i admin; rw [admin_behavior_preserved reach s r c admin]; simp [adminStep, admin, h]
  · cases h

-- The delegation authority is itself in the admin class, so it cannot spread.
theorem delegation_not_transferable (delete : Bool) : isAdminClass (operation delete) = true := by
  cases delete <;> decide

/-- The facts the native gate observes for one request, as the conformance
test serializes them. -/
structure Facts where
  requestorAdmin : Bool
  relation : String
  holdsOperation : Bool
  holdsPermission : Bool
  wildcardSubject : Bool
  privilegedSubject : Bool
  delete : Bool
  deriving FromJson, ToJson

/-- The decision on observed facts (`DelegationFacts::admits` in Rust). -/
def admits (f : Facts) : Bool :=
  f.requestorAdmin ||
    (f.relation != "admin" && !isAdminClass f.relation && f.holdsOperation &&
      f.holdsPermission && !f.wildcardSubject && !(f.delete && f.privilegedSubject))

-- The relation's wire name.
def Relation.name : Relation → String
  | .admin => "admin"
  | .permission p => p

-- The facts a state, requester and change present to the gate.
def observe (reach : Reach) (s : State) (r : String) (c : Change) : Facts :=
  { requestorAdmin := s.isAdmin r
    relation := c.relation.name
    holdsOperation := holds reach s (operation c.delete) r
    holdsPermission := match c.relation with
      | .admin => false
      | .permission p => holds reach s p r
    wildcardSubject := !concrete c.tuple.subject
    privilegedSubject := privileged s c.tuple.subject
    delete := c.delete }

/-- The fact-level decision is the state-level one, for every permission
except one literally named `admin` (no node permission has that name). -/
theorem admits_observe (reach : Reach) (s : State) (r : String) (c : Change)
    (named : c.relation ≠ .permission "admin") :
    admits (observe reach s r c) = authorized reach s r c := by
  cases h : c.relation with
  | admin => simp [admits, observe, authorized, delegated, Relation.name, h]
  | permission p =>
    have hp : p ≠ "admin" := by intro eq; subst eq; exact named h
    have ne : (p != "admin") = true := by simp [hp]
    cases hs : c.tuple.subject <;> cases hd : c.delete <;>
      simp [admits, observe, authorized, delegated, Relation.name, concrete, h, hp, ne, hs, hd,
        Bool.and_assoc, Bool.and_comm, Bool.and_left_comm]

-- Emit the modeled decision for serialized native conformance inputs.
def evaluate (json : Json) : Except String Json := do
  let inputs : Array Facts ← fromJson? json
  pure <| toJson <| inputs.map fun input => Json.mkObj
    [("input", toJson input), ("admits", toJson (admits input))]
end NodeAcpDelegation

-- The native test sends its actual inputs to this executable model.
def main : IO Unit := do
  let input ← (← IO.getStdin).getLine
  match Lean.Json.parse input >>= NodeAcpDelegation.evaluate with
  | .ok output => IO.println output.compress
  | .error error => throw <| IO.userError error
