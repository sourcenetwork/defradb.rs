---- MODULE AssignmentPositionFloor ----
\* The story, in plain words.
\*
\* A caller asks for certified assignment evidence no older than a position
\* it has already seen (check_position in
\* crates/engine/vera/src/vera_rs/provider/assignments.rs). The provider's
\* guard accepts evidence at or after that term and height; at the same
\* position, only the same certified block hash is accepted. A replay of an
\* older term or height, or a different block at the floor, is refused.

EXTENDS Naturals

CONSTANTS
  Guard,        \* TRUE: the code compares the read with the caller's floor
  FloorTerm,    \* the term of the oldest accepted position
  FloorHeight,  \* the height of that position
  FloorHash,    \* the block hash at that position
  ReadTerm,     \* the term of the evidence being checked
  ReadHeight,   \* its height
  ReadHash      \* its block hash

VARIABLES accepted

vars == <<accepted>>

AtOrAboveFloor ==
  \/ ReadTerm > FloorTerm
  \/ /\ ReadTerm = FloorTerm
     /\ ReadHeight > FloorHeight
  \/ /\ ReadTerm = FloorTerm
     /\ ReadHeight = FloorHeight
     /\ ReadHash = FloorHash

TypeOK == accepted \in BOOLEAN

Init == accepted = FALSE

Check ==
  /\ ~accepted
  /\ accepted' = IF Guard => AtOrAboveFloor THEN TRUE ELSE FALSE

Next == Check

Spec == Init /\ [][Next]_vars

INV_PositionFloor ==
  ~accepted \/ AtOrAboveFloor

====
