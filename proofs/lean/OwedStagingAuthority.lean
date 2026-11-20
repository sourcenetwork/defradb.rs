/-! The story. Before a node stages a relationship change for an external
provider, it asks whether revocations already owed on the object may have
taken the actor's authority away: the provider has not applied them yet, so
it would still say yes. Authority to change a relation rests on the tuples
of the relations its managers are computed from (the sources). This file
decides it and proves that only a revocation of a source naming the actor or
everyone takes the authority, whoever staged it.

The code: `rests_on_revoked` in `crates/engine/acp/src/outbox/staging.rs`. -/
namespace OwedStagingAuthority

-- Whom an owed delete names.
inductive Subject where
  -- One actor.
  | actor (name : String)
  -- More than one actor: everyone, a typed wildcard or a userset.
  | wide
  deriving DecidableEq, Repr

-- One owed delete: its relation, its subject and the actor that staged it.
structure Delete where
  -- The relation of the revoked tuple.
  relation : String
  -- Whom the tuple named.
  subject : Subject
  -- Who staged the revocation.
  stagedBy : String
  deriving Repr

-- Whether a delete's subject covers the actor.
def covers (subject : Subject) (actor : String) : Bool :=
  -- One actor covers only itself; a wider subject covers everyone.
  match subject with
  | .actor name => name == actor
  | .wide => true

/-- Whether one of `deletes` takes away the actor's authority: a delete of
one of `sources` covering the actor. Without the policy here the sources
are unknown (`none`), and a delete of any relation covering the actor does. -/
def restsOnRevoked (sources : Option (List String)) (deletes : List Delete) (actor : String) : Bool :=
  -- Any delete of a source relation (any relation, when unknown) that covers the actor.
  deletes.any fun d => (sources.map (·.contains d.relation)).getD true && covers d.subject actor

/-- A revocation of a relation the authority does not rest on never takes it. -/
theorem other_relation_spares (sources : List String) (d : Delete) (actor : String)
    (h : d.relation ∉ sources) :
    restsOnRevoked (some sources) [d] actor = false := by
  -- The only delete fails its first conjunct.
  simp [restsOnRevoked, h]

/-- A revocation of a source naming everyone takes the authority. -/
theorem wide_source_takes (sources : List String) (d : Delete) (actor : String)
    (h : d.relation ∈ sources) (w : d.subject = .wide) :
    restsOnRevoked (some sources) [d] actor = true := by
  -- Both conjuncts hold for the only delete.
  simp [restsOnRevoked, h, w, covers]

/-- An actor that revoked its own source keeps no authority from it: who
staged a delete does not enter the decision. -/
theorem staged_by_self_still_takes (sources : List String) (d : Delete)
    (h : d.relation ∈ sources) (n : d.subject = .actor d.stagedBy) :
    restsOnRevoked (some sources) [d] d.stagedBy = true := by
  -- The delete names the very actor that staged it.
  simp [restsOnRevoked, h, n, covers]

/-- Without the policy, a delete naming everyone takes the authority, whatever
its relation: the old, wider rule. -/
theorem unknown_sources_any_wide_takes (d : Delete) (actor : String)
    (w : d.subject = .wide) :
    restsOnRevoked none [d] actor = true := by
  -- The unknown sources accept the relation, and a wide subject covers everyone.
  simp [restsOnRevoked, w, covers]

/-- Without the policy, a delete naming another actor still spares this one. -/
theorem unknown_sources_other_actor_spares (d : Delete) (other actor : String)
    (n : d.subject = .actor other) (h : other ≠ actor) :
    restsOnRevoked none [d] actor = false := by
  -- The subject names someone else.
  simp [restsOnRevoked, n, covers, h]

end OwedStagingAuthority
