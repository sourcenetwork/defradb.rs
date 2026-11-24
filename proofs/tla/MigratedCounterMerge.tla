--------------------------- MODULE MigratedCounterMerge ---------------------------
(***************************************************************************)
(* The story, in plain words.                                              *)
(*                                                                         *)
(* A peer keeps a counter at a collection's source version. The receiving  *)
(* node has migrated that counter to the active version with a lens and    *)
(* stores it there. When the peer adds one, the receiver takes its stored  *)
(* document back to the source version with the lens's inverse, adds the   *)
(* peer's delta there, then migrates that result once.                     *)
(*                                                                         *)
(* A lens need not export an inverse. The receiver then cannot go back to  *)
(* the source version, so it adds the peer's delta to the migrated value   *)
(* as it is. The increment still lands.                                    *)
(*                                                                         *)
(* That shortcut is exact only for a lens that shifts the counter by a     *)
(* constant or leaves it alone. A forward-only lens that sets or scales    *)
(* the counter gives Transform(source) + 1 here, while a receiver that     *)
(* still stores the source version reads Transform(source + 1). The model  *)
(* checks a doubling lens to show where the guarantee stops; such a lens   *)
(* needs an inverse for receivers to agree.                                *)
(*                                                                         *)
(* A receiver that still stores the document at the source version has     *)
(* nothing to translate: it adds the delta to the stored value and every   *)
(* read migrates the whole document. That case is not modelled here.       *)
(*                                                                         *)
(* Who acts: the migration registers the lens, and the peer increments the *)
(* source counter.                                                         *)
(*                                                                         *)
(* What must never happen: the active value is migrated more than once, or *)
(* an increment never lands, so the active value stops naming the migrated *)
(* source value.                                                           *)
(*                                                                         *)
(* Rust code modelled:                                                     *)
(*   crates/database/db/src/merge/merge_handler/field_ops.rs               *)
(*     counter_on_active_version, apply_field_op_once                      *)
(*   crates/database/db/src/merge/merge_handler/composite_persist.rs       *)
(*     persist_merged_document (the translated active result is put whole) *)
(***************************************************************************)
EXTENDS Naturals

CONSTANTS
  \* The peer's starting counter value and the highest source value checked.
  StartSource, MaxSource,
  \* TRUE when the lens exports an inverse, FALSE for a forward-only lens.
  HasInverse,
  \* "shift" adds one to the counter, "scale" doubles it.
  LensShape,
  \* "once" is the fix. "double" plants the old active-value translation.
  \* "stuck" plants a receiver that has no answer for a forward-only lens.
  Mode
\* The start is a counter value no larger than the highest one checked, the
\* lens either has an inverse or not, and the mode is one of the three.
ASSUME StartSource \in Nat /\ MaxSource \in Nat /\ StartSource <= MaxSource
       /\ HasInverse \in BOOLEAN
       /\ LensShape \in {"shift", "scale"}
       /\ Mode \in {"once", "double", "stuck"}

VARIABLES source, active, migrated
vars == <<source, active, migrated>>

\* What the lens does to the counter when it migrates: add one, or double.
Transform(value) == IF LensShape = "shift" THEN value + 1 ELSE value * 2

\* The receivers are promised to agree when the lens has an inverse, or when
\* adding the delta to the migrated value is exact, which is the shift lens.
Covered == HasInverse \/ LensShape = "shift"

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

\* What the receiver stores after the peer's increment, given the peer's new
\* source value.
\*   "double": the old bug. It adds to the already-migrated value and then
\*             migrates the sum again.
\*   with an inverse: the stored value goes back to the source version, takes
\*             the delta there and migrates once, which is the migrated new
\*             source value.
\*   "stuck", no inverse: the planted bug. Going back fails and nothing is
\*             stored, so the active value stays where it was.
\*   "once", no inverse: the delta is added to the migrated value as it is.
Merged(newSource) ==
  IF Mode = "double" THEN Transform(active + 1)
  ELSE IF HasInverse THEN Transform(newSource)
  ELSE IF Mode = "stuck" THEN active
  ELSE active + 1

\* The peer increments its source value and the receiver merges the delta.
Increment ==
  /\ migrated
  /\ source < MaxSource
  /\ source' = source + 1
  /\ active' = Merged(source')
  /\ UNCHANGED migrated

Next == Migrate \/ Increment

Spec == Init /\ [][Next]_vars

\* Wherever the guarantee covers the lens, the active value names the
\* migrated source value, which is what a receiver storing the source
\* version reads.
INV_TransformedOnce == ~migrated \/ ~Covered \/ active = Transform(source)

\* With a forward-only lens every increment lands exactly once on the
\* migrated value: the migration ran at the start value, and each later
\* increment added one.
INV_DeltaLands ==
  ~migrated \/ HasInverse \/ active = Transform(StartSource) + (source - StartSource)

\* Every lens, covered or not, leaves the active value naming the migrated
\* source value. Only the limit configuration checks it, to show that a
\* forward-only doubling lens breaks it.
INV_ReceiversAgree == ~migrated \/ active = Transform(source)

====
