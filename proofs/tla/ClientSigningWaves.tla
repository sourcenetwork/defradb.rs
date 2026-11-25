---- MODULE ClientSigningWaves ----
\* The story. A client creates a document over the HTTP signing WebSocket.
\* The server builds the blocks and asks the client to sign them in waves:
\* first the field blocks, then the composite block that names them. The
\* server numbers each wave; the client answers a wave by naming its number
\* and sending one signature per block. The client may misbehave: answer a
\* wave twice, answer a wave that was never asked for, or sign some other
\* block. The server may, by a retry, ask again for a block it already had
\* signed. What must never happen: a block signed twice (a second signature
\* would give the document a second ID), a signature accepted for a block the
\* server did not ask for, an answer accepted for a wave that is not the open
\* one, or a wave signed after the session refused an answer.
\* Rust owner: crates/surfaces/http/src/client_signing_waves.rs
\* SigningWaves::open and SigningWaves::answer, driven by
\* crates/surfaces/http/src/client_signing.rs run.
EXTENDS Naturals, FiniteSets, Sequences

\* Each switch is one guard of SigningWaves; a RED configuration turns one off.
\* CheckOnce: open refuses a wave holding a block already signed.
\* CheckWave: answer refuses a wave number that is not the open wave's.
\* CheckGraph: answer refuses a signature that does not verify over its block.
\* RefusalFinal: once the session refused, every later wave is refused too.
CONSTANT CheckOnce, CheckWave, CheckGraph, RefusalFinal

\* "f" is the field block, "c" the composite block, "x" a block outside the graph.
Blocks == {"f", "c", "x"}
\* The waves the server means to ask for, in order: fields, then the composite.
Plan == <<{"f"}, {"c"}>>
\* The most waves a session opens here: the plan plus one retry that asks again.
MaxWaves == 3

\* issued: how many waves the server has opened (the open one's number).
\* open: whether a wave is waiting for its answer.
\* asked: the blocks of the open wave.
\* signed: how many accepted signatures each block has.
\* refused: whether the session has refused an answer or a wave.
\* badWave, badGraph, badAfterRefusal: a history of each violation, kept once seen.
VARIABLES issued, open, asked, signed, refused, badWave, badGraph, badAfterRefusal

\* Every variable, for stuttering steps.
vars == <<issued, open, asked, signed, refused, badWave, badGraph, badAfterRefusal>>

\* A session starts with no wave opened, nothing signed and nothing refused.
Init == /\ issued = 0
        /\ open = FALSE
        /\ asked = {}
        /\ signed = [b \in Blocks |-> 0]
        /\ refused = FALSE
        /\ badWave = FALSE
        /\ badGraph = FALSE
        /\ badAfterRefusal = FALSE

\* The blocks that already carry an accepted signature.
Signed == {b \in Blocks : signed[b] > 0}

\* The server opens its next wave: the next planned one, or (after the plan
\* started) a retry that asks again for the field block.
\* With CheckOnce, a wave holding an already-signed block is refused instead.
\* With RefusalFinal, a refused session opens nothing more.
Open(blocks) ==
  /\ ~open
  /\ issued < MaxWaves
  \* Refusing: the guard sees a signed block or an earlier refusal.
  /\ IF (CheckOnce /\ blocks \cap Signed # {}) \/ (RefusalFinal /\ refused)
       \* The wave is not sent; the session is refused from now on.
       THEN /\ refused' = TRUE
            /\ UNCHANGED <<issued, open, asked>>
       \* The wave is sent to the client with the next number.
       ELSE /\ issued' = issued + 1
            /\ open' = TRUE
            /\ asked' = blocks
            /\ UNCHANGED refused
  /\ UNCHANGED <<signed, badWave, badGraph, badAfterRefusal>>

\* The next wave the server asks for: the planned one, or a retry of the field block.
OpenNext ==
  \/ (issued < Len(Plan) /\ Open(Plan[issued + 1]))
  \/ (issued >= 1 /\ Open({"f"}))

\* Whether SigningWaves accepts an answer naming wave n whose signatures
\* verify over the blocks in `over`.
Accepts(n, over) ==
  /\ open
  /\ (RefusalFinal => ~refused)
  /\ (CheckWave => n = issued)
  /\ (CheckGraph => over = asked)

\* The client answers wave n with signatures over the blocks `over`; it may
\* lie about either. An accepted answer signs the asked blocks; a refused
\* one closes the wave and refuses the session.
Answer(n, over) ==
  /\ open
  /\ IF Accepts(n, over)
       \* Each asked block gains one accepted signature, and each violation is recorded.
       THEN /\ signed' = [b \in Blocks |-> IF b \in asked THEN signed[b] + 1 ELSE signed[b]]
            /\ badWave' = (badWave \/ n # issued)
            /\ badGraph' = (badGraph \/ over # asked)
            /\ badAfterRefusal' = (badAfterRefusal \/ refused)
            /\ UNCHANGED refused
       \* Nothing is signed; the session is refused.
       ELSE /\ refused' = TRUE
            /\ UNCHANGED <<signed, badWave, badGraph, badAfterRefusal>>
  /\ open' = FALSE
  /\ asked' = {}
  /\ UNCHANGED issued

\* The client sends an answer while no wave is open: re-sent or never asked.
\* The server reads it between waves and refuses the session.
Stray ==
  /\ ~open
  /\ refused' = TRUE
  /\ UNCHANGED <<issued, open, asked, signed, badWave, badGraph, badAfterRefusal>>

\* Any of these can happen next.
Next == \/ OpenNext
        \/ (\E n \in 1..MaxWaves : \E over \in SUBSET Blocks : Answer(n, over))
        \/ Stray

\* The whole behaviour: start, then take steps.
Spec == Init /\ [][Next]_vars

\* Every variable keeps its type.
TypeOK == /\ issued \in 0..MaxWaves
          /\ open \in BOOLEAN
          /\ asked \subseteq Blocks
          /\ signed \in [Blocks -> 0..MaxWaves]
          /\ refused \in BOOLEAN
          /\ badWave \in BOOLEAN
          /\ badGraph \in BOOLEAN
          /\ badAfterRefusal \in BOOLEAN

\* No block is ever signed twice in one session, so its ID never changes.
INV_SignedOnce == \A b \in Blocks : signed[b] <= 1
\* Only an answer that names the open wave is ever accepted.
INV_OpenWave == ~badWave
\* Only signatures over the asked blocks are ever accepted.
INV_InGraph == ~badGraph
\* After the session refuses, no wave is ever signed.
INV_RefusalFinal == ~badAfterRefusal
====
