---- MODULE AssignmentPosition ----
\* A certified assignment response is checked against an optional caller floor.
\* check_position compares (term,height) lexicographically, requiring the
\* same hash only when those coordinates are equal. Hash parsing and the light
\* client's certificate and assignment-proof verification are assumed here.
\* Rust: crates/engine/vera/src/vera_rs/provider/assignments.rs check_position.
EXTENDS Naturals
CONSTANTS CheckStale, CheckHash
VARIABLES current, floor, hasFloor, checked, accepted
\* The complete state changes together on the check.
vars == <<current, floor, hasFloor, checked, accepted>>
\* A bounded sample of terms, heights and distinct block hashes.
Positions == [term : 0..1, height : 0..1, hash : 0..1]
\* Term takes precedence over height when comparing positions.
Earlier(c, f) == c.term < f.term \/ (c.term = f.term /\ c.height < f.height)
\* Equal coordinates still need a hash comparison.
SameHeight(c, f) == c.term = f.term /\ c.height = f.height
\* Choose any current position, floor and whether a floor was supplied.
Init == /\ current \in Positions
        /\ floor \in Positions
        /\ hasFloor \in BOOLEAN
        /\ checked = FALSE
        /\ accepted = FALSE
\* Apply both guards once; the red configurations remove one guard apiece.
Check == /\ ~checked
         /\ checked' = TRUE
         /\ accepted' = (~hasFloor \/ ((~CheckStale \/ ~Earlier(current, floor))
                 /\ (~CheckHash \/ ~SameHeight(current, floor) \/ current.hash = floor.hash)))
         /\ UNCHANGED <<current, floor, hasFloor>>
\* The check is the only modeled action; stuttering is permitted afterward.
Next == Check
Spec == Init /\ [][Next]_vars
\* Accepted evidence is never earlier than the floor.
INV_NoStale == accepted /\ hasFloor => ~Earlier(current, floor)
\* At the floor's coordinates the accepted block is exactly the floor block.
INV_SameHash == accepted /\ hasFloor /\ SameHeight(current, floor) => current.hash = floor.hash
\* Valid evidence is not needlessly refused.
INV_AcceptValid == checked /\ (~hasFloor \/ (~Earlier(current, floor) /\
    (~SameHeight(current, floor) \/ current.hash = floor.hash))) => accepted
====
