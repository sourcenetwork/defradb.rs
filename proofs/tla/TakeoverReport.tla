---- MODULE TakeoverReport ----
\* The story, in plain words.
\*
\* A DefraDB node keeps its access rules on a Vera deployment. When another
\* device takes that deployment over, some submissions the node had seen
\* confirmed are no longer there. The node sends them again. Each replay has
\* one of three outcomes: the deployment holds it again, the node owes it
\* again through its outbox, or the node cannot restore it and says why.
\*
\* The embedder does not watch the replay. It is told once, after the node
\* has classified every submission, in a report that names the term, the cut
\* and the outcome of every replay. A lost policy creation is one of the
\* things that cannot be restored, and its reason says that.
\*
\* What must happen: every replayed submission is classified, and the report
\* is published. A replay that is never published is the bug.
\*
\* Code this models:
\*   crates/engine/vera/src/vera_rs/follow/report.rs   TakeoverReport,
\*       Applied, Unrestorable
\*   crates/engine/vera/src/vera_rs/follow/mod.rs      reconcile (the replay)
\*   crates/engine/vera/src/vera_rs/provider/terms.rs  Follower::publish

EXTENDS Naturals, Sequences

CONSTANTS
    Term,         \* the certified term the report names
    Cut,          \* the height at or below which the new term keeps history
    Submissions,  \* the submissions one takeover replayed
    Publish,      \* TRUE: the provider publishes the report
    NameUnrestorable \* TRUE: an unrestorable replay says why

VARIABLES
    replayed,   \* the submissions the takeover replayed
    outcome,    \* the outcome of each replayed submission
    reason,     \* why an unrestorable submission could not be restored
    published   \* whether the report was published

vars == <<replayed, outcome, reason, published>>

\* The kinds the event carries.
Kinds == {"applied", "owed", "unrestorable"}

\* A valid outcome names its kind, and an unrestorable one says why.
ValidOutcome(i) ==
    /\ outcome[i] \in Kinds
    /\ (outcome[i] = "unrestorable" => reason[i] # "")

\* Every variable holds a value of the expected kind.
TypeOK ==
    /\ replayed \subseteq Submissions            \* submissions replayed so far
    /\ outcome \in [Submissions -> Kinds]        \* each replay has one outcome
    /\ reason \in [Submissions -> STRING]        \* its reason, when it needs one
    /\ published \in BOOLEAN                     \* the report may not be sent yet

\* At the start: nothing is replayed and no report is published.
Init ==
    /\ replayed = {}                             \* no replay has run
    /\ outcome = [i \in Submissions |-> "applied"] \* each starts classified
    /\ reason = [i \in Submissions |-> ""]       \* and starts unnamed
    /\ published = FALSE                         \* and has published nothing

\* One replay is classified and, with publishing on, reported at once. The
\* cut and term are in the report too; this model is only their partition.
Replay(i, kind) ==
    /\ i \in Submissions                         \* a submission to replay
    /\ i \notin replayed                         \* replayed once
    /\ kind \in Kinds                            \* the outcome the replay found
    /\ replayed' = replayed \cup {i}             \* it is replayed
    /\ outcome' = [outcome EXCEPT ![i] = kind]     \* its outcome is recorded
    /\ reason' = [reason EXCEPT ![i] =             \* an unrestorable one names
                     IF kind = "unrestorable" /\ NameUnrestorable
                     THEN "lost policy creation" ELSE ""]
    /\ published' = Publish                      \* and the report is sent

\* The provider stops when every submission has been replayed.
Stop ==
    /\ replayed = Submissions                    \* every replay has run
    /\ UNCHANGED vars                            \* and nothing else happens

Next == \E i \in Submissions : \E kind \in Kinds : Replay(i, kind) \/ Stop

Spec == Init /\ [][Next]_vars /\ \A i \in Submissions : WF_vars(Replay(i, "applied"))

\* Every replay names one of the three event kinds, and an unrestorable one
\* carries its reason.
INV_Classified ==
    \A i \in replayed : ValidOutcome(i)

\* A completed replay is not left unpublished.
INV_Published ==
    replayed # Submissions \/ published

\* Every submission is eventually replayed, so its outcome is reported.
LIVE_Reported ==
    <>[](replayed = Submissions /\ \A i \in replayed : ValidOutcome(i))

====
