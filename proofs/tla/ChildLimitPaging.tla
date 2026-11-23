---- MODULE ChildLimitPaging ----
\* The story: a parent asks for the first few of its children, and those
\* children are streamed in an order the selected index guarantees. The scan
\* may stop as soon as that parent has its page; it must not borrow another
\* parent's page or spend another parent's scan bound.
EXTENDS Naturals, FiniteSets, Sequences

CONSTANTS
  Parents,   \* parents in the query result
  Children,  \* all children visible to the scan
  Guarded    \* whether the selected index is checked before stopping

ASSUME Cardinality(Parents) >= 2
ASSUME Cardinality(Children) >= 2
ASSUME Guarded \in BOOLEAN

VARIABLES
  parent,     \* the parent whose page is being collected
  position,   \* the current position in the ordered scan
  collected,  \* each parent's collected page, as a function to a sequence
  matched,    \* children already matched to the current parent
  done        \* all requested pages have been collected

vars == <<parent, position, collected, matched, done>>

Init ==
  /\ parent \in Parents
  /\ position = 1
  /\ collected = [p \in Parents |-> <<>>]
  /\ matched = {}
  /\ done = FALSE

\* The selected scan is ordered only when the guard runs. A filter-only or
\* different-order index must not license early stop.
ProvidesOrder == Guarded

\* Every child belongs to every parent in this abstract model, so a match is
\* simply a child not consumed for this parent yet.
NextChild ==
  Children \ matched

\* Advance the scan and collect the child if the current page still needs it.
\* Early stop is allowed only when the scan actually provides the page order.
Scan ==
  /\ ~done
  /\ collected[parent] = <<>>
  /\ \E child \in NextChild:
       /\ matched' = matched \cup {child}
       /\ IF ProvidesOrder
            THEN /\ collected' = [collected EXCEPT ![parent] = <<child>>]
                 /\ parent' = CHOOSE p \in Parents: p # parent
            ELSE /\ collected' = [collected EXCEPT ![parent] = <<child>>]
                 /\ parent' = parent
                 /\ done' = TRUE
       /\ done' = IF ProvidesOrder THEN done ELSE TRUE
       /\ UNCHANGED <<position>>

\* Without the guard, the page keeps scanning until every child is consumed.
\* This keeps the same page collection but never claims early ordering.
Drain ==
  /\ ~done
  /\ ~ProvidesOrder
  /\ collected[parent] = <<>>
  /\ \E child \in NextChild:
       /\ matched' = matched \cup {child}
       /\ collected' = [collected EXCEPT ![parent] = <<child>>]
       /\ parent' = CHOOSE p \in Parents: p # parent
       /\ done' = done
       /\ UNCHANGED <<position>>

\* Finish after every child has been offered.
Finish ==
  /\ ~done
  /\ ProvidesOrder
  /\ \A p \in Parents: collected[p] # <<>>
  /\ done' = TRUE
  /\ UNCHANGED <<parent, position, collected, matched>>

Next == Scan \/ Drain \/ Finish
Done == done /\ UNCHANGED vars
Spec == Init /\ [][Next \/ Done]_vars

TypeStays ==
  /\ parent \in Parents
  /\ position = 1
  /\ \A p \in Parents: collected[p] \in Seq(Children)
  /\ matched \in SUBSET Children
  /\ done \in BOOLEAN

\* Each parent gets exactly one child in this small page model; no parent is
\* left without a page just because another parent came first.
INV_EveryParentHasPage ==
  done => \A p \in Parents: collected[p] # <<>>

\* Early stop never creates a page from a scan that did not guarantee order.
INV_EarlyStopOnlyWhenOrdered ==
  \A p \in Parents:
    collected[p] # <<>> => ProvidesOrder \/ position = Cardinality(Children) + 1
====
