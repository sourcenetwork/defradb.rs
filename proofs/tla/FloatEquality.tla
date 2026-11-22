---- MODULE FloatEquality ----
\* A filter asks whether a stored float equals its target. The exact rule
\* answers yes only for the same number. A tolerant rule may answer yes for
\* two different numbers.
\* Rust owners: crates/engine/query/src/mapper/filter/eval/operators.rs
\* (values_equal) and crates/engine/storage/src/index/matcher/mod.rs
\* (values_equal).
EXTENDS Naturals

\* Values are the stored floats; Target is the filter's operand; Epsilon is
\* the old tolerance; Mode chooses the exact rule or plants the old bug.
CONSTANTS Values, Target, Epsilon, Mode
ASSUME Target \in Values
       /\ Epsilon \in Nat
       /\ Mode \in {"exact", "epsilon"}

VARIABLES matched, asked
vars == <<matched, asked>>

TypeOK == /\ matched \subseteq Values
          /\ asked \in BOOLEAN

Init == /\ matched = {}
        /\ asked = FALSE

Near(v, t) == /\ v <= t + Epsilon
              /\ t <= v + Epsilon

Ask ==
  /\ ~asked
  /\ matched' = {v \in Values :
                   IF Mode = "exact"
                   THEN v = Target
                   ELSE Near(v, Target)}
  /\ asked' = TRUE

Next == Ask
Spec == Init /\ [][Next]_vars

\* An exact equality never matches a value other than its target.
INV_ExactOnly == matched \subseteq {Target}

====
