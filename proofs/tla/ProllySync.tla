------------------------------ MODULE ProllySync ------------------------------
\* The story, in plain words.
\*
\* Who acts: three nodes that share one plain (non-branchable) collection with
\* two documents. Each node keeps, per document, the versions it has seen; a
\* document's heads are a function of that set, and merging what a peer has is
\* adding its versions to ours. Not every node may read every document.
\*
\* What can happen: a node writes a new version of a document it may read. A
\* node pulls from a peer to catch up: the peer (the responder) builds a prolly
\* tree of its documents, keeping only the ones the puller may read, and sends
\* its root. If that root is the puller's own root, the two hold the same state
\* and the pull ends. Otherwise the puller walks down to the documents whose
\* entries differ, learns the responder's entries for them, and fetches every
\* one whose versions it does not already hold all of. The writes and pulls
\* interleave in any order.
\*
\* What must never happen: a node learns a document it may not read (NoLeak),
\* or a pull ends as "caught up" while the responder still has versions of a
\* document the puller may read that the puller lacks (PulledHolds).
\*
\* What this model trusts. A root is the tree's entries (`Root(v) == v`): the
\* Lean proof `ProllyTree.build_same_entries` shows a root depends only on the
\* entries, not on the writes' order, so equal entries give equal roots, and a
\* chunk hash taken as collision-free makes different entries give different
\* roots. The walk down finds exactly the documents whose entries differ: the
\* Lean proofs `ProllyTree.diff_eq` and `ProllyTree.mergeDiff_spec` show it.
\*
\* What this model leaves out. A pull is one atomic step: the responder cannot
\* write between sending its root and the puller finishing, so "caught up"
\* means caught up with the responder as it was at that step. Who may read what
\* (CanRead) never changes, so a read revoked between the root reply and a
\* leaf is not modeled. When the pull is wired (#2977) and either can happen,
\* the step is split and those cases are checked first.
\*
\* Rust anchors:
\* * `crates/foundation/prolly/src/tree.rs` `Tree::build` and `Tree::root`: the responder
\*   builds the tree of what the puller may read on the fly, so the root it
\*   sends is never older than its entries (FreshRoot), and holds only those
\*   documents (Filter). Modeled by `Pull`'s `Shown` and `SentRoot`.
\* * `crates/foundation/prolly/src/diff.rs` `diff`: the documents whose entries differ.
\*   Modeled by `Pull`'s `differ`.
\* * The pull itself is not wired into `crates/p2p` yet (#2977): this model is
\*   its contract, written before that code, with the fetch rule (FetchChanged)
\*   it must keep.
EXTENDS Naturals, FiniteSets

\* MaxVersion: how many writes happen in total, so the model is finite.
\* Filter: the responder keeps only documents the puller may read (TRUE in GREEN).
\* FreshRoot: the responder builds its root when asked, not from a cache (TRUE in GREEN).
\* FetchChanged: the puller fetches a document whose versions differ, not only
\* a document it lacks altogether (TRUE in GREEN).
\* InvalidateOnMerge: a pull that changes the puller ends every "caught up"
\* claim made against it (TRUE in GREEN).
CONSTANTS MaxVersion, Filter, FreshRoot, FetchChanged, InvalidateOnMerge

\* The three nodes, by number.
Nodes == 1..3
\* The two documents, by number.
Docs == 1..2
\* Who may read what: node 1 and node 3 read both documents, node 2 reads only
\* document 1. So node 2 must never learn document 2.
CanRead == {<<1, 1>>, <<1, 2>>, <<2, 1>>, <<3, 1>>, <<3, 2>>}
\* The version numbers a write can create.
Versions == 1..MaxVersion
\* A view: for each document, the set of versions in it (empty: not there).
Empty == [d \in Docs |-> {}]

VARIABLES
  \* held[n][d]: the versions of document d that node n holds.
  held,
  \* seen[n]: the documents whose entries node n has been sent by a peer.
  seen,
  \* cached[b][a]: the root responder b built for puller a the last time it
  \* refreshed; used only when FreshRoot is FALSE.
  cached,
  \* pulled[a][b]: a's last pull from b ended "caught up", and b has not
  \* changed since. A ghost variable: it only states what the pull claimed.
  pulled,
  \* written: how many writes have happened, so each version is new.
  written

\* Everything the model tracks, in one tuple.
vars == <<held, seen, cached, pulled, written>>

\* The entries responder b shows puller a: every document when nothing is
\* filtered, otherwise only those a may read.
Shown(b, a) == [d \in Docs |-> IF ~Filter \/ <<a, d>> \in CanRead THEN held[b][d] ELSE {}]

\* What a is owed by b: b's versions of every document a may read.
Owed(b, a) == [d \in Docs |-> IF <<a, d>> \in CanRead THEN held[b][d] ELSE {}]

\* The root of a tree is its entries (history independence, see above).
Root(v) == v

\* The tree b answers a with: built now from what it shows, or the cached one.
\* The root comparison, the walk and the fetch all read this one tree.
Sent(b, a) == IF FreshRoot THEN Shown(b, a) ELSE cached[b][a]

\* The root b sends to a: the root of the tree it answers with.
SentRoot(b, a) == Root(Sent(b, a))

\* Nothing held, seen, cached or pulled yet; no writes.
Init ==
  \* No node holds any version.
  /\ held = [n \in Nodes |-> Empty]
  \* No node has been sent any entry.
  /\ seen = [n \in Nodes |-> {}]
  \* Every cached root is the empty tree's.
  /\ cached = [b \in Nodes |-> [a \in Nodes |-> Empty]]
  \* No pull has ended yet.
  /\ pulled = [a \in Nodes |-> [b \in Nodes |-> FALSE]]
  \* No write has happened.
  /\ written = 0

\* Node n writes a new version of document d, which it may read. Every pull
\* from n that ended "caught up" is now out of date.
Write(n, d) ==
  \* Only a reader writes, and only while writes remain.
  /\ <<n, d>> \in CanRead
  /\ written < MaxVersion
  \* The new version is the next number, added to what n holds of d.
  /\ written' = written + 1
  /\ held' = [held EXCEPT ![n][d] = @ \cup {written + 1}]
  \* Nobody is caught up with n any more.
  /\ pulled' = [a \in Nodes |-> [b \in Nodes |-> IF b = n THEN FALSE ELSE pulled[a][b]]]
  \* Nothing is sent and no root is cached.
  /\ UNCHANGED <<seen, cached>>

\* Responder b caches the root it would build for a now. Only a node that
\* caches roots does this, so the GREEN runs never take this step.
Refresh(b, a) ==
  \* Only when roots come from a cache.
  /\ ~FreshRoot
  /\ cached' = [cached EXCEPT ![b][a] = Root(Shown(b, a))]
  \* Nothing else changes.
  /\ UNCHANGED <<held, seen, pulled, written>>

\* Node a pulls from node b.
Pull(a, b) ==
  \* Two different nodes.
  /\ a # b
  /\ IF SentRoot(b, a) = Root(held[a])
       \* Equal roots: a takes the two as equal and the pull ends at once.
       THEN /\ pulled' = [pulled EXCEPT ![a][b] = TRUE]
            /\ UNCHANGED <<held, seen, cached, written>>
       \* Different roots: walk down to the documents whose entries differ.
       \* The walk reads the tree b sent, not b's current state.
       ELSE LET differ == {d \in Docs : Sent(b, a)[d] # held[a][d]}
                \* The documents a fetches: those where b has a version a
                \* lacks, or, with the bug, only those a lacks entirely.
                fetch == IF FetchChanged
                           THEN {d \in differ : ~(Sent(b, a)[d] \subseteq held[a][d])}
                           ELSE {d \in differ : held[a][d] = {}}
                \* What a holds after merging what it fetched.
                after == [d \in Docs |-> IF d \in fetch THEN held[a][d] \cup Sent(b, a)[d]
                                                         ELSE held[a][d]]
            IN /\ held' = [held EXCEPT ![a] = after]
               \* a now knows of every differing document b has an entry for.
               /\ seen' = [seen EXCEPT ![a] = @ \cup {d \in differ : Sent(b, a)[d] # {}}]
               \* The pull ends "caught up" with b; if a changed, nobody is
               \* caught up with a any more (unless the bug keeps old claims).
               /\ pulled' = [x \in Nodes |-> [y \in Nodes |->
                               IF x = a /\ y = b THEN TRUE
                               ELSE IF InvalidateOnMerge /\ y = a /\ after # held[a] THEN FALSE
                               ELSE pulled[x][y]]]
               /\ UNCHANGED <<cached, written>>

\* Every step is a write, a cache refresh or a pull.
Next ==
  \/ \E n \in Nodes, d \in Docs : Write(n, d)
  \/ \E b, a \in Nodes : Refresh(b, a)
  \/ \E a, b \in Nodes : Pull(a, b)

\* The whole behaviour: start empty, then take steps.
Spec == Init /\ [][Next]_vars

\* Every variable holds the kind of value it should.
TypeOK ==
  \* Held versions are version numbers.
  /\ held \in [Nodes -> [Docs -> SUBSET Versions]]
  \* Seen documents are documents.
  /\ seen \in [Nodes -> SUBSET Docs]
  \* Pull claims are true or false.
  /\ pulled \in [Nodes -> [Nodes -> BOOLEAN]]
  \* The write count stays in bounds.
  /\ written \in 0..MaxVersion

\* A node never learns of, or holds, a document it may not read.
NoLeak ==
  \A n \in Nodes, d \in Docs :
    \* Sent an entry for it, or holding a version of it, means it may read it.
    (d \in seen[n] \/ held[n][d] # {}) => <<n, d>> \in CanRead

\* A pull that ended "caught up" is right: the puller holds every version the
\* responder has of every document the puller may read.
PulledHolds ==
  \A a, b \in Nodes :
    \* Each document's versions owed by b are all held by a.
    pulled[a][b] => \A d \in Docs : Owed(b, a)[d] \subseteq held[a][d]

=============================================================================
