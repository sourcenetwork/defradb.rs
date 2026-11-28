---- MODULE ChunkEmbeddingSwitch ----
\* A collection has old vectors while replacements are prepared. Search sees the
\* definition and rows together when the replacement commits.
\* crates/database/db/src/definition/patch/embedding_switch.rs switches the rows.
\* crates/database/db/src/merge/merge_handler/stale_version.rs checks the generation.
EXTENDS Naturals
\* Each setting enables one part of the atomic switch.
CONSTANTS NeedVectors, RemoveOld, FenceReplay, Authorized, CheckAccess
\* Track preparation, the active definition, and the searchable generations.
VARIABLES ready, selected, visible
\* The initial collection serves its first generation.
Init == /\ ready = FALSE
        /\ selected = 0
        /\ visible = {0}
\* A successful provider response prepares every replacement.
Prepare == /\ (~CheckAccess \/ Authorized)
           /\ ready' = TRUE
           /\ UNCHANGED <<selected, visible>>
\* Publish the new definition and rows and remove the old rows in one step.
Switch == /\ selected = 0
          /\ (~NeedVectors \/ ready)
          /\ selected' = 1
          /\ visible' = (IF RemoveOld THEN {} ELSE visible) \cup {1}
          /\ UNCHANGED ready
\* A late chunk from the first generation cannot enter the new one.
Replay == /\ selected = 1
          /\ ~FenceReplay
          /\ visible' = visible \cup {0}
          /\ UNCHANGED <<ready, selected>>
\* A failed provider response leaves the snapshot unchanged.
Fail == UNCHANGED <<ready, selected, visible>>
\* Any of the steps can occur.
Spec == Init /\ [][Prepare \/ Switch \/ Replay \/ Fail]_<<ready, selected, visible>>
\* Every published generation has its vectors.
Prepared == selected = 1 => ready
\* Only callers who may read the source can prepare replacement vectors.
AuthorizedPreparation == ready => Authorized
\* Search sees exactly the active generation.
SingleGeneration == visible = {selected}
====
