------------------------ MODULE OwedStagingAuthority ------------------------
\* The story. A document has an owner, a manager (who holds the `manager`
\* relation, which manages `reader`) and other actors. In one transaction, or
\* in committed ones not yet settled, actors stage revocations on the document
\* that are owed to the external provider. Then an actor stages a `reader`
\* change on the same document. Before staging it, the node asks whether the
\* actor's authority may already be gone: the provider has not applied the
\* revocations yet, so it would still say yes.
\*
\* What must never happen: an actor whose authority to change `reader` rests
\* on a tuple an owed revocation takes away (its own `manager` tuple, or one
\* naming everyone) stages the change, even when it staged that revocation
\* itself; and an actor whose authority rests on no revoked tuple (the owner,
\* or a manager when only a `reader` tuple is revoked) is refused.
\*
\* The code: TransactionOutbox::advise and TransactionOutbox::stage_prepared
\* in crates/engine/acp/src/outbox/staging.rs, with management_sources (the
\* relations the managers of `reader` are computed from) and
\* ops::owed_deletes_on (the deletes owed on the object).
\* Numbers and the size of finite sets are used.
EXTENDS Naturals, FiniteSets

\* Bug: "none", "wide" (any owed delete naming the actor or everyone denies,
\* whatever its relation), "self" (a delete the actor staged itself is not
\* counted against it), or "noOwner" (the owner loses its exemption).
CONSTANT Bug

\* The actors, and "*" for everyone.
Actors == {"owner", "mgr", "other"}
\* The relations: `manager` manages `reader`.
Relations == {"reader", "manager"}
\* The relations the managers of `reader` are computed from.
Sources == {"manager"}
\* The tuples on the document at the start: the manager's, and a reader.
Held == {<<"manager", "mgr">>, <<"reader", "other">>}

\* owed: the deletes owed, as <<relation, subject, staged by>>.
\* wrongRefusal: an actor whose authority rests on no revoked tuple was
\* refused. keptRevoked: an actor whose authority an owed delete takes away
\* staged a change.
VARIABLES owed, wrongRefusal, keptRevoked

\* Every variable, for stuttering steps.
vars == <<owed, wrongRefusal, keptRevoked>>

\* Nothing owed, nothing decided.
Init == owed = {} /\ wrongRefusal = FALSE /\ keptRevoked = FALSE

\* The actor holds the managing relation on the provider.
Manages(a) == <<"manager", a>> \in Held

\* An owed delete takes away what `a`'s authority rests on: a source relation
\* naming `a` or everyone.
AtRisk(a) == \E d \in owed : d[1] \in Sources /\ d[2] \in {a, "*"}

\* What the node checks, with the bug planted where asked.
Revoked(a) ==
  CASE Bug = "wide" -> \E d \in owed : d[2] \in {a, "*"}
    [] Bug = "self" -> \E d \in owed : d[1] \in Sources /\ d[2] \in {a, "*"} /\ d[3] # a
    [] OTHER -> AtRisk(a)

\* The owner may always change; a manager unless its authority was revoked.
Admits(a) == (a = "owner" /\ Bug # "noOwner") \/ (Manages(a) /\ ~Revoked(a))

\* The owner or the manager stages a revocation of a tuple of either
\* relation, for the manager or everyone (whether it may is not this model's
\* question); at most two are owed, to keep the model small.
StageDelete(by, r, s) == /\ Cardinality(owed) < 2
                         /\ owed' = owed \cup {<<r, s, by>>}
                         /\ UNCHANGED <<wrongRefusal, keptRevoked>>

\* An actor stages a `reader` change; the node admits or refuses it.
StageChange(a) ==
  /\ wrongRefusal' = (wrongRefusal \/ (~Admits(a) /\ (a = "owner" \/ (Manages(a) /\ ~AtRisk(a)))))
  /\ keptRevoked' = (keptRevoked \/ (Admits(a) /\ a # "owner" /\ AtRisk(a)))
  /\ UNCHANGED owed

\* Revocations and changes in any order.
Next == \/ \E by \in {"owner", "mgr"}, r \in Relations, s \in {"mgr", "*"} : StageDelete(by, r, s)
        \/ \E a \in Actors : StageChange(a)

\* Steps may also wait.
Spec == Init /\ [][Next]_vars

\* No actor is refused for a revocation its authority does not rest on.
INV_NoOverWideDenial == ~wrongRefusal

\* No actor stages a change on authority an owed revocation takes away,
\* even a revocation it staged itself.
INV_RevokedAuthorityDenies == ~keptRevoked
=============================================================================
