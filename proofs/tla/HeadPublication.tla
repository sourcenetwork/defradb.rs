---- MODULE HeadPublication ----
\* THE STORY.
\* A document's heads live in one head set: one stored value that writers only
\* ever change by merging an operand into it (a blind merge). Replication
\* publishes a block into that set: the block becomes a head, and each parent
\* it names stops being one. A parent that is a head right now is superseded.
\* A parent that is not a head is either a block that was published and has
\* since been superseded, or a block that has not arrived yet; the second is
\* held "ahead", so its own arrival later is swallowed instead of making it a
\* head under its own child.
\*
\* Who acts: transactions, each publishing one block. The same block can be
\* published twice (a replay: gossip delivers it again, or a restart forgets it
\* was merged).
\*
\* What must never happen:
\*   - a block that has a published child reads as a head (a resurrection);
\*   - a published block stays held ahead (a stale name nobody will ever
\*     consume, and a set that no longer says exactly what the DAG says);
\*   - a publication is turned away because of a write that has nothing to do
\*     with the decisions it made.
\*
\* WHAT THE CODE DOES, AND WHERE (the correspondence).
\*   Snapshot(t), the decisions          crates/database/db/src/block/heads/document.rs `publish`
\*     the block's publication record    its priority index entry `h/p/{doc}/{priority}/{cid}`
\*                                       with the published value; read with `get`, which
\*                                       regolith validates at commit (an Ordinary key)
\*     a parent that is a head           `Set::is_live` on the set the caller read
\*     a parent's record                 `was_published`: the parent's stored block gives its
\*                                       priority, then its priority index entry; not
\*                                       validated, the part read below covers it
\*     the part read                     `get_parts(set key, parts)` naming only the parents
\*                                       held ahead (`head_set::part` per name)
\*   Commit(t), the refusal              regolith's commit check: a `get` is refused by any
\*                                       newer write of its key; a read by parts only by an
\*                                       operand `MergeOperator::touches` says touches a part
\*                                       (crates/engine/storage/src/backends/regolith/merge/head_set.rs
\*                                       `touches`: an operand touches the names it holds)
\*   Commit(t), the writes               `Write::{supersede, supersede_ahead, add}` merged as
\*                                       one operand, and the priority index entry set
\*   Add / Supersede / SupersedeAhead    the per-name table in head_set.rs
\*
\* There is no separate guard key: the record is the priority index entry every
\* publication writes anyway, and the race on a parent goes through the head
\* set's own key.
\*
\* KNOBS. Each RED configuration removes one guard and must fail on the
\* invariant that guard keeps.
\*   Record   = "Durable"     the publication record is read and written. [GREEN]
\*            = "None"        nothing records a publication: a replay of a
\*                            superseded block makes it a head again.
\*                            INV_LiveExact fails.                     [RED]
\*   OwnRead  = "Validated"   the record read is refused at commit by a newer
\*                            publication of the same block.           [GREEN]
\*            = "Unvalidated" two publications of one block both add it, and a
\*                            child that superseded it in between is undone.
\*                            INV_LiveExact fails.                     [RED]
\*   PartRead = "Names"       a parent held ahead is read by its part, which only
\*                            an operand naming that parent refuses.   [GREEN]
\*            = "Off"         no read: the parent arrives and is superseded by a
\*                            second child, then the first child's hold lands
\*                            on it. INV_AheadExact fails.             [RED]
\*            = "WholeSet"    the read is refused by any operand on the set, the
\*                            behaviour of an operator whose `touches` says yes
\*                            to every part. An unrelated block's publication
\*                            turns the reader away. INV_NoUnrelatedAbort
\*                            fails.                                   [RED]

\* Naturals for the commit clock, FiniteSets for set sizes, TLC for the checker.
EXTENDS Naturals, FiniteSets, TLC

CONSTANTS
  Record,   \* "Durable" | "None": whether a publication leaves a record
  OwnRead,  \* "Validated" | "Unvalidated": whether the record read is checked at commit
  PartRead  \* "Names" | "Off" | "WholeSet": how a parent held ahead is read

\* Every knob takes one of its named values.
ASSUME Record \in {"Durable", "None"}
\* The record read is either checked at commit or not.
ASSUME OwnRead \in {"Validated", "Unvalidated"}
\* The parent read is by names, absent, or of the whole set.
ASSUME PartRead \in {"Names", "Off", "WholeSet"}

\* The blocks: a parent p, two children c and d that both name p (a fork), and
\* an unrelated block e with no parents.
Blocks == {"p", "c", "d", "e"}

\* The parents each block names.
Parents == [b \in Blocks |-> IF b \in {"c", "d"} THEN {"p"} ELSE {}]

\* The transactions: p is published twice (p2 is the replay), every other
\* block once.
Txns == {"p1", "p2", "c", "d", "e"}

\* The block a transaction publishes.
BlockOf(t) == IF t \in {"p1", "p2"} THEN "p" ELSE t

\* The three states a name can have in the head set.
States == {"absent", "ahead", "live"}

\* What an `add` does to a name: absent becomes a head, a name held ahead is
\* consumed (its child already superseded it), a head stays a head.
Add(s) == CASE s = "absent" -> "live"
            [] s = "ahead"  -> "absent"
            [] s = "live"   -> "live"

\* What a `supersede` does: a head stops being one; anything else is unchanged.
Supersede(s) == IF s = "live" THEN "absent" ELSE s

\* What a `supersede_ahead` does: a head stops being one, and a name that was
\* never seen is held ahead.
SupersedeAhead(s) == CASE s = "absent" -> "ahead"
                       [] s = "ahead"  -> "ahead"
                       [] s = "live"   -> "absent"

VARIABLES
  state,     \* [Blocks -> States]: the head set as it is stored
  record,    \* [Blocks -> BOOLEAN]: the block's publication record exists
  pub,       \* SUBSET Blocks: the blocks whose publication committed (the truth)
  clock,     \* Nat: counts the commits that wrote something
  opAt,      \* [Blocks -> Nat]: the clock of the newest operand naming the block
  anyOpAt,   \* Nat: the clock of the newest operand on the set at all
  recAt,     \* [Blocks -> Nat]: the clock of the newest write of the block's record
  phase,     \* [Txns -> {"idle", "open", "done"}]: where each transaction is
  snap,      \* [Txns -> Nat]: the clock when the transaction took its snapshot
  skip,      \* [Txns -> BOOLEAN]: it found its block already published
  sup,       \* [Txns -> SUBSET Blocks]: the parents it supersedes
  hold,      \* [Txns -> SUBSET Blocks]: the parents it holds ahead
  parts,     \* [Txns -> SUBSET Blocks]: the parents it read by part
  unrelated  \* BOOLEAN: some refusal had no write behind it that named what it read

\* Every variable, for stuttering steps.
vars == <<state, record, pub, clock, opAt, anyOpAt, recAt, phase, snap, skip,
          sup, hold, parts, unrelated>>

----------------------------------------------------------------------------
\* Nothing is published, nothing is open.
Init ==
  \* No name is in the set.
  /\ state = [b \in Blocks |-> "absent"]
  \* No record exists.
  /\ record = [b \in Blocks |-> FALSE]
  \* No block is published.
  /\ pub = {}
  \* No commit happened yet.
  /\ clock = 0
  \* No operand was written for any name.
  /\ opAt = [b \in Blocks |-> 0]
  \* No operand was written at all.
  /\ anyOpAt = 0
  \* No record was written.
  /\ recAt = [b \in Blocks |-> 0]
  \* Every transaction waits to start.
  /\ phase = [t \in Txns |-> "idle"]
  \* Snapshots are taken on start.
  /\ snap = [t \in Txns |-> 0]
  \* Decisions are made on start.
  /\ skip = [t \in Txns |-> FALSE]
  \* No parent superseded yet.
  /\ sup = [t \in Txns |-> {}]
  \* No parent held yet.
  /\ hold = [t \in Txns |-> {}]
  \* No part read yet.
  /\ parts = [t \in Txns |-> {}]
  \* No refusal yet.
  /\ unrelated = FALSE

\* Whether the parent q is known published: only with records, and only
\* through the record (the parent's own block and its priority index entry).
KnownPublished(q) == Record = "Durable" /\ record[q]

\* A transaction starts: it reads the set and the records at this moment and
\* makes every decision `publish` makes.
Start(t) ==
  \* Only a transaction that is not running yet starts.
  /\ phase[t] = "idle"
  /\ LET b == BlockOf(t)
         \* Parents that are heads now: superseded.
         live == {q \in Parents[b] : state[q] = "live"}
         \* Parents that are not heads but were published: superseded too.
         gone == {q \in Parents[b] \ live : KnownPublished(q)}
         \* Every other parent has not arrived: held ahead.
         ahead == Parents[b] \ (live \cup gone)
     IN
     \* It is running now.
     /\ phase' = [phase EXCEPT ![t] = "open"]
     \* Its snapshot is the current clock.
     /\ snap' = [snap EXCEPT ![t] = clock]
     \* A block whose record says it was published is skipped whole.
     /\ skip' = [skip EXCEPT ![t] = KnownPublished(b)]
     \* The parents to supersede.
     /\ sup' = [sup EXCEPT ![t] = live \cup gone]
     \* The parents to hold ahead.
     /\ hold' = [hold EXCEPT ![t] = ahead]
     \* Only the parents held ahead are read by part, unless that read is off.
     /\ parts' = [parts EXCEPT ![t] = IF PartRead = "Off" THEN {} ELSE ahead]
  \* The store itself does not change on a start.
  /\ UNCHANGED <<state, record, pub, clock, opAt, anyOpAt, recAt, unrelated>>

\* Whether regolith refuses t's commit: something it read changed after its
\* snapshot, in a way its read is checked against.
Refused(t) ==
  LET b == BlockOf(t) IN
  \* The record read, checked: a newer write of the block's record.
  \/ (Record = "Durable" /\ OwnRead = "Validated" /\ recAt[b] > snap[t])
  \* A read by names: a newer operand that names one of the parts read.
  \/ (PartRead = "Names" /\ \E q \in parts[t] : opAt[q] > snap[t])
  \* A read of the whole set: any newer operand at all.
  \/ (PartRead = "WholeSet" /\ parts[t] # {} /\ anyOpAt > snap[t])

\* Whether a refusal had a reason the transaction's decisions depended on: a
\* newer publication of its own block, or a newer operand naming a parent it
\* held ahead.
Related(t) ==
  \/ recAt[BlockOf(t)] > snap[t]
  \/ \E q \in parts[t] : opAt[q] > snap[t]

\* A transaction tries to commit.
Commit(t) ==
  \* Only a running transaction commits.
  /\ phase[t] = "open"
  /\ LET b == BlockOf(t)
         \* The names its operand holds: the parents and the block itself.
         named == sup[t] \cup hold[t] \cup {b}
         \* The clock value this commit takes, if it writes.
         now == clock + 1
     IN
     IF Refused(t)
       \* Refused: it starts over from a new snapshot, and the refusal is noted
       \* as unrelated when nothing it depended on changed.
       THEN /\ phase' = [phase EXCEPT ![t] = "idle"]
            /\ unrelated' = (unrelated \/ ~Related(t))
            /\ UNCHANGED <<state, record, pub, clock, opAt, anyOpAt, recAt,
                           snap, skip, sup, hold, parts>>
       ELSE IF skip[t]
         \* Already published: it writes nothing and is done.
         THEN /\ phase' = [phase EXCEPT ![t] = "done"]
              /\ UNCHANGED <<state, record, pub, clock, opAt, anyOpAt, recAt,
                             snap, skip, sup, hold, parts, unrelated>>
         \* It merges its operand and writes its record.
         ELSE /\ state' = [n \in Blocks |->
                             \* Its own block is added.
                             IF n = b THEN Add(state[n])
                             \* A parent it supersedes.
                             ELSE IF n \in sup[t] THEN Supersede(state[n])
                             \* A parent it holds ahead.
                             ELSE IF n \in hold[t] THEN SupersedeAhead(state[n])
                             \* Every other name is untouched.
                             ELSE state[n]]
              \* The record exists, whether or not records are read.
              /\ record' = [record EXCEPT ![b] = TRUE]
              \* The block is published now.
              /\ pub' = pub \cup {b}
              \* One more commit.
              /\ clock' = now
              \* Every name the operand holds was written now.
              /\ opAt' = [n \in Blocks |-> IF n \in named THEN now ELSE opAt[n]]
              \* The set was written now.
              /\ anyOpAt' = now
              \* The record was written now.
              /\ recAt' = [recAt EXCEPT ![b] = now]
              \* It is done.
              /\ phase' = [phase EXCEPT ![t] = "done"]
              /\ UNCHANGED <<snap, skip, sup, hold, parts, unrelated>>

\* A step is one transaction starting or committing.
Next ==
  \/ \E t \in Txns : Start(t)
  \/ \E t \in Txns : Commit(t)

\* Start from Init and take steps.
Spec == Init /\ [][Next]_vars

\* The same, where a step that can be taken is eventually taken.
FairSpec == Spec /\ WF_vars(Next)

----------------------------------------------------------------------------
\* Every variable holds a value of its kind.
TypeOK ==
  /\ state \in [Blocks -> States]
  /\ record \in [Blocks -> BOOLEAN]
  /\ pub \subseteq Blocks
  /\ phase \in [Txns -> {"idle", "open", "done"}]
  /\ unrelated \in BOOLEAN

\* The blocks published so far that some published block names as a parent.
Superseded == {b \in pub : \E c \in pub : b \in Parents[c]}

\* The parents some published block names that are not published themselves.
Missing == {b \in Blocks \ pub : \E c \in pub : b \in Parents[c]}

\* The set's heads are exactly the DAG's tips: every published block that no
\* published block names as a parent. A resurrection breaks it.
INV_LiveExact == {b \in Blocks : state[b] = "live"} = pub \ Superseded

\* The set holds ahead exactly the parents that have not arrived: none stays
\* ahead once it is published.
INV_AheadExact == {b \in Blocks : state[b] = "ahead"} = Missing

\* No publication is turned away by a write that named nothing it decided on.
INV_NoUnrelatedAbort == ~unrelated

\* Every publication ends: a refusal needs a newer write, and each block is
\* written once, so the retries run out.
Termination == <>(\A t \in Txns : phase[t] = "done")

====
