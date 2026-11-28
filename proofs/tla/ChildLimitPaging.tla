---- MODULE ChildLimitPaging ----
\* Each parent scans the same child stream and keeps only its own children.
\* This checks parent scoping and the ordered early stop. Vector candidate
\* widening and scalar filtering are outside this specification.
\* The scan may stop at offset + limit only when the selected index supplies
\* the requested order. An unordered scan must be exhausted before sorting.
\* Models TypeJoinMany::next_per_parent in
\* crates/engine/query/src/plan/type_join/type_join_many/children.rs.
EXTENDS Naturals, FiniteSets, Sequences

\* Children have a parent and a distinct requested-order rank per parent.
CONSTANTS Parents, Children, OrderedScan, Offset, Limit, Guarded

\* Ranks and owners define three children per parent.
Owner == [c \in Children |-> IF c <= 3 THEN "a" ELSE "b"]
Rank == [c \in Children |-> IF c <= 3 THEN c ELSE c - 3]
ScanOrder == IF OrderedScan THEN <<1, 2, 3, 4, 5, 6>> ELSE <<1, 3, 2, 4, 6, 5>>

\* The offset and limit select ranks in the requested child order.
ASSUME /\ Parents # {} /\ Children # {} /\ Offset \in Nat /\ Limit \in Nat
       /\ Limit > 0 /\ Guarded \in BOOLEAN
       /\ Owner \in [Children -> Parents] /\ Rank \in [Children -> Nat]
       /\ ScanOrder \in Seq(Children) /\ Len(ScanOrder) = Cardinality(Children)
       /\ {ScanOrder[i] : i \in 1..Len(ScanOrder)} = Children
       /\ \A p \in Parents : \A a,b \in Children :
            (Owner[a] = p /\ Owner[b] = p /\ a # b) => Rank[a] # Rank[b]

\* Each parent owns a cursor, matched children, and an independently closed page.
VARIABLES position, matched, done, early
vars == <<position, matched, done, early>>

\* The requested scan order is checked for each parent's children.
Ordered == \A i,j \in 1..Len(ScanOrder) :
  (i < j /\ Owner[ScanOrder[i]] = Owner[ScanOrder[j]]) =>
    Rank[ScanOrder[i]] < Rank[ScanOrder[j]]

\* A page contains ranks after the offset and within the requested limit.
Page(items) == {c \in items :
  /\ Cardinality({other \in items : Rank[other] < Rank[c]}) >= Offset
  /\ Cardinality({other \in items : Rank[other] < Rank[c]}) < Offset + Limit}

\* Each parent's correct page comes from all of that parent's children.
Wanted(p) == Page({c \in Children : Owner[c] = p})

\* All parents start before the first scanned child, with no matches.
Init ==
  /\ position = [p \in Parents |-> 1]
  /\ matched = [p \in Parents |-> {}]
  /\ done = [p \in Parents |-> FALSE]
  /\ early = [p \in Parents |-> FALSE]

\* One parent consumes one child; other parents' pages remain untouched.
Scan(p) ==
  /\ ~done[p] /\ position[p] <= Len(ScanOrder)
  /\ LET child == ScanOrder[position[p]]
         nextMatches == IF Owner[child] = p THEN matched[p] \cup {child} ELSE matched[p]
         mayStop == (Ordered \/ ~Guarded) /\ Cardinality(nextMatches) >= Offset + Limit
     IN /\ matched' = [matched EXCEPT ![p] = nextMatches]
        /\ position' = [position EXCEPT ![p] = @ + 1]
        /\ done' = [done EXCEPT ![p] = mayStop \/ position[p] = Len(ScanOrder)]
        /\ early' = [early EXCEPT ![p] = mayStop /\ position[p] < Len(ScanOrder)]

\* A parent with an empty stream finishes without returning children.
Finish(p) ==
  /\ ~done[p] /\ position[p] > Len(ScanOrder)
  /\ done' = [done EXCEPT ![p] = TRUE]
  /\ UNCHANGED <<position, matched, early>>

\* Any unfinished parent may take its next scan step.
Next == \E p \in Parents : Scan(p) \/ Finish(p)
Spec == Init /\ [][Next]_vars

\* A parent that can continue scanning eventually gets another turn.
FairSpec == Spec /\ \A p \in Parents : WF_vars(Scan(p) \/ Finish(p))

\* Cursors, matches, and completion markers remain within their domains.
TypeStays ==
  /\ position \in [Parents -> 1..(Len(ScanOrder) + 1)]
  /\ matched \in [Parents -> SUBSET Children]
  /\ done \in [Parents -> BOOLEAN] /\ early \in [Parents -> BOOLEAN]

\* A completed parent returns exactly its requested offset and limit page.
INV_CompletedPage == \A p \in Parents : done[p] => Page(matched[p]) = Wanted(p)

\* Stopping before the end is safe only when the actual scan is ordered.
INV_EarlyStopOnlyWhenOrdered == \A p \in Parents : early[p] => Ordered

\* Every parent eventually finishes its page when scan steps are scheduled.
LIVE_AllPagesComplete == <>(\A p \in Parents : done[p])
====
