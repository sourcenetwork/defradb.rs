---- MODULE ChildPagePerParent ----
\* The story, in plain words.
\*
\* One parent asks for the first few children in an index order, then another
\* parent asks for its first few. The page belongs to the parent asking: in
\* per-parent mode each parent restarts the scan and can fill its own page. In
\* the old global mode, the first parent's page consumes the shared scan bound,
\* so a later parent can be left with nothing.

EXTENDS Naturals

CONSTANTS
  Mode,        \* "PerParent" or "Global"
  Limit,       \* how many children one page may take
  Parents,     \* the parents making pages
  ChildrenOfA, \* child positions belonging to parent "a"
  ChildrenOfB  \* child positions belonging to parent "b"

ChildrenOf(p) == IF p = "a" THEN ChildrenOfA ELSE ChildrenOfB

VARIABLES
  page,   \* parent -> the child position its page starts at, 0 before one
  cursor, \* the next child position a global scan would read
  scans   \* how many children the global scan has consumed

vars == <<page, cursor, scans>>

TypeOK ==
  /\ page \in [Parents -> 0..4]
  /\ cursor \in 1..5
  /\ scans \in 0..Limit

Init ==
  /\ page = [p \in Parents |-> 0]
  /\ cursor = 1
  /\ scans = 0

PerParent(p) ==
  /\ page[p] = 0
  /\ \E c \in ChildrenOf(p) :
       /\ page' = [page EXCEPT ![p] = c]
  /\ UNCHANGED <<cursor, scans>>

Global ==
  /\ scans < Limit
  /\ cursor <= 4
  /\ page' = [page EXCEPT ![IF cursor <= 2 THEN "a" ELSE "b"] = cursor]
  /\ cursor' = cursor + 1
  /\ scans' = scans + 1

Next ==
  \/ \E p \in Parents : PerParent(p)
  \/ (Mode = "Global" /\ Global)

Spec == Init /\ [][Next]_vars

FairSpec ==
  /\ Spec
  /\ \A p \in Parents : WF_vars(PerParent(p))

LIVE_EveryParentPage == <>(\A p \in Parents : page[p] # 0)

INV_SecondParentKeepsPage == scans = Limit => page["b"] # 0

====
