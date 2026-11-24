------------------------- MODULE ExplainIndexName -------------------------
\* A join replaces a child scan's metrics in its execution explain. A direct
\* foreign-key lookup supplies the name of the actual index used; if there is
\* no direct lookup, the original child plan supplies its optional index name.
\* Rust: crates/engine/query/src/plan/type_join/mod.rs scan_node_with_index_name;
\* type_join_one.rs explain_execute_inner.
EXTENDS Naturals
\* Lookup and Child are index names or the sentinel NoneName. PreferChild
\* plants the bug that ignores a configured direct lookup.
CONSTANTS Lookup, Child, NoneName, PreferChild
\* output is the name selected for the scan, or NoneName; done marks output.
VARIABLES output, done
\* All variables that change.
vars == <<output, done>>
\* Before explain is built no name has been output.
Init == /\ output = NoneName /\ done = FALSE
\* Prefer the lookup unless the broken rule has been selected.
Explain == /\ ~done
           /\ output' = IF Lookup # NoneName /\ ~PreferChild THEN Lookup ELSE Child
           /\ done' = TRUE
\* Explain is the only action.
Next == Explain
\* The system may stutter.
Spec == Init /\ [][Next]_vars
\* The recorded name is the direct lookup when one exists, else the child name.
INV_NameMatches == done => output = (IF Lookup # NoneName THEN Lookup ELSE Child)
=============================================================================
