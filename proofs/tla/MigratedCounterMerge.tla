--------------------------- MODULE MigratedCounterMerge ---------------------------
(***************************************************************************)
(* The story, in plain words.                                              *)
(*                                                                         *)
(* A peer keeps a counter at a collection's source version. The receiving  *)
(* node has migrated that counter to the active version with a lens. When  *)
(* the peer adds one, the receiver first reads the source-version value,   *)
(* adds the peer's delta there, then migrates that result once.            *)
(*                                                                         *)
(* Who acts: the migration registers the lens, and the peer increments the *)
(* source counter.                                                         *)
(*                                                                         *)
(* What must never happen: the active value is migrated more than once, so *)
(* it stops naming the migrated source value.                              *)
(*                                                                         *)
(* Rust code modelled:                                                     *)
(*   crates/database/db/src/merge/merge_handler/field_ops.rs               *)
(*     counter_document_on_active_version, counter_result_on_active_version*)
(*   crates/database/db/src/merge/merge_handler/composite_persist.rs       *)
(*     persist_merged_document (the translated active result is put whole) *)
(***************************************************************************)
EXTENDS Naturals

CONSTANTS
  \* The peer's starting counter value and the highest source value checked.
  StartSource, MaxSource,
  \* "once" is the fix. "double" plants the old active-value translation.
  Mode
ASSUME StartSource \in Nat /\ MaxSource \in Nat /\ StartSource <= MaxSource
       /\ Mode \in {"once", "double"}

VARIABLES source, active, migrated
vars == <<source, active, migrated>>

\* The lens in the focused scenario adds one to the counter.
Transform(value) == value + 1

TypeOK == /\ source \in 0..MaxSource
          /\ active \in Nat
          /\ migrated \in BOOLEAN

Init == /\ source = StartSource
        /\ active = 0
        /\ migrated = FALSE

\* The migration reads the source version and stores its active form once.
Migrate ==
  /\ ~migrated
  /\ migrated' = TRUE
  /\ active' = Transform(source)
  /\ UNCHANGED source

\* The peer increments its source value. The receiver translates that delta
\* onto the source projection. The red mode plants the old bug: it adds to
\* the already-migrated value and then migrates the sum again.
Increment ==
  /\ migrated
  /\ source < MaxSource
  /\ source' = source + 1
  /\ active' = IF Mode = "once"
                 THEN Transform(source')
                 ELSE Transform(active + 1)
  /\ UNCHANGED migrated

Next == Migrate \/ Increment

Spec == Init /\ [][Next]_vars

\* The active value always names the migrated source value.
INV_TransformedOnce == ~migrated \/ active = Transform(source)

====
