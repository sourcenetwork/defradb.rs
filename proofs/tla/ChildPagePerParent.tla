---- MODULE ChildPagePerParent ----
\* The story, in plain words.
\*
\* One parent asks for the first few children in an index order, then another
\* parent asks for its first few. The page belongs to the parent asking: in
\* per-parent mode each parent restarts the scan and fills exactly the ordered
\* page it asked for. In the old global mode, the first parent's page consumes
\* the shared scan bound, so a later parent can be left without its page.
\* A page must also stop at the limit: a scan that kept reading past it would
\* hand the parent more children than it asked for.

EXTENDS Naturals, FiniteSets, Sequences

CONSTANTS
  Mode,        \* "PerParent" or "Global"
  StopAtLimit, \* TRUE: a per-parent scan stops once its page is full
  Limit,       \* how many children one page may take
  Parents,     \* the parents making pages
  ChildrenOfA, \* child positions belonging to parent "a"
  ChildrenOfB  \* child positions belonging to parent "b"

\* The child positions that belong to parent p.
ChildrenOf(p) == IF p = "a" THEN ChildrenOfA ELSE ChildrenOfB

VARIABLES
  page,   \* parent -> the ordered child positions collected so far
  cursor, \* the next child position a global scan would read
  scans   \* how many children the global scan has consumed

\* Every variable, so a step that changes nothing can be named.
vars == <<page, cursor, scans>>

\* Each variable always holds a value of the expected shape.
TypeOK ==
  /\ page \in [Parents -> Seq(ChildrenOfA \cup ChildrenOfB)]
  /\ cursor \in 1..5
  /\ scans \in 0..Limit

\* At the start no parent has a page yet and the global scan is at the first child.
Init ==
  /\ page = [p \in Parents |-> <<>>] \* every page is empty
  /\ cursor = 1                     \* the global scan starts at position 1
  /\ scans = 0                      \* and has consumed nothing

\* How long parent p's page must be: all its children, or Limit of them if it
\* has more. This is where the limit bounds each page.
PageLen(p) ==
  IF Cardinality(ChildrenOf(p)) < Limit \* fewer children than the limit
    THEN Cardinality(ChildrenOf(p))     \* the page holds them all
    ELSE Limit                          \* otherwise the page stops at the limit

\* Parent p's n-th child in index order: the child with exactly n of p's
\* children at or before it.
NthOwned(p, n) ==
  CHOOSE c \in ChildrenOf(p) :                       \* pick the child of p
    Cardinality({d \in ChildrenOf(p) : d <= c}) = n \* that is n-th in order

\* The exact page parent p asked for: its first PageLen(p) children, in order.
RequiredPage(p) ==
  [i \in 1..PageLen(p) |-> NthOwned(p, i)]

\* How far parent p's own scan may read: up to the page length with the early
\* stop, or through every child p has without it.
ScanStop(p) ==
  IF StopAtLimit                    \* the early stop is in place
    THEN PageLen(p)                 \* so the scan ends when the page is full
    ELSE Cardinality(ChildrenOf(p)) \* otherwise it reads every child of p

\* Per-parent mode: parent p's own scan reads its next child in index order
\* and adds it to its page, until the page is full (the early stop).
PerParent(p) ==
  /\ Len(page[p]) < ScanStop(p)                                      \* the scan has not stopped yet
  /\ page' = [page EXCEPT ![p] = Append(page[p], NthOwned(p, Len(page[p]) + 1))] \* take the next child
  /\ UNCHANGED <<cursor, scans>>                                     \* the global scan is not used

\* Old global mode: one shared scan, bounded by Limit for the whole relation,
\* hands each child it reads to the parent that owns it.
Global ==
  /\ scans < Limit                                           \* the shared bound is not used up
  /\ cursor <= 4                                             \* there is a child left to read
  /\ page' = [page EXCEPT ![IF cursor <= 2 THEN "a" ELSE "b"] = \* the owner of this child
                Append(page[IF cursor <= 2 THEN "a" ELSE "b"], cursor)] \* gets it on its page
  /\ cursor' = cursor + 1                                    \* move to the next child
  /\ scans' = scans + 1                                      \* and spend one unit of the bound

\* A step is a per-parent read, or a global read when the model runs in global mode.
Next ==
  \/ \E p \in Parents : PerParent(p)
  \/ (Mode = "Global" /\ Global)

\* Start in Init, then take steps.
Spec == Init /\ [][Next]_vars

\* Same, but a parent that can still read its next child eventually does.
FairSpec ==
  /\ Spec
  /\ \A p \in Parents : WF_vars(PerParent(p))

\* Promise: every parent eventually holds exactly the ordered page it asked for.
LIVE_EveryParentCompletesOrderedPage ==
  <>(\A p \in Parents : page[p] = RequiredPage(p))

\* Promise: at every moment a page is the start of the required page, so no
\* child is out of order, foreign, or past the limit.
INV_PageIsOrderedPrefix ==
  \A p \in Parents :
    page[p] = SubSeq(RequiredPage(p), 1, Len(page[p]))

\* Promise: no page ever holds more children than the limit lets it take.
INV_PageWithinLimit ==
  \A p \in Parents : Len(page[p]) <= PageLen(p)

\* RED check: once the global bound is spent, parent "b" must still have its
\* full page. Global mode breaks it, since "a" spends the whole bound.
INV_RedSecondParentKeepsPage == scans = Limit => page["b"] = RequiredPage("b")

====
