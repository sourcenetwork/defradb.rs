---- MODULE ChildPagePerParent ----
\* The story, in plain words.
\*
\* Several parents each ask for a page of their own children: at most Limit of
\* them. A scan hands out children one at a time, and every child it hands out
\* is charged to a bound. In per-parent mode each parent has its own bound, so
\* every parent can fill its own page. In the old global mode all parents share
\* one bound, so the first pages use it up and a later parent is left short.
\* What must never happen: the scan stops while a parent still has children it
\* was owed.

EXTENDS Naturals, FiniteSets \* numbers, and Cardinality to count a set

CONSTANTS
  Mode,        \* "PerParent" or "Global": who owns the bound
  Limit,       \* how many children one page may take
  Parents,     \* the parents asking for a page
  ChildrenOfA, \* the children belonging to parent "a"
  ChildrenOfB  \* the children belonging to parent "b"

\* The children that belong to parent p.
ChildrenOf(p) == IF p = "a" THEN ChildrenOfA ELSE ChildrenOfB

\* The smaller of two numbers.
Min(x, y) == IF x < y THEN x ELSE y

\* The page parent p is owed: Limit children, or all of them if it has fewer.
Owed(p) == Min(Limit, Cardinality(ChildrenOf(p)))

\* Every child there is, used only to bound the counters.
MostChildren == Cardinality(ChildrenOfA) + Cardinality(ChildrenOfB)

VARIABLES
  taken, \* parent -> how many children its page holds so far
  scans  \* how many children the scan has handed out in total

\* Both variables together, for "nothing else changes" statements.
vars == <<taken, scans>>

\* Every counter stays inside its small range.
TypeOK ==
  /\ taken \in [Parents -> 0..MostChildren] \* a page count per parent
  /\ scans \in 0..MostChildren              \* the total handed out

\* At the start no page holds a child and the scan has handed out nothing.
Init ==
  /\ taken = [p \in Parents |-> 0] \* every page is empty
  /\ scans = 0                     \* nothing handed out yet

\* What has been charged to the bound parent p reads: its own page in
\* per-parent mode, everything the scan handed out in global mode.
Charged(p) == IF Mode = "PerParent" THEN taken[p] ELSE scans

\* Parent p can take one more child: its bound has room and it has a child left.
CanTake(p) ==
  /\ Charged(p) < Limit                     \* the bound p reads is not used up
  /\ taken[p] < Cardinality(ChildrenOf(p))  \* p still has a child not in its page

\* The scan hands parent p one more child.
Take(p) ==
  /\ CanTake(p)                             \* only while p may take one
  /\ taken' = [taken EXCEPT ![p] = @ + 1]   \* p's page grows by one child
  /\ scans' = scans + 1                     \* the scan has handed out one more

\* One step: some parent takes one child.
Next == \E p \in Parents : Take(p)

\* The model: start empty, then take children one at a time.
Spec == Init /\ [][Next]_vars

\* The same model, with the promise that a parent who can keep taking does.
FairSpec ==
  /\ Spec                                   \* the plain model
  /\ \A p \in Parents : WF_vars(Take(p))    \* no parent is ignored forever

\* The scan has stopped: no parent can take another child.
Finished == \A p \in Parents : ~CanTake(p)

\* Promise: sooner or later every parent holds the whole page it is owed.
LIVE_EveryParentPage == <>(\A p \in Parents : taken[p] = Owed(p))

\* Promise: when the scan stops, the second parent holds the whole page it is
\* owed, however much the first parent took before it.
INV_SecondParentKeepsPage == Finished => taken["b"] = Owed("b")

\* Promise: no page ever holds more than Limit children.
INV_PageWithinLimit == \A p \in Parents : taken[p] <= Limit

====
