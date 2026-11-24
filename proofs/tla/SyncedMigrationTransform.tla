---- MODULE SyncedMigrationTransform ----
\* A synced patch supplies a head but not its migration transform. A local
\* placeholder supplies it only if its edge names that head; a real version
\* already held is left untouched. Storage success is assumed.
\* Rust: crates/database/db/src/merge/merge_handler/definition.rs
\* process_collection_definition_delta (transform_for_head and held gate).
EXTENDS Naturals
CONSTANTS KeepTransform, CheckHead
VARIABLES held, placeholderHead, placeholderTransform, incomingHead, resultHead, resultTransform, done
\* State for one incoming definition and its local same-version record.
vars == <<held, placeholderHead, placeholderTransform, incomingHead, resultHead, resultTransform, done>>
\* Choose whether there is no held version, a placeholder, or a real version.
Init == /\ held \in {"absent", "placeholder", "real"}
        /\ placeholderHead \in 0..1
        /\ placeholderTransform \in {"none", "lens"}
        /\ incomingHead \in 0..1
        /\ resultHead = placeholderHead
        /\ resultTransform = placeholderTransform
        /\ done = FALSE
\* The real record stays intact; a new record uses the incoming head and
\* inherits only a matching placeholder's transform.
Merge == /\ ~done
         /\ done' = TRUE
         /\ resultHead' = IF held = "real" THEN resultHead ELSE incomingHead
         /\ resultTransform' = IF held = "real" THEN resultTransform
             ELSE IF KeepTransform /\ held = "placeholder" /\
                     (~CheckHead \/ placeholderHead = incomingHead)
                  THEN placeholderTransform ELSE "none"
         /\ UNCHANGED <<held, placeholderHead, placeholderTransform, incomingHead>>
\* One merge followed by stuttering.
Next == Merge
Spec == Init /\ [][Next]_vars
\* A placeholder with a matching edge retains its transform.
INV_Keep == done /\ held = "placeholder" /\ placeholderHead = incomingHead =>
    resultHead = incomingHead /\ resultTransform = placeholderTransform
\* An unrelated predecessor's transform is never attached to this edge.
INV_Match == done /\ held = "placeholder" /\ placeholderHead # incomingHead =>
    resultHead = incomingHead /\ resultTransform = "none"
\* Repeated receipt of a real definition cannot replace its local edge.
INV_Real == done /\ held = "real" =>
    resultHead = placeholderHead /\ resultTransform = placeholderTransform
====
