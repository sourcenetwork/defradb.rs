---- MODULE JsonCrdt ----
\* The story, in plain words.
\*
\* A JSON field holds a small tree of maps, lists and plain values, and many
\* peers edit it at once. Every edit is one block in the document's DAG. A
\* block knows every block its author had seen (its causal past) and says one
\* thing about one path in the tree: "set this path to a value", "set it to
\* an empty map {}", "set it to an empty list []", or "delete this path".
\*
\* Each path is its own register, so edits to different paths never touch.
\* When edits to one path race, the one with the higher priority (the DAG
\* height) wins, and the block ID (the CID) breaks a tie. The losers are not
\* thrown away: a conflicts query lists them.
\*
\* Edits at different depths meet in two ways:
\*   * An edit to a path wipes out the edits below it that it had seen. Setting
\*     "a" to 5 removes the old "a.b"; deleting "a" removes it too.
\*   * An edit below a path it had not seen survives. If Alice deletes "a"
\*     while Bob, at the same time, sets "a.b" to 1, Bob's edit stays, and
\*     "a" comes back as a map holding it. An update beats a concurrent delete.
\*     If Alice had set "a" to 5 instead, "a" can't be both 5 and a map:
\*     Alice's 5 and Bob's implied map race like any two edits to "a", and the
\*     loser takes its children with it.
\* Only a set below a path implies the path is a map. A delete never creates
\* anything: if Bob had deleted "a.b" instead of setting it, "a" stays gone.
\*
\* A frontier names each edit by its operation ID, which a writer computes
\* before its block (and its CID) exists. Two peers can make the very same
\* edit on the same heads at once; their blocks differ only in signature,
\* so their CIDs differ. Each write carries its own nonce, so the two edits
\* still take two IDs. If they shared one, a peer that merged both would
\* keep one entry for two blocks, and a delete that saw only one of them
\* would wipe the other on some peers and not on others.
\*
\* Example: "a" and "a.b" are empty. Alice deletes "a"; Bob sets "a.b" to 1,
\* neither having seen the other. Every peer ends with {"a": {"b": 1}}, and
\* the conflicts query at "a" lists Alice's delete.
\*
\* What must never happen:
\*   * two peers holding the same blocks show different JSON or conflicts;
\*   * a peer's JSON, built one block at a time, differs from reading its
\*     blocks from scratch;
\*   * an edit vanishes when nothing else touched its path or the paths
\*     above and below it;
\*   * edits racing on one path end anywhere but at the highest priority,
\*     then the highest CID;
\*   * a concurrent delete above an edit removes it;
\*   * a delete makes a path present with no set behind it;
\*   * a type clash resolves other than by that same order;
\*   * the conflicts query lists anything but the racing edits that lost.
\*
\* Code this models (defradb.rs):
\*   crates/defra-core/src/json_crdt.rs            the per-path registers: the
\*                                                 frontier of racing edits,
\*                                                 the winner, the conflicts
\*   crates/crdt/src/field/json.rs                 JsonField, the field's
\*                                                 TaggedCrdt: a write's ops,
\*                                                 a block's operand, replay
\*   crates/db/src/merge/merge_handler/field_tagged.rs
\*                                                 applies a received block
\*                                                 once its parents are merged
\*   crates/db/src/write/autocommit/helpers.rs     a local write's ops and
\*                                                 operand
\*                                                 (prepare_tagged_writes)
\*
\* Correspondence table (model -> Rust):
\*   Create        json.rs write_data (block_ops, deep_set; json_crdt.rs diff):
\*                 a write turns a JSON value into ops at paths;
\*                 prepare_tagged_writes gives its block the field's heads as
\*                 parents. A model block edits one path. A Rust block may
\*                 carry ops at several distinct paths; this model does not
\*                 check those, and covers single-path blocks only
\*   Deliver       field_tagged.rs merge_field_tagged: a received block is
\*                 merged once every parent block is local; b \notin known[n]
\*                 is the block's published priority entry (h/p/), which
\*                 outlives the frontier dropping its ID
\*   Apply         json_crdt.rs Frontier::delta, then Frontier::compose: drop
\*                 the edits in the block's past at its paths and below, add
\*                 its edits (json.rs effect); tagged_past finds that past by
\*                 walking its DAG
\*   front         json_crdt.rs Frontier::entries, stored as the field's
\*                 part; it holds operation IDs
\*   OpId          json_crdt.rs block_id over the data json.rs write_data
\*                 writes (encode_write: the ops and a nonce drawn by
\*                 prepare_tagged_writes); SharedIds exists only for the RED
\*                 configuration whose writes carry no nonce
\*   Dup           field_tagged.rs merge_field_tagged: a block whose ID the
\*                 stored state already records changes nothing
\*   win, Winner   json_crdt.rs Index::winner
\*   Beats         json_crdt.rs beats: priority, then block ID. The ID
\*                 (block_id) is a digest of the block's priority, parents and
\*                 plain ops: it stands in for the CID, which a local writer
\*                 does not know yet when it writes the stored value
\*   Implied       json_crdt.rs Index::racers: the sets below a path
\*   Doc           json_crdt.rs Frontier::materialize
\*   Conflicts     json_crdt.rs Frontier::conflicts, read by _jsonConflicts
\*                 (crates/db/src/read/json_conflicts.rs); an edit's `seen`
\*                 records the edits above it in its past, which is how the
\*                 code tells two edits at different depths are concurrent
\*   SpecDoc       json_crdt.rs replay (json.rs TaggedCrdt::replay): the JSON
\*                 read straight from the whole DAG, used by time-travel reads
\*                 (crates/db/src/read/versioned.rs)

EXTENDS Naturals, FiniteSets, TLC

CONSTANTS
  Peers,       \* the peers editing one JSON field
  MaxBlocks,   \* how many edit blocks may be written in all, so the model stays finite
  UseCid,           \* TRUE: a priority tie is broken by the CID (the design)
  DeleteWins,       \* TRUE: a delete beats a concurrent edit below it (a RED)
  DeleteImpliesMap, \* TRUE: a delete below a path says the path is a map (a RED)
  SharedIds         \* TRUE: equal edits on the same past share one ID (a RED)

VARIABLES
  blocks,   \* the blocks written so far; the index is the block ID
  nblocks,  \* how many blocks have been written
  known,    \* for each peer, the IDs of the blocks it has merged
  front,    \* for each peer and path, the frontier of edits to that path
  win       \* for each peer and path, the winner among that path's own edits, or 0

\* Every variable, for "nothing changed".
vars == <<blocks, nblocks, known, front, win>>

\* The paths of a small JSON tree: "a", "a.b" under it, and "c" beside them.
Paths == {"a", "ab", "c"}

\* The path one level up; the root is the document's own map.
Parent(p) == IF p = "ab" THEN "a" ELSE "root"

\* Whether path p is strictly above path q.
Above(p, q) == p = "a" /\ q = "ab"

\* The paths strictly below p.
Below(p) == {q \in Paths : Above(p, q)}

\* Whether p and q are one path, or one sits above the other.
Related(p, q) == p = q \/ Above(p, q) \/ Above(q, p)

\* What an edit does: set a plain value, set {}, set [], or delete.
Kinds == {"value", "map", "list", "delete"}

\* The IDs blocks may take. A real CID is a hash; here it is a number handed
\* out in the order blocks are written. The model tries every order of writes,
\* so two concurrent blocks get their IDs both ways round.
Ids == 1..MaxBlocks

\* What a path reads as. A plain value is shown as the ID of the block that
\* set it, so two different values never look alike.
Absent == 0     \* the path is not there
MapVal == 100   \* the path holds a map
ListVal == 101  \* the path holds a list

\* A slot for a block not written yet.
NoBlock == [author |-> "none", path |-> "none", kind |-> "none", past |-> {}, prio |-> 0]

\* Shorthands for a block's fields, by ID.
PathOf(b) == blocks[b].path   \* the path it edits
KindOf(b) == blocks[b].kind   \* what it does there
Past(b) == blocks[b].past     \* the blocks its author had seen
Prio(b) == blocks[b].prio     \* its priority: one more than the highest it saw

\* The operation ID of block b. In the design every update draws a nonce, and
\* a create is the root of its own document (a DocID is the CID of its first
\* block, signature included), so every block has its own ID. In the RED, a block takes the ID of the first
\* block that made the same edit on the same past at the same priority,
\* whoever wrote it.
OpId(b) ==
  IF ~SharedIds THEN b
  ELSE CHOOSE x \in 1..b :
         /\ PathOf(x) = PathOf(b) /\ KindOf(x) = KindOf(b)       \* the same edit
         /\ Past(x) = Past(b) /\ Prio(x) = Prio(b)               \* on the same past
         /\ \A y \in 1..(x - 1) :                                \* and the first such
              ~(PathOf(y) = PathOf(b) /\ KindOf(y) = KindOf(b) /\ Past(y) = Past(b) /\ Prio(y) = Prio(b))

\* Two blocks race when neither author had seen the other's block.
Concurrent(x, y) == x # y /\ x \notin Past(y) /\ y \notin Past(x)

\* Block x beats block y: higher priority, or the same priority and a higher
\* CID. With UseCid off (the RED) a tie beats nothing.
Beats(x, y) ==
  \/ Prio(x) > Prio(y)                              \* higher priority wins
  \/ UseCid /\ Prio(x) = Prio(y) /\ x > y           \* a tie goes to the higher CID

\* The block in s that nothing else in s beats, or 0 for an empty s. With
\* the CID tiebreak there is exactly one; without it, ties pick one.
Best(s) == IF s = {} THEN 0 ELSE CHOOSE x \in s : \A y \in s : ~Beats(y, x)

\* The highest number in a set of numbers, 0 for none.
Max(s) == IF s = {} THEN 0 ELSE CHOOSE m \in s : \A n \in s : n <= m

\* What block b says about path p: its own edit if it edits p, and "p is a
\* map" if it sets a path below p.
KindAt(b, p) == IF PathOf(b) = p THEN KindOf(b) ELSE "map"

\* The edits in frontiers f below path p that say "p is a map": the sets. A
\* delete below says nothing about p, so it can't bring p back. In the RED,
\* deletes below count too.
Implied(f, p) ==
  {b \in UNION {f[q] : q \in Below(p)} :                 \* every edit below p
     DeleteImpliesMap \/ KindOf(b) # "delete"}           \* that sets something

\* What a winning block makes path p read as.
ValueOf(b, p) ==
  CASE KindAt(b, p) = "value"  -> OpId(b)   \* the plain value it set
    [] KindAt(b, p) = "map"    -> MapVal    \* a map
    [] KindAt(b, p) = "list"   -> ListVal   \* a list
    [] KindAt(b, p) = "delete" -> Absent    \* gone

\* The winner at path p, read from frontiers f and own-edit winners w. The
\* sets below p each say "p is a map". When any exist, a delete of p can't
\* win (an update beats a concurrent delete), so the race is between p's
\* other edits and those implied maps. With no sets below, p's own winner
\* stands. In the RED, deletes stay in the race.
Winner(f, w, p) ==
  LET below == Implied(f, p)                                  \* sets below p
      own == IF DeleteWins THEN f[p]                          \* RED: deletes race too
             ELSE {b \in f[p] : KindOf(b) # "delete"}         \* design: deletes sit out
  IN IF below = {} THEN w[p]                                  \* no sets below: own winner
     ELSE Best(own \cup below)                                \* else everything races

\* What path p reads as on its own, before looking above it.
NodeVal(f, w, p) ==
  LET b == Winner(f, w, p)                     \* the winning edit
  IN IF b = 0 THEN Absent ELSE ValueOf(b, p)   \* none: absent; else its value

\* The JSON: each path's value, absent when the path above isn't a map.
DocOf(f, w) ==
  [p \in Paths |->
     IF Parent(p) # "root" /\ NodeVal(f, w, Parent(p)) # MapVal
       THEN Absent                             \* the parent isn't a map: no child
       ELSE NodeVal(f, w, p)]                  \* otherwise its own value

\* The conflicts at path p: every edit in the race that lost to the winner
\* without having been seen by it, except implied maps losing to a map (they
\* agree, so nothing was dropped). The race is p's own edits and the sets
\* below it; a delete below was never in it.
ConflictsOf(f, w, p) ==
  LET b == Winner(f, w, p)                                   \* the winner
      racers == f[p] \cup Implied(f, p)                      \* p's edits and the sets below
  IN IF b = 0 THEN {}                                        \* no winner, no conflict
     ELSE {c \in racers :
             /\ Concurrent(c, b)                             \* it raced the winner
             /\ ~(KindAt(c, p) = "map" /\ KindAt(b, p) = "map")} \* and said something else

\* The conflicts query at every path.
AllConflicts(f, w) == [p \in Paths |-> ConflictsOf(f, w, p)]

\* A peer's JSON and conflicts, from its incremental state.
Doc(n) == DocOf(front[n], win[n])
Conf(n) == AllConflicts(front[n], win[n])

\* ---- The same, read straight from a set of blocks k ----

\* Block b was seen by a later edit of its own path: superseded.
Superseded(k, b) == \E y \in k : PathOf(y) = PathOf(b) /\ b \in Past(y)

\* Block b was seen by an edit of a path above it: wiped out.
Wiped(k, b) == \E x \in k : Above(PathOf(x), PathOf(b)) /\ b \in Past(x)

\* The frontier of path p in k: its edits nothing later has seen.
SpecFront(k) == [p \in Paths |->
                   {b \in k : PathOf(b) = p /\ ~Superseded(k, b) /\ ~Wiped(k, b)}]

\* Each path's own winner in k.
SpecWin(k) == [p \in Paths |-> Best(SpecFront(k)[p])]

\* The JSON and conflicts that the blocks k say, read from scratch.
SpecDoc(k) == DocOf(SpecFront(k), SpecWin(k))
SpecConf(k) == AllConflicts(SpecFront(k), SpecWin(k))

\* ---- Behaviour ----

\* Swapping the peers' names changes nothing: no rule names a peer, and every
\* invariant speaks of all of them alike. So TLC checks one state of each pair
\* that differ only by which peer is which (SYMMETRY in the GREEN config).
PeerSymmetry == Permutations(Peers)

\* The GREEN check is split in two by what the first block written does, so
\* each half finishes in time. A block never changes once written, so every
\* state lies in exactly one half (or in both, before any block exists).
FirstSetsValueOrMap == nblocks = 0 \/ KindOf(1) \in {"value", "map"}
FirstSetsListOrDeletes == nblocks = 0 \/ KindOf(1) \in {"list", "delete"}

\* The kinds of value each variable may hold.
TypeOK ==
  /\ nblocks \in 0..MaxBlocks                                 \* within the allowance
  /\ \A n \in Peers : known[n] \subseteq 1..nblocks           \* only written blocks
  /\ \A n \in Peers : \A p \in Paths :
       front[n][p] \subseteq {OpId(x) : x \in known[n]}               \* frontiers hold merged edits' IDs
  /\ \A n \in Peers : \A p \in Paths : win[n][p] \in {0} \cup front[n][p] \* a winner is on the frontier

\* At the start nothing is written and nobody holds anything.
Init ==
  /\ blocks = [i \in Ids |-> NoBlock]                    \* every slot empty
  /\ nblocks = 0                                         \* no block yet
  /\ known = [n \in Peers |-> {}]                        \* nobody merged anything
  /\ front = [n \in Peers |-> [p \in Paths |-> {}]]      \* every frontier empty
  /\ win = [n \in Peers |-> [p \in Paths |-> 0]]         \* no winner anywhere

\* Peer n merges block b: its edit, by ID, joins its path's frontier, and
\* the edits it had seen leave that frontier and every frontier below. A path
\* keeps its winner unless the new block beats it or the winner just left;
\* then the best of what remains takes over. A block whose ID the frontier
\* already holds changes nothing.
Apply(n, b) ==
  LET p == PathOf(b)                                          \* the path b edits
      i == OpId(b)                                            \* the ID its edit goes by
      seen == {OpId(x) : x \in Past(b)}                       \* the IDs of what it saw
      dup == \E q \in Paths : i \in front[n][q]               \* an edit by that ID is held
      f == [q \in Paths |->
              IF dup THEN front[n][q]                         \* a held edit: nothing changes
              ELSE IF q = p THEN (front[n][q] \ seen) \cup {i} \* b joins, what it saw leaves
              ELSE IF Above(p, q) THEN front[n][q] \ seen      \* below: what it saw is wiped
              ELSE front[n][q]]                               \* elsewhere: untouched
      old == win[n]                                           \* the winners before
      w == [q \in Paths |->
              IF old[q] \notin f[q] THEN Best(f[q])           \* winner gone: best remaining
              ELSE IF ~dup /\ q = p /\ Beats(i, old[q]) THEN i \* b beats it: b
              ELSE old[q]]                                    \* otherwise it stays
  IN /\ known' = [known EXCEPT ![n] = @ \cup {b}]             \* n now holds b
     /\ front' = [front EXCEPT ![n] = f]                      \* its new frontiers
     /\ win' = [win EXCEPT ![n] = w]                          \* its new winners

\* Peer n writes a new block: an edit of kind k at path p, having seen every
\* block it holds. It merges the block itself through Deliver, and writes
\* nothing else until it has, as a real author merges its own edit at once.
Create(n, p, k) ==
  LET b == nblocks + 1                                        \* the next ID
  IN /\ nblocks < MaxBlocks                                   \* within the allowance
     /\ \A x \in 1..nblocks : blocks[x].author = n => x \in known[n] \* its own blocks are merged
     /\ blocks' = [blocks EXCEPT ![b] =
                    [author |-> n, path |-> p, kind |-> k, past |-> known[n],
                     prio |-> 1 + Max({Prio(x) : x \in known[n]})]] \* one above anything it saw
     /\ nblocks' = b                                          \* one more block
     /\ UNCHANGED <<known, front, win>>

\* Peer n receives block b, once it holds every block b's author had seen.
Deliver(n, b) ==
  /\ b \in 1..nblocks              \* b has been written
  /\ b \notin known[n]             \* n doesn't hold it yet
  /\ Past(b) \subseteq known[n]    \* the causal buffer: its past is merged
  /\ Apply(n, b)                   \* merge it
  /\ UNCHANGED <<blocks, nblocks>>

\* Every block is written and every peer holds them all.
Done == nblocks = MaxBlocks /\ \A n \in Peers : known[n] = 1..nblocks

\* One step: a peer writes a block or receives one. Once everything is
\* written and delivered nothing more happens, which is not a deadlock.
Next ==
  \/ \E n \in Peers, p \in Paths, k \in Kinds : Create(n, p, k)  \* someone edits
  \/ \E n \in Peers, b \in Ids : Deliver(n, b)                    \* someone receives
  \/ Done /\ UNCHANGED vars                                       \* or all is settled

\* The whole behaviour: start, then any sequence of steps.
Spec == Init /\ [][Next]_vars

\* A state constraint the shared-ID RED checks under, so it finishes in time:
\* only the edits its bug needs, sets of "a.b" and deletes of "a".
SharedIdEdits ==
  \A b \in 1..nblocks :
    \/ PathOf(b) = "ab" /\ KindOf(b) = "value"   \* a set of "a.b"
    \/ PathOf(b) = "a" /\ KindOf(b) = "delete"   \* or a delete of "a"

\* ---- What must hold ----

\* The value block b sets at its own path.
OwnVal(b) == ValueOf(b, PathOf(b))

\* The blocks of k on path p's line: p itself and the paths above and below.
Line(k, p) == {b \in k : Related(PathOf(b), p)}

\* Two peers holding the same blocks show the same JSON and the same conflicts.
INV_Converge ==
  \A n, m \in Peers : known[n] = known[m] => (Doc(n) = Doc(m) /\ Conf(n) = Conf(m))

\* A peer's JSON and conflicts, built one block at a time, are what reading
\* its blocks from scratch gives.
INV_MatchesReplay ==
  \A n \in Peers : Doc(n) = SpecDoc(known[n]) /\ Conf(n) = SpecConf(known[n])

\* An edit that is alone on its line shows, whatever happens on other paths.
INV_DisjointKept ==
  \A n \in Peers : \A b \in known[n] :
    Line(known[n], PathOf(b)) = {b} => Doc(n)[PathOf(b)] = OwnVal(b)

\* When only one path's own edits are on its line, it reads as the edit with
\* the highest priority, then the highest CID.
INV_SamePathLww ==
  \A n \in Peers : \A p \in Paths :
    LET own == {b \in known[n] : PathOf(b) = p}      \* p's own edits
        top == CHOOSE x \in own : \A y \in own : x = y \/ Prio(x) > Prio(y) \/ (Prio(x) = Prio(y) /\ x > y)
    IN (own # {} /\ Line(known[n], p) = own) => Doc(n)[p] = OwnVal(top)

\* A delete racing an edit below it, alone on their line, leaves the edit in
\* place and the deleted path back as a map holding it.
INV_UpdateWins ==
  \A n \in Peers : \A x, b \in known[n] :
    (/\ KindOf(x) = "delete"                      \* x deletes a path
     /\ Above(PathOf(x), PathOf(b))               \* above b's path
     /\ KindOf(b) # "delete"                      \* b is an update
     /\ Concurrent(x, b)                          \* neither saw the other
     /\ Line(known[n], PathOf(b)) = {x, b})       \* nothing else on the line
    => (Doc(n)[PathOf(b)] = OwnVal(b) /\ Doc(n)[PathOf(x)] = MapVal)

\* A plain value or a list set on a path, racing an edit below it, alone on
\* their line: the higher priority, then CID, wins, and the loser's side goes.
INV_TypeClash ==
  \A n \in Peers : \A x, b \in known[n] :
    (/\ KindOf(x) \in {"value", "list"}           \* x makes the path not a map
     /\ Above(PathOf(x), PathOf(b))               \* above b's path
     /\ KindOf(b) # "delete"                      \* b needs it to be a map
     /\ Concurrent(x, b)                          \* neither saw the other
     /\ Line(known[n], PathOf(b)) = {x, b})       \* nothing else on the line
    => IF Prio(x) > Prio(b) \/ (Prio(x) = Prio(b) /\ x > b)
         THEN Doc(n)[PathOf(x)] = OwnVal(x) /\ Doc(n)[PathOf(b)] = Absent  \* x wins
         ELSE Doc(n)[PathOf(x)] = MapVal /\ Doc(n)[PathOf(b)] = OwnVal(b)  \* b wins

\* The conflicts query never lists a path's winner or an edit the winner had
\* seen, and lists every edit of that path that raced the winner and said
\* something different.
INV_ConflictsAreLosers ==
  \A n \in Peers : \A p \in Paths :
    LET b == Winner(front[n], win[n], p) IN
    b # 0 =>                                                      \* a path with a winner
    /\ b \notin Conf(n)[p]                                        \* never the winner
    /\ \A c \in Conf(n)[p] : Concurrent(c, b)                     \* only racers
    /\ \A c \in front[n][p] :                                     \* every racing own edit
         (Concurrent(c, b) /\ ~(KindAt(c, p) = "map" /\ KindAt(b, p) = "map"))
           => c \in Conf(n)[p]

\* A delete never creates anything: take away a delete that no other held
\* block had seen, and any path it turns from absent to present is held up by
\* a set the delete hadn't seen, at that path or below it. (A delete can
\* uncover such a set: deleting a list at "a" uncovers a racing set of "a.b"
\* that the list had beaten. It can never conjure a path with no set behind it.)
INV_DeleteCreatesNothing ==
  \A n \in Peers : \A d \in known[n] :
    (/\ KindOf(d) = "delete"                          \* d is a delete
     /\ \A y \in known[n] : d \notin Past(y))         \* nothing held came after it
    => \A p \in Paths :
         (SpecDoc(known[n] \ {d})[p] = Absent /\ Doc(n)[p] # Absent) \* d made p present
         => \E s \in known[n] :                       \* then some edit s
              /\ KindOf(s) # "delete"                 \* that sets something
              /\ (PathOf(s) = p \/ Above(p, PathOf(s))) \* at p or below it
              /\ s \notin Past(d)                     \* and that d hadn't seen
====
