--------------------------- MODULE GovernedByRoot ---------------------------
\* Three nodes hold the same three collections. "book" declares a governance
\* root in its own definition. "letter" declares none, but some nodes claim
\* it by name when they are built. "note" declares none and nobody claims it.
\* Composites reach each node from its peers, in any order.
\*
\* Who acts: a node, each time a composite reaches it. It asks whether the
\* composite's collection is governed there. The rule has two arms: the
\* collection's name was claimed on this node, or this node has a merge
\* validator and the collection declares a root. If neither arm holds, the
\* composite merges as in any collection. If one does and the node has a
\* validator, the validator judges it, and it merges only when accepted. If
\* the name was claimed and the node has no validator, the composite is held
\* unmerged, as it always was.
\*
\* What must never happen: a node with a validator merges a composite of a
\* rooted or claimed collection that its validator did not accept; a node
\* with a validator holds a rooted composite without judging it; two nodes
\* with validators split, one merging what the other rejected; a node with
\* no validator treats a collection differently from before roots governed
\* anything (it holds exactly what it claimed by name, and merges the rest);
\* a collection that is neither rooted nor claimed is held or rejected.
\*
\* What is allowed, on purpose: a node with no validator merges a rooted
\* composite unjudged, even one that a validator elsewhere rejects. Such a
\* node is treated as faulty; what it should do instead is an open question
\* left to a later change.
\*
\* The nodes differ in what they were given when they were built. "named"
\* has a validator and claimed "book" and "letter" by name. "late" has a
\* validator and claimed nothing, as a node that started before "book" was
\* defined. "plain" has no validator and claimed "letter".
\*
\* Rust: crates/database/db/src/merge/governance/validator.rs
\* MergeGovernance::governs; crates/database/db/src/merge/governance/judge.rs
\* DbMergeHandler::is_governed and DbMergeHandler::judge_governed.

\* Each constant plants one bug when TRUE.
\* DropRootArm forgets the root arm: only claimed names are governed.
\* DropNameArm forgets the name arm: only rooted collections are governed.
\* RootWithoutValidator lets the root govern on a node with no validator.
\* ValidatorOnlyForNamed finds the validator only for a claimed collection.
\* ClaimAll treats every collection as governed.
CONSTANTS DropRootArm, DropNameArm, RootWithoutValidator,
          ValidatorOnlyForNamed, ClaimAll

\* The three nodes.
Nodes == {"named", "late", "plain"}
\* The nodes that have a merge validator installed.
Validating == {"named", "late"}
\* The collections whose definition declares a governance root.
Rooted == {"book"}
\* The collections a node claimed by name when it was built.
Named(n) == CASE n = "named" -> {"book", "letter"}
              [] n = "plain" -> {"letter"}
              [] OTHER -> {}
\* The composites that can arrive: two of "book", one of "letter" and one of
\* "note".
Composites == {"good", "bad", "card", "memo"}
\* The collection a composite belongs to.
CollOf(w) == CASE w = "card" -> "letter"
               [] w = "memo" -> "note"
               [] OTHER -> "book"
\* The composites a validator rejects on their own content.
Bad == {"bad"}

\* merged[n] is what node n has merged, and accepted[n] what its validator
\* accepted. rejected[n] is what its validator rejected. held[n] is what it
\* left unmerged without a verdict.
VARIABLES merged, accepted, rejected, held
\* Every changing variable.
vars == <<merged, accepted, rejected, held>>

\* At the start no node has seen any composite.
Init == /\ merged = [n \in Nodes |-> {}]
        /\ accepted = [n \in Nodes |-> {}]
        /\ rejected = [n \in Nodes |-> {}]
        /\ held = [n \in Nodes |-> {}]

\* The name arm: the node claimed the collection's name. DropNameArm
\* switches it off.
NameArm(n, c) == c \in Named(n) /\ ~DropNameArm
\* The root arm: the collection declares a root and the node has a
\* validator. RootWithoutValidator drops the validator condition, and
\* DropRootArm switches the arm off.
RootArm(n, c) == /\ c \in Rooted
                 /\ (n \in Validating \/ RootWithoutValidator)
                 /\ ~DropRootArm
\* A collection is governed at a node when either arm holds. ClaimAll
\* governs everything.
Governed(n, c) == ClaimAll \/ NameArm(n, c) \/ RootArm(n, c)

\* Whether the node finds a validator for a governed collection: it does
\* when it has one installed. ValidatorOnlyForNamed finds it only for a
\* collection whose name was claimed.
HasValidator(n, c) == /\ n \in Validating
                      /\ (c \in Named(n) \/ ~ValidatorOnlyForNamed)

\* A composite the node has not dealt with yet.
Fresh(n, w) == w \notin (merged[n] \cup rejected[n] \cup held[n])

\* A composite of an ungoverned collection merges without being judged.
MergeUngoverned(n, w) ==
    \* The node has not dealt with it.
    /\ Fresh(n, w)
    \* Its collection is not governed here.
    /\ ~Governed(n, CollOf(w))
    \* So it merges.
    /\ merged' = [merged EXCEPT ![n] = @ \cup {w}]
    \* No validator was asked and nothing was held.
    /\ UNCHANGED <<accepted, rejected, held>>

\* The validator accepts a governed composite, and it merges.
Accept(n, w) ==
    \* The node has not dealt with it.
    /\ Fresh(n, w)
    \* Its collection is governed here.
    /\ Governed(n, CollOf(w))
    \* A validator is found, and it finds nothing wrong.
    /\ HasValidator(n, CollOf(w)) /\ w \notin Bad
    \* The verdict is recorded.
    /\ accepted' = [accepted EXCEPT ![n] = @ \cup {w}]
    \* And the composite merges.
    /\ merged' = [merged EXCEPT ![n] = @ \cup {w}]
    \* Nothing was rejected or held.
    /\ UNCHANGED <<rejected, held>>

\* The validator rejects a governed composite, and it never merges.
Reject(n, w) ==
    \* The node has not dealt with it.
    /\ Fresh(n, w)
    \* Its collection is governed here.
    /\ Governed(n, CollOf(w))
    \* A validator is found, and it refuses the composite.
    /\ HasValidator(n, CollOf(w)) /\ w \in Bad
    \* The verdict is recorded.
    /\ rejected' = [rejected EXCEPT ![n] = @ \cup {w}]
    \* Nothing merges.
    /\ UNCHANGED <<merged, accepted, held>>

\* No validator is found for a governed composite, so it is held unmerged
\* with no verdict.
Hold(n, w) ==
    \* The node has not dealt with it.
    /\ Fresh(n, w)
    \* Its collection is governed here.
    /\ Governed(n, CollOf(w))
    \* No validator is found for it.
    /\ ~HasValidator(n, CollOf(w))
    \* It is held.
    /\ held' = [held EXCEPT ![n] = @ \cup {w}]
    \* Nothing merges and no verdict is recorded.
    /\ UNCHANGED <<merged, accepted, rejected>>

\* Any composite may reach any node next.
Next == \E n \in Nodes, w \in Composites :
            \/ MergeUngoverned(n, w)
            \/ Accept(n, w)
            \/ Reject(n, w)
            \/ Hold(n, w)

\* The execution can also stutter.
Spec == Init /\ [][Next]_vars

\* A node with a validator merges a composite of a rooted or claimed
\* collection only when its validator accepted it.
INV_JudgedWhereValidating ==
    \A n \in Validating : \A w \in merged[n] :
        (CollOf(w) \in Rooted \/ CollOf(w) \in Named(n)) => w \in accepted[n]

\* A node with a validator never holds a rooted composite without judging it.
INV_ValidatorNeverHoldsRooted ==
    \A n \in Validating : \A w \in held[n] : CollOf(w) \notin Rooted

\* Two nodes with validators never split: neither merges what the other's
\* validator rejected. A node with no validator is outside this promise.
INV_NoSplitAmongValidating ==
    \A a \in Validating : \A b \in Validating : merged[a] \cap rejected[b] = {}

\* A node with no validator does what it did before a root governed
\* anything: it holds exactly the composites of the collections it claimed
\* by name, merges every other, and rejects nothing.
INV_NoValidatorAsBefore ==
    \A n \in Nodes \ Validating :
        /\ \A w \in held[n] : CollOf(w) \in Named(n)
        /\ \A w \in merged[n] : CollOf(w) \notin Named(n)
        /\ rejected[n] = {}

\* Only a collection that the node claimed, or that declares a root on a
\* node with a validator, is ever held back or rejected.
INV_UngovernedUntouched ==
    \A n \in Nodes : \A w \in (held[n] \cup rejected[n]) :
        \/ CollOf(w) \in Named(n)
        \/ (CollOf(w) \in Rooted /\ n \in Validating)
=============================================================================
