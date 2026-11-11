------------------------ MODULE HttpTxnConfiguration ------------------------
\* An embedding process may use its own transaction ownership mode while it
\* is the only caller. An HTTP listener introduces independent callers, so it
\* may start only when the registry keys transactions by caller identity.
EXTENDS Naturals
CONSTANT Bug                            \* RED removes the builder guard.
VARIABLES started, unsafe              \* The resulting node configuration.
vars == <<started, unsafe>>
Init == /\ started = FALSE             \* No node has started.
        /\ unsafe = FALSE              \* No unsafe node has started.
Start(http, embedder) ==
  /\ ~started                          \* Start at most one node.
  /\ LET allowed == ~(http /\ embedder) \/ Bug = "Allow"
     IN /\ started' = allowed          \* Refused configurations do not start.
        /\ unsafe' = (allowed /\ http /\ embedder)
Next == \E http, embedder \in BOOLEAN : Start(http, embedder)
Spec == Init /\ [][Next]_vars          \* Explore every builder choice.
INV_HttpOwnerIsolation == ~unsafe      \* HTTP never uses the embedder key.
=============================================================================
