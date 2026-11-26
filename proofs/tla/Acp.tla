---- MODULE Acp ----
\* The story, in plain words.
\*
\* Who acts: one authority that holds the real permission tuples (Vera, or
\* the owner node's own tuple store), and a few nodes that each keep a
\* replicated copy of those tuples and answer access checks from it.
\*
\* What can happen: the authority grants a tuple or revokes one; a grant or a
\* revocation travels to each node over the network; a node answers an
\* access check.
\*
\* What must never happen: once a revocation has reached a node, that node
\* still says yes for the revoked tuple. No node keeps a cache of earlier
\* answers: a Vera node asks Vera every time (`VeraDocumentACP` keeps no
\* decision cache of its own), so a node's only state is its tuple copy.
\*
\* Rust it models:
\*   crates/engine/acp/src/local.rs check and add/delete relationship
\*   crates/engine/acp/src/persistent.rs tuple put/delete/has
\*   crates/engine/vera/src/dac.rs VeraDocumentACP::check_doc_access
\*   crates/engine/vera/src/decisions.rs RemoteDecisions (an answer is reused
\*     only at the verified state it was read at)
EXTENDS FiniteSets
\* FiniteSets is the standard set library; only plain set operations are used.

CONSTANTS
  \* The nodes that answer access checks.
  Nodes,
  \* Every permission tuple the scenario talks about.
  Tuples,
  \* The tuples the authority holds at the start.
  InitialAuthority,
  \* What each node's copy holds at the start.
  InitiallyKnown,
  \* The tuples the authority may grant during the run.
  Grantable,
  \* The tuples the authority may revoke during the run.
  Revocable,
  \* Whether grants and revocations can reach the nodes at all.
  NetworkConnected,
  \* How a node applies a revocation it receives: correctly, or (in a RED
  \* run) by keeping the tuple anyway.
  EnforcementMode

\* The two ways a node can apply a revocation. "CurrentStore" removes the
\* tuple from its copy, as the code does; "StaleStore" plants the bug of
\* keeping it.
Modes == {"CurrentStore", "StaleStore"}
\* Every (node, tuple) pair, used to record which checks said yes.
NodeTuples == Nodes \X Tuples

\* There is at least one node.
ASSUME Nodes # {}
\* There is at least one tuple.
ASSUME Tuples # {}
\* The authority starts with tuples from the scenario only.
ASSUME InitialAuthority \subseteq Tuples
\* Each node's copy starts with tuples from the scenario only.
ASSUME InitiallyKnown \in [Nodes -> SUBSET Tuples]
\* Only scenario tuples can be granted.
ASSUME Grantable \subseteq Tuples
\* Only scenario tuples can be revoked.
ASSUME Revocable \subseteq Tuples
\* A tuple can only be revoked if the authority held it at the start.
ASSUME Revocable \subseteq InitialAuthority
\* The network is either up or down for the whole run.
ASSUME NetworkConnected \in BOOLEAN
\* The mode is one of the two above.
ASSUME EnforcementMode \in Modes

VARIABLES
  \* The tuples the authority holds right now.
  authority,
  \* Each node's replicated copy of the tuples.
  known,
  \* Every tuple the authority has revoked so far.
  revoked,
  \* For each node, the revocations that have reached it.
  seenRevoke,
  \* The (node, tuple) checks that said yes, kept for inspection.
  checked

\* All the variables together, for stuttering steps and fairness.
vars == <<authority, known, revoked, seenRevoke, checked>>

\* Every variable holds the kind of value it should.
TypeOK ==
  \* The authority holds a set of tuples.
  /\ authority \subseteq Tuples
  \* Each node holds a set of tuples.
  /\ known \in [Nodes -> SUBSET Tuples]
  \* The revoked tuples are a set of tuples.
  /\ revoked \subseteq Tuples
  \* Each node has seen a set of revocations.
  /\ seenRevoke \in [Nodes -> SUBSET Tuples]
  \* The recorded checks are (node, tuple) pairs.
  /\ checked \subseteq NodeTuples

\* The start: the authority and the copies as configured, nothing revoked,
\* nothing seen, nothing checked.
Init ==
  /\ authority = InitialAuthority
  /\ known = InitiallyKnown
  /\ revoked = {}
  /\ seenRevoke = [n \in Nodes |-> {}]
  /\ checked = {}

\* A node says yes for a tuple exactly when its copy holds it: there is no
\* cache of earlier answers to consult.
CheckAllowed(n, t) == t \in known[n]

\* The authority grants a tuple it may grant, does not hold, and never
\* revoked.
Grant(t) ==
  /\ t \in Grantable
  /\ t \notin authority
  /\ t \notin revoked
  \* The authority now holds it.
  /\ authority' = authority \cup {t}
  \* Nothing else changes yet: the nodes learn of it later.
  /\ UNCHANGED <<known, revoked, seenRevoke, checked>>

\* The authority revokes a tuple it may revoke and still holds.
Revoke(t) ==
  /\ t \in Revocable
  /\ t \in authority
  \* The authority no longer holds it.
  /\ authority' = authority \ {t}
  \* And it is on record as revoked.
  /\ revoked' = revoked \cup {t}
  \* The nodes learn of it later.
  /\ UNCHANGED <<known, seenRevoke, checked>>

\* A grant the authority holds reaches node n, whose copy lacks it.
ReplicateGrant(n, t) ==
  /\ NetworkConnected
  /\ n \in Nodes
  /\ t \in authority
  /\ t \notin known[n]
  \* The node's copy now holds it.
  /\ known' = [known EXCEPT ![n] = @ \cup {t}]
  /\ UNCHANGED <<authority, revoked, seenRevoke, checked>>

\* A revocation reaches node n for the first time.
ReplicateRevoke(n, t) ==
  /\ NetworkConnected
  /\ n \in Nodes
  /\ t \in revoked
  /\ t \notin seenRevoke[n]
  \* The node remembers that this revocation reached it.
  /\ seenRevoke' = [seenRevoke EXCEPT ![n] = @ \cup {t}]
  \* The guard: the node drops the tuple from its copy. The RED mode keeps
  \* it, which is the bug.
  /\ known' =
      [known EXCEPT ![n] =
        IF EnforcementMode = "StaleStore" THEN @ ELSE @ \ {t}]
  /\ UNCHANGED <<authority, revoked, checked>>

\* Node n answers yes to a check for tuple t, and that answer is recorded.
Check(n, t) ==
  /\ n \in Nodes
  /\ t \in Tuples
  /\ CheckAllowed(n, t)
  /\ checked' = checked \cup {<<n, t>>}
  /\ UNCHANGED <<authority, known, revoked, seenRevoke>>

\* Any one of the steps above can happen next.
Next ==
  \/ \E t \in Tuples : Grant(t)
  \/ \E t \in Tuples : Revoke(t)
  \/ \E n \in Nodes, t \in Tuples : ReplicateGrant(n, t)
  \/ \E n \in Nodes, t \in Tuples : ReplicateRevoke(n, t)
  \/ \E n \in Nodes, t \in Tuples : Check(n, t)

\* Revocations are eventually made, and the network eventually delivers
\* every grant and every revocation.
Fairness ==
  /\ \A t \in Revocable : WF_vars(Revoke(t))
  /\ \A n \in Nodes, t \in Tuples : WF_vars(ReplicateGrant(n, t))
  /\ \A n \in Nodes, t \in Tuples : WF_vars(ReplicateRevoke(n, t))

\* The whole behaviour: start, take steps, and be fair.
Spec == Init /\ [][Next]_vars /\ Fairness

\* Promise: once a revocation has reached a node, that node says no for the
\* revoked tuple.
INV_RevocationConsistent ==
  \A n \in Nodes, t \in Tuples :
    t \in revoked /\ t \in seenRevoke[n] => ~CheckAllowed(n, t)

\* Promise: the authority never holds a tuple it revoked.
INV_RevokedNotAuthoritative == revoked \cap authority = {}

\* Promise: with the network up, every revocation the scenario picks is
\* eventually made, reaches every node, and from then on every node says no.
PROP_RevocationEventuallyEnforced ==
  <>[](\A t \in Revocable :
        /\ t \in revoked
        /\ \A n \in Nodes :
             /\ t \in seenRevoke[n]
             /\ ~CheckAllowed(n, t))
====
