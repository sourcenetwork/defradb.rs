---- MODULE ChainJail ----
\* The story. Every author writes its own chain: each block names the block
\* before it, and only that author can sign it. An honest author writes one
\* block per height. A cheating author can "fork": sign two different blocks
\* at the same height, and show one version to some nodes and the other
\* version to others.
\*
\* Honest nodes never trust a claim of cheating on its own. They keep every
\* block they receive, and the two signed blocks of one fork are the proof:
\* anyone can check them, and nobody can make them without the author's key.
\* A node that holds the proof "jails" the author: it stops merging that
\* author's blocks and hands the proof to its peers.
\*
\* What must never happen: an honest node is jailed; an honest node merges both
\* sides of a fork. What must always happen: once any honest node can see a
\* fork, every honest node ends up jailing the cheater, even the ones the
\* cheater only ever showed one side to.
\*
\* What this does not show: that cheating is deterred. A jail only costs the
\* cheater the one key it signed with, and a new key is free, so the cheater
\* here is one fixed identity that never makes another. A jailed author that
\* comes back under a new key is stopped only if writing needs an identity
\* that is costly to get. The design takes that cost from admission: only an
\* admitted validator or member may write a contended resource, so a new key
\* has no rights until it is admitted again. Admission is outside this model.
\*
\* How this maps onto DefraDB. A block's heads are the document's previous
\* heads, not its author's last block, so "two versions at one height" stands
\* for two blocks signed by one key on one document where neither is an
\* ancestor of the other. That is only a fork if one key never writes from two
\* places at once, so every instance signs with a key of its own. A did:key
\* DID is its key, so each instance is its own DID; an agent that runs several
\* instances (gents) is an ACP group of their DIDs, and a jail revokes one
\* instance, never the group.
\*
\* This is the design model for the single-writer chain layer of the
\* exactly-one-winner work. There is no Rust for it yet; the anchor in the
\* registry is the signature check the proof relies on.
EXTENDS Naturals, FiniteSets

CONSTANTS
  Honest,          \* the honest nodes
  Cheater,         \* the one cheating author
  Peers,           \* who each honest node can talk to
  Heights,         \* the chain heights the cheater may write at
  CheckEvidence,   \* GREEN TRUE; RED_Hearsay FALSE: jail on any accusation
  EnforceJail,     \* GREEN TRUE; RED_NoJail FALSE: keep merging a jailed author
  GossipEvidence   \* GREEN TRUE; RED_NoGossip FALSE: never pass the proof on

Nodes == Honest \cup {Cheater}   \* everyone who can be accused

ASSUME Cheater \notin Honest                         \* the cheater is not honest
ASSUME Peers \in [Honest -> SUBSET Honest]           \* honest links only
ASSUME \A n \in Honest : \A m \in Peers[n] : n \in Peers[m]  \* links go both ways
ASSUME CheckEvidence \in BOOLEAN /\ EnforceJail \in BOOLEAN /\ GossipEvidence \in BOOLEAN

\* A block: who signed it, its height (standing in for "which parent it names"),
\* and which version it is. Two versions at one height are a fork.
Block == [a : Nodes, h : Heights, v : {1, 2}]

\* produced: every block the cheater has signed.
\* seen[n]: every block node n has received, merged or not, kept as evidence.
\* merged[n]: the blocks node n has merged into its database.
\* jailed[n]: the authors node n refuses to merge.
VARIABLES produced, seen, merged, jailed
vars == <<produced, seen, merged, jailed>>

TypeOK ==
  /\ produced \subseteq Block                 \* only well-formed blocks exist
  /\ seen \in [Honest -> SUBSET Block]        \* each node's received blocks
  /\ merged \in [Honest -> SUBSET Block]      \* each node's merged blocks
  /\ jailed \in [Honest -> SUBSET Nodes]      \* each node's jailed authors

\* The proof of cheating: two signed blocks by author a at one height that differ.
Conflicting(P, a) ==
  \E x, y \in P : x.a = a /\ y.a = a /\ x.h = y.h /\ x.v # y.v

\* Whether a node accepts an accusation of a backed by blocks P. The blocks must
\* really have been signed (P \subseteq produced is the signature boundary: no
\* one forges a block), and with CheckEvidence they must show a fork.
Convinced(P, a) ==
  /\ P \subseteq produced                     \* real signed blocks only
  /\ IF CheckEvidence THEN Conflicting(P, a) ELSE TRUE  \* proof, or hearsay

Init ==
  /\ produced = {}                            \* nothing signed yet
  /\ seen = [n \in Honest |-> {}]             \* nobody has received anything
  /\ merged = [n \in Honest |-> {}]           \* nobody has merged anything
  /\ jailed = [n \in Honest |-> {}]           \* nobody is jailed

\* Node n receives block b: it keeps the block as evidence, jails the author if
\* it now holds a fork, and merges the block only if the author is not jailed.
Receive(n, b) ==
  LET s == seen[n] \cup {b}                                     \* keep it, always
      j == IF Conflicting(s, b.a) THEN jailed[n] \cup {b.a} ELSE jailed[n]  \* fork seen: jail
      ok == ~EnforceJail \/ b.a \notin j                        \* jail blocks the merge
  IN /\ seen' = [seen EXCEPT ![n] = s]
     /\ jailed' = [jailed EXCEPT ![n] = j]
     /\ merged' = [merged EXCEPT ![n] = IF ok THEN @ \cup {b} ELSE @]

\* The cheater signs any block of its own, forks included.
Sign(b) ==
  /\ b.a = Cheater                            \* it can only sign as itself
  /\ b \notin produced                        \* a new block
  /\ produced' = produced \cup {b}
  /\ UNCHANGED <<seen, merged, jailed>>

\* The cheater shows one of its blocks to any honest node it likes.
Show(b, n) ==
  /\ b \in produced                           \* a block it signed
  /\ b \notin seen[n]                         \* not shown there yet
  /\ Receive(n, b)
  /\ UNCHANGED produced

\* An honest node passes a block it merged on to a peer, as replication does.
Relay(n, m, b) ==
  /\ m \in Peers[n]                           \* only along a link
  /\ b \in merged[n]                          \* only blocks it merged
  /\ b \notin seen[m]                         \* the peer lacks it
  /\ Receive(m, b)
  /\ UNCHANGED produced

\* Hearing an accusation of a backed by blocks P: a convinced node keeps the
\* blocks as its own evidence and jails a.
Hear(m, a, P) ==
  /\ a \notin jailed[m]                       \* not jailed there yet
  /\ Convinced(P, a)                          \* the accusation holds up
  /\ seen' = [seen EXCEPT ![m] = @ \cup P]
  /\ jailed' = [jailed EXCEPT ![m] = @ \cup {a}]
  /\ UNCHANGED <<produced, merged>>

\* An honest node that jailed a hands every block of a it holds to a peer.
Accuse(n, m, a) ==
  /\ GossipEvidence                           \* RED_NoGossip turns this off
  /\ m \in Peers[n]                           \* only along a link
  /\ a \in jailed[n]                          \* it jailed a itself
  /\ Hear(m, a, {b \in seen[n] : b.a = a})    \* the proof is what it holds

\* The cheater accuses anyone, attaching any signed blocks it likes.
Frame(m, a, P) ==
  /\ P \subseteq produced                     \* it can only attach real blocks
  /\ Hear(m, a, P)

Next ==
  \/ \E b \in Block : Sign(b)
  \/ \E b \in Block, n \in Honest : Show(b, n)
  \/ \E n, m \in Honest, b \in Block : Relay(n, m, b)
  \/ \E n, m \in Honest, a \in Nodes : Accuse(n, m, a)
  \/ \E m \in Honest, a \in Nodes, P \in SUBSET produced : Frame(m, a, P)

\* Honest links are reliable: a relay or an accusation that stays possible
\* happens. The cheater gets no fairness: it signs, shows and frames as it likes.
Fairness ==
  /\ \A n, m \in Honest, b \in Block : WF_vars(Relay(n, m, b))
  /\ \A n, m \in Honest, a \in Nodes : WF_vars(Accuse(n, m, a))

Spec == Init /\ [][Next]_vars /\ Fairness

\* No honest node is ever jailed by an honest node.
INV_NoWrongfulJail == \A n \in Honest : jailed[n] \subseteq {Cheater}

\* No honest node ever merges both sides of a fork.
INV_NoForkMerged == \A n \in Honest : \A a \in Nodes : ~Conflicting(merged[n], a)

\* Some honest node holds a fork between them all (the proof exists somewhere).
ForkVisible == Conflicting(UNION {seen[n] : n \in Honest}, Cheater)

\* Every honest node has jailed the cheater.
AllJailed == \A n \in Honest : Cheater \in jailed[n]

\* Once the proof exists among honest nodes, every honest node jails the cheater.
PROP_EveryoneJails == ForkVisible ~> AllJailed
====
