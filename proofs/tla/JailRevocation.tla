---- MODULE JailRevocation ----
\* The story. Only admitted keys may write a contended resource. An admin
\* admits keys, and its admissions reach the nodes over time. Admission is
\* the cost of an identity: the admin admits at most Budget keys for the
\* cheater, however many keys the cheater makes (making a key is free).
\*
\* A signed fork is revocation. A node that holds two signed blocks by one key
\* at one height treats that key as no longer a member, with no admin step:
\* the proof is enough, and the node passes it on (ChainJail.tla covers why
\* only a signed fork counts and why the proof must travel on its own).
\*
\* An honest writer is admitted too. Its block can reach a node before the
\* admission does; the node keeps the block and merges it once it learns of
\* the admission.
\*
\* What must never happen: a node merges a block from a key it was never told
\* is admitted; a node merges a block from a key it holds a fork proof
\* against. What must always happen: an admitted honest writer's block ends
\* up merged on every node.
EXTENDS Naturals, FiniteSets

CONSTANTS
  Nodes,             \* the honest nodes
  Peers,             \* who each node can talk to
  CheaterKeys,       \* every key the cheater could make
  HonestKey,         \* the honest writer's key
  WriterHome,        \* the node the honest writer writes on
  Budget,            \* how many cheater keys the admin will admit
  CheckMembership,   \* GREEN TRUE; RED_FreeKeys FALSE: merge from any key
  CheckEvidence,     \* GREEN TRUE; RED_AclOnly FALSE: membership ignores the fork proof
  RetryOnAdmission   \* GREEN TRUE; RED_DropEarly FALSE: a block that came early is never merged

Keys == CheaterKeys \cup {HonestKey}   \* every key that can sign

ASSUME HonestKey \notin CheaterKeys                          \* the writer is not the cheater
ASSUME WriterHome \in Nodes                                  \* it writes on a real node
ASSUME Peers \in [Nodes -> SUBSET Nodes]                     \* links between nodes
ASSUME \A n \in Nodes : \A m \in Peers[n] : n \in Peers[m]   \* links go both ways
ASSUME Budget \in Nat                                        \* a count of admissions
ASSUME CheckMembership \in BOOLEAN /\ CheckEvidence \in BOOLEAN /\ RetryOnAdmission \in BOOLEAN

\* A block: its signing key, the version (two versions are a fork). One height
\* is enough here: the fork itself is the only conflict there is.
Block == [a : Keys, v : {1, 2}]

\* admitted: the keys the admin has admitted, in its own log.
\* known[n]: the admissions node n has received.
\* produced: every block signed so far.
\* seen[n]: every block node n received, merged or not, kept as evidence.
\* merged[n]: the blocks node n merged.
\* lateMerge: a ghost flag, set if any node merged a block from a key it held
\*   a fork proof against at that moment.
VARIABLES admitted, known, produced, seen, merged, lateMerge
vars == <<admitted, known, produced, seen, merged, lateMerge>>

TypeOK ==
  /\ admitted \subseteq Keys                  \* the admin admits real keys
  /\ known \in [Nodes -> SUBSET Keys]         \* each node's known admissions
  /\ produced \subseteq Block                 \* only well-formed blocks
  /\ seen \in [Nodes -> SUBSET Block]         \* each node's received blocks
  /\ merged \in [Nodes -> SUBSET Block]       \* each node's merged blocks
  /\ lateMerge \in BOOLEAN                    \* the ghost flag

\* The fork proof against key k inside a set of blocks P.
Forked(P, k) == \E x, y \in P : x.a = k /\ y.a = k /\ x.v # y.v

\* Whether node n, holding blocks s, treats key k as a member right now.
Member(n, s, k) ==
  /\ CheckMembership => k \in known[n]                    \* told it was admitted
  /\ CheckEvidence => ~Forked(s, k)                       \* and holds no fork proof

\* Whether merging a block from k at n, holding s, is a late merge.
Late(s, k) == Forked(s, k)

Init ==
  /\ admitted = {}                            \* nobody admitted yet
  /\ known = [n \in Nodes |-> {}]             \* no node knows any admission
  /\ produced = {}                            \* nothing signed
  /\ seen = [n \in Nodes |-> {}]              \* nothing received
  /\ merged = [n \in Nodes |-> {}]            \* nothing merged
  /\ lateMerge = FALSE                        \* nothing went wrong

\* Node n receives block b: it keeps it, and merges it if the key is a member.
Receive(n, b) ==
  LET s == seen[n] \cup {b}                               \* keep it, always
      ok == Member(n, s, b.a)                             \* member right now?
  IN /\ seen' = [seen EXCEPT ![n] = s]
     /\ merged' = [merged EXCEPT ![n] = IF ok THEN @ \cup {b} ELSE @]
     /\ lateMerge' = (lateMerge \/ (ok /\ Late(s, b.a)))  \* record a late merge

\* The admin admits a key: the honest writer's freely, the cheater's only
\* within Budget. It cannot tell which cheater key is which person's; Budget
\* is how much admitting costs.
Admit(k) ==
  /\ k \notin admitted                                    \* a new admission
  /\ k \in CheaterKeys => Cardinality(admitted \cap CheaterKeys) < Budget  \* the cost
  /\ admitted' = admitted \cup {k}
  /\ UNCHANGED <<known, produced, seen, merged, lateMerge>>

\* An admission reaches node n.
Learn(n, k) ==
  /\ k \in admitted                                       \* the admin admitted it
  /\ k \notin known[n]                                    \* news to n
  /\ known' = [known EXCEPT ![n] = @ \cup {k}]
  /\ UNCHANGED <<admitted, produced, seen, merged, lateMerge>>

\* Node n merges a block it kept earlier, now that its key is a member.
Retry(n, b) ==
  /\ RetryOnAdmission                                     \* RED_DropEarly turns this off
  /\ b \in seen[n] \ merged[n]                            \* kept, not merged
  /\ Member(n, seen[n], b.a)                              \* a member now
  /\ merged' = [merged EXCEPT ![n] = @ \cup {b}]
  /\ lateMerge' = (lateMerge \/ Late(seen[n], b.a))
  /\ UNCHANGED <<admitted, known, produced, seen>>

\* The honest writer, once admitted, writes one block on its home node.
Write ==
  LET b == [a |-> HonestKey, v |-> 1]
  IN /\ HonestKey \in admitted                            \* only after admission
     /\ b \notin produced                                 \* writes it once
     /\ produced' = produced \cup {b}
     /\ Receive(WriterHome, b)
     /\ UNCHANGED <<admitted, known>>

\* The cheater signs any block with any key it made, forks included.
Sign(b) ==
  /\ b.a \in CheaterKeys                                  \* its own keys only
  /\ b \notin produced                                    \* a new block
  /\ produced' = produced \cup {b}
  /\ UNCHANGED <<admitted, known, seen, merged, lateMerge>>

\* The cheater shows one of its blocks to any node.
Show(b, n) ==
  /\ b \in produced /\ b.a \in CheaterKeys                \* a block it signed
  /\ b \notin seen[n]                                     \* not shown there yet
  /\ Receive(n, b)
  /\ UNCHANGED <<admitted, known, produced>>

\* A node passes a block it merged on to a peer.
Relay(n, m, b) ==
  /\ m \in Peers[n] /\ b \in merged[n] /\ b \notin seen[m]
  /\ Receive(m, b)
  /\ UNCHANGED <<admitted, known, produced>>

\* A node holding a fork proof against k hands the proof to a peer.
Accuse(n, m, k) ==
  /\ m \in Peers[n]                                       \* only along a link
  /\ Forked(seen[n], k)                                   \* it holds the proof
  /\ ~Forked(seen[m], k)                                  \* the peer does not
  /\ seen' = [seen EXCEPT ![m] = @ \cup {b \in seen[n] : b.a = k}]
  /\ UNCHANGED <<admitted, known, produced, merged, lateMerge>>

Next ==
  \/ \E k \in Keys : Admit(k)
  \/ \E n \in Nodes, k \in Keys : Learn(n, k)
  \/ \E n \in Nodes, b \in Block : Retry(n, b)
  \/ Write
  \/ \E b \in Block : Sign(b)
  \/ \E b \in Block, n \in Nodes : Show(b, n)
  \/ \E n, m \in Nodes, b \in Block : Relay(n, m, b)
  \/ \E n, m \in Nodes, k \in Keys : Accuse(n, m, k)

\* Admissions travel, retries run, relays and proofs are delivered; the admin
\* and the cheater act as they like.
Fairness ==
  /\ \A n \in Nodes, k \in Keys : WF_vars(Learn(n, k))
  /\ \A n \in Nodes, b \in Block : WF_vars(Retry(n, b))
  /\ \A n, m \in Nodes, b \in Block : WF_vars(Relay(n, m, b))
  /\ \A n, m \in Nodes, k \in Keys : WF_vars(Accuse(n, m, k))

Spec == Init /\ [][Next]_vars /\ Fairness

\* Every merged block comes from a key that node was told is admitted, so the
\* cheater gets no more keys in than the admin's Budget.
INV_OnlyAdmittedWrite == \A n \in Nodes : \A b \in merged[n] : b.a \in known[n]

\* The cost holds: no more cheater keys get a block merged anywhere than the
\* admin was willing to admit, however many keys the cheater made.
INV_CheaterWithinBudget ==
  Cardinality({b.a : b \in UNION {merged[n] : n \in Nodes}} \cap CheaterKeys) <= Budget

\* No node ever merges a block from a key it holds a fork proof against.
INV_NoMergeAfterProof == ~lateMerge

\* Once the honest writer's block exists, every node ends up merging it.
HonestBlock == [a |-> HonestKey, v |-> 1]
PROP_AdmittedWritesLand == (HonestBlock \in produced) ~> (\A n \in Nodes : HonestBlock \in merged[n])
====
