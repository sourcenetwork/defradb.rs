/-!
Deciding a collection `create` from checks on the objects a document names.

A policy resource may say who may add a document to its collection, such as
`create: author->publish`: the document names an author through a field, and
whoever creates it must hold `publish` on that author. The document does not
exist yet, so the only relationships it has are the ones the node derives
from its fields (`book:<new>#author@author:<id>`). acp-rs decides `create`
over those derived relationships plus the stored grants.

The node decides the same thing before the create's transaction (and the
local engine inside it), one permission check per linked object: an arrow
`r->p` holds when some derived relationship of relation `r` links an object
on which the creator holds `p`. Unions, intersections and differences combine
the answers.

What must never happen: the node's answer differs from acp-rs's, so a create
acp-rs refuses passes the node (and takes a document ID), or one it admits is
refused. Also: an arrow whose relation the document does not carry (its field
was null) never holds, and a name that is not an arrow (which acp-rs refuses
in `create`) never admits anything.

Rust owner: `authorize_create` and `admits` in
`crates/engine/acp/src/create_decision.rs`, over `RelationExpression` in
`crates/building-blocks/zanzibar/src/expression/mod.rs`.
-/
namespace CreateDecision

/-- A relationship the node derives from the new document's fields. -/
structure Derived where
  -- The relation it is in, such as `author`.
  relation : String
  -- The object it links, such as `author:<id>`.
  target : String
  deriving DecidableEq, Repr

/-- A `create` expression, as the parser builds it. -/
inductive Expr where
  -- `r->p`: a linked object through relation `r`, on which `p` holds.
  | arrow (relation computed : String)
  -- Any other name (`_this`, a relation or permission of the same
  -- resource); acp-rs refuses these in `create`.
  | other (name : String)
  -- `a + b`.
  | union (a b : Expr)
  -- `a & b`.
  | inter (a b : Expr)
  -- `a - b`.
  | diff (a b : Expr)
  deriving Repr

/-- acp-rs's answer: an arrow holds when some derived relationship of its
relation links an object on which the creator holds the computed name, as
the grants say (`grant computed target`). Other names hold nothing: the new
object has no relationships but the derived ones, whose subjects are objects,
not the creator. -/
def reference (grant : String → String → Bool) (derived : List Derived) : Expr → Prop
  -- Some derived relationship of this relation links a granted object.
  | .arrow r p => ∃ d ∈ derived, d.relation = r ∧ grant p d.target = true
  -- Nothing on the new object names the creator.
  | .other _ => False
  -- Either side holds.
  | .union a b => reference grant derived a ∨ reference grant derived b
  -- Both sides hold.
  | .inter a b => reference grant derived a ∧ reference grant derived b
  -- The first holds and the second does not.
  | .diff a b => reference grant derived a ∧ ¬ reference grant derived b

/-- The node's decision: each arrow asks the permission check of every
linked object of its relation, in order, and stops at the first yes. -/
def admits (check : String → String → Bool) (derived : List Derived) : Expr → Bool
  -- Filter the derived relationships to this relation, then ask each.
  | .arrow r p => (derived.filter (fun d => d.relation = r)).any (fun d => check p d.target)
  -- Never admits.
  | .other _ => false
  -- Either answer.
  | .union a b => admits check derived a || admits check derived b
  -- Both answers.
  | .inter a b => admits check derived a && admits check derived b
  -- The first answer and not the second.
  | .diff a b => admits check derived a && !admits check derived b

/-- When the node's checks answer as the grants do, the node's decision is
acp-rs's, for every expression and every set of derived relationships. -/
theorem decide_is_reference (grant : String → String → Bool) (derived : List Derived) :
    ∀ e, admits grant derived e = true ↔ reference grant derived e := by
  -- By the shape of the expression.
  intro e
  induction e with
  | arrow r p =>
    -- An arrow: `any` over the filtered list is "some member satisfies".
    simp [admits, reference, List.any_eq_true, List.mem_filter, and_assoc]
  | other n =>
    -- Never true on either side.
    simp [admits, reference]
  | union a b iha ihb =>
    -- `||` is `∨`, side by side.
    simp [admits, reference, iha, ihb]
  | inter a b iha ihb =>
    -- `&&` is `∧`, side by side.
    simp [admits, reference, iha, ihb]
  | diff a b iha ihb =>
    -- `&& !` is "and not": write each side's answer as a Boolean, rewrite
    -- the references by the two answers, then try each pair of answers.
    show (admits grant derived a && !admits grant derived b) = true ↔
      (reference grant derived a ∧ ¬ reference grant derived b)
    rw [← iha, ← ihb]
    cases admits grant derived a <;> cases admits grant derived b <;> simp

/-- An arrow through a relation the document carries no relationship of (its
bound field was null) never admits, whatever the checks say. -/
theorem arrow_needs_its_relation (check : String → String → Bool)
    (derived : List Derived) (r p : String)
    (none : ∀ d ∈ derived, d.relation ≠ r) :
    admits check derived (.arrow r p) = false := by
  -- No member survives the filter, so `any` is false.
  rw [admits, Bool.eq_false_iff]
  -- Suppose some member answered yes: it passed the filter, so its relation
  -- is `r`, which no member's is.
  intro h
  obtain ⟨d, hmem, _⟩ := List.any_eq_true.mp h
  have hd := List.mem_filter.mp hmem
  exact none d hd.1 (by simpa using hd.2)

/-- A name that is not an arrow admits nothing, alone or as the base of a
difference. -/
theorem other_never_admits (check : String → String → Bool) (derived : List Derived)
    (n : String) (e : Expr) :
    admits check derived (.other n) = false ∧
      admits check derived (.diff (.other n) e) = false := by
  -- Both reduce to `false` by definition.
  simp [admits]

end CreateDecision
