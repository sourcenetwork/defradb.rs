---- MODULE LensSettlement ----
\* A patch commits a collection version, registers its stored lens, and then
\* settles any owed reindex. Settlement must not run before registration.
\* Rust owners: crates/database/db/src/definition/patch/mod.rs
\* (patch_collection), crates/database/db/src/definition/patch/in_place.rs
\* (update_version_in_place), and
\* crates/database/db/src/definition/migration/reindex/page.rs (walk).
EXTENDS Naturals, Sequences

CONSTANT Mode
ASSUME Mode \in {"safe", "early"}

VARIABLES committed, lensRegistered, settled
vars == <<committed, lensRegistered, settled>>

TypeOK == /\ committed \in BOOLEAN
          /\ lensRegistered \in BOOLEAN
          /\ settled \in BOOLEAN

Init == /\ committed = FALSE
        /\ lensRegistered = FALSE
        /\ settled = FALSE

Commit == /\ ~committed
          /\ committed' = TRUE
          /\ UNCHANGED <<lensRegistered, settled>>

Register == /\ committed
            /\ ~lensRegistered
            /\ lensRegistered' = TRUE
            /\ UNCHANGED <<committed, settled>>

Settle ==
  /\ committed
  /\ (Mode = "early" \/ lensRegistered)
  /\ ~settled
  /\ lensRegistered' = IF Mode = "safe" THEN lensRegistered ELSE FALSE
  /\ settled' = TRUE
  /\ UNCHANGED committed

Next == Commit \/ Register \/ Settle
Spec == Init /\ [][Next]_vars

INV_SettleAfterLens == settled => lensRegistered
====
