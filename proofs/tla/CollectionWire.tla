--------------------------- MODULE CollectionWire ---------------------------
\* A browser reads one stored vector index and an integer default.
\* It should receive only the chosen block and a number even for a large default.
\* Rust: collection_js.rs, IndexJs::from and collections_to_js.
\* Inject either old behavior to check that its corresponding promise fails.
CONSTANTS CopyOtherBlocks, RejectWideIntegers
\* The selected algorithm, stored blocks, kind of default, completion flag, and returned object.
VARIABLES selected, blocks, wide, done, response

\* Try both kinds of default and every subset of the supported parameter blocks.
Init ==
    \* Flat has no parameter block. The other two stand for algorithms with blocks.
    /\ selected \in {"Flat", "Hnsw", "Ssg"}
    \* The store can carry any combination of blocks.
    /\ blocks \in SUBSET {"Hnsw", "Ssg"}
    \* The default may be within or outside the browser's safe integer range.
    /\ wide \in BOOLEAN
    \* There is no result to inspect until the request runs.
    /\ done = FALSE
    /\ response = [blocks |-> {}, number |-> FALSE]

\* One request projects the stored blocks and parses the JSON number.
Read ==
    \* Mark the response ready. A repeated read of the same inputs has the same output.
    /\ done' = TRUE
    \* Pick the matching block. JSON parsing accepts all the integer tokens.
    /\ response' = [blocks |-> IF CopyOtherBlocks THEN blocks ELSE blocks \cap {selected},
                    number |-> ~(wide /\ RejectWideIntegers)]
    \* Reading the response leaves the stored description alone.
    /\ UNCHANGED <<selected, blocks, wide>>

\* Reading is the only action. The state can also stay still.
Next == Read
Spec == Init /\ [][Next]_<<selected, blocks, wide, done, response>>

\* Once ready, the response contains the selected block exactly when the store had it.
SelectedBlockOnly ==
    IF done THEN response.blocks = blocks \cap {selected} ELSE TRUE

\* Every stored integer default yields a number.
DefaultIsNumber ==
    IF done THEN response.number ELSE TRUE
=============================================================================
