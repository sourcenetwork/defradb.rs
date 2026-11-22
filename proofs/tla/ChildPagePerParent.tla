---- MODULE ChildPagePerParent ----
\* The story, in plain words.
\*
\* One parent asks for the first few children in an index order, then another
\* parent asks for its first few. The page belongs to the parent asking: in
\* per-parent mode each parent restarts the scan and fills exactly the ordered
\* page it asked for. In the old global mode, the first parent's page consumes
\* the shared scan bound, so a later parent can be left without its page.

EXTENDS Naturals, FiniteSets, Sequences

CONSTANTS
  Mode,        \* "PerParent" or "Global"
  Limit,       \* how many children one page may take
  Parents,     \* the parents making pages
  ChildrenOfA, \* child positions belonging to parent "a"
  ChildrenOfB  \* child positions belonging to parent "b"

ChildrenOf(p) == IF p = "a" THEN ChildrenOfA ELSE ChildrenOfB

VARIABLES
  page,   \* parent -> the ordered child positions collected so far
  cursor, \* the next child position a global scan would read
  scans   \* how many children the global scan has consumed

vars == <<page, cursor, scans>>

TypeOK ==
  /\ page \in [Parents -> Seq(ChildrenOfA \cup ChildrenOfB)]
  /\ cursor \in 1..5
  /\ scans \in 0..Limit

Init ==
  /\ page = [p \in Parents |-> <<>>]
  /\ cursor = 1
  /\ scans = 0

\* The first page requested from a parent is the parent's children in ascending
\* index order, truncated to the limit.
PageLen(p) ==
  IF Cardinality(ChildrenOf(p)) < Limit
    THEN Cardinality(ChildrenOf(p))
    ELSE Limit

NthOwned(p, n) ==
  CHOOSE c \in ChildrenOf(p) :
    Cardinality({d \in ChildrenOf(p) : d <= c}) = n

RequiredPage(p) ==
  [i \in 1..PageLen(p) |-> NthOwned(p, i)]

PerParent(p) ==
  /\ Len(page[p]) < PageLen(p)
  /\ page' = [page EXCEPT ![p] = Append(page[p], NthOwned(p, Len(page[p]) + 1))]
  /\ UNCHANGED <<cursor, scans>>

Global ==
  /\ scans < Limit
  /\ cursor <= 4
  /\ page' = [page EXCEPT ![IF cursor <= 2 THEN "a" ELSE "b"] =
                Append(page[IF cursor <= 2 THEN "a" ELSE "b"], cursor)]
  /\ cursor' = cursor + 1
  /\ scans' = scans + 1

Next ==
  \/ \E p \in Parents : PerParent(p)
  \/ (Mode = "Global" /\ Global)

Spec == Init /\ [][Next]_vars

FairSpec ==
  /\ Spec
  /\ \A p \in Parents : WF_vars(PerParent(p))

LIVE_EveryParentCompletesOrderedPage ==
  <>(\A p \in Parents : page[p] = RequiredPage(p))

INV_PageIsOrderedPrefix ==
  \A p \in Parents :
    page[p] = SubSeq(RequiredPage(p), 1, Len(page[p]))

INV_RedSecondParentKeepsPage == scans = Limit => page["b"] = RequiredPage("b")

====
