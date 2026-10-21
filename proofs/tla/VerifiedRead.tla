---- MODULE VerifiedRead ----
\* The story, in plain words.
\*
\* A node asks Vera questions about access: who owns this document, may this
\* reader read it, what does this policy say. It does not trust the server's
\* word: the light client checks every answer against a proof, at the newest
\* chain state it has authenticated (a "state root"). A background loop keeps
\* that newest root current: each time the chain finalizes a block, the root
\* moves on.
\*
\* A question takes a round trip. If a block is finalized while the answer is
\* on its way back, the answer was proven at a root that is no longer the
\* newest one. The light client refuses it ("ACP state changed during
\* verification; retry the request"): the access could have changed in that
\* block. The refusal is not a failed read, only a late one. The same question
\* asked again is answered at the newer root.
\*
\* Example: the node asks "may Bob read doc 7?" at root 4. Block 5 finalizes
\* while the proof travels. The proof says root 4, the newest is 5: refused.
\* Asked again, the proof comes back at root 5, which is still the newest:
\* accepted.
\*
\* The bug this fixes: nobody asked again. A query of a document's commit
\* history turned the refusal into "unable to verify access to commit
\* history", although the reader was allowed and nothing had failed.
\*
\* What must never happen:
\*   * an answer accepted although a newer root existed when it was accepted
\*     (it could grant an access the newer state revokes);
\*   * a read that ends in an error while it still had asks left, only because
\*     a block finalized during one of them;
\*   * a read asked again and again without end.
\* And what must happen: a read ends with an answer whenever the chain
\* finalizes fewer blocks than the read may ask.
\*
\* Code this models (defradb.rs, light client at backbone rev 2720b61):
\*   crates/vera/src/vera_rs/provider/verified.rs   verified, ATTEMPTS, overtaken
\*   crates/vera/src/vera_rs/provider.rs            create_policy, query_policy,
\*       query_object_owner, verify_access, create_access_decision (each read
\*       goes through `verified`)
\*   acp-light-client src/lib.rs                    verify_access, read_object_owner,
\*       read_key (minimum = fresh_state height), verify_access_decision
\*   acp-light-client src/header_sync.rs            run_header_loop (root moves),
\*       HeaderChain::accept_response (the refusal)
\*
\* Correspondence table (model -> Rust):
\*   root, Advance        header_sync.rs run_header_loop / observe_revision: the
\*                        newest authenticated module_state_root moves on
\*   Ask                  lib.rs: `fresh_state()?.height` is the minimum, the
\*                        proof request leaves (one call of `read` in verified)
\*   readAt               the revision the proof server answers at (>= minimum)
\*   Return, CheckRoot    header_sync.rs accept_response: `current.module_state_root
\*                        == root` else the refusal; lib.rs verify_access_decision
\*                        compares the record's root with fresh_state the same way
\*   Retry, attempt       verified.rs `verified`: an `overtaken` error with asks
\*                        left asks again; any other outcome is returned as it is
\*   Capped, Attempts     verified.rs ATTEMPTS: after that many asks the refusal
\*                        is returned to the caller
\*   stale                history variable, no Rust counterpart
\*   overtaken            history variable: refusals in a row for this read
EXTENDS Naturals

CONSTANTS
    Attempts,     \* ATTEMPTS in verified.rs: how many times one read may be asked
    MaxAdvances,  \* how many blocks the chain finalizes in a run (keeps the model finite)
    CheckRoot,    \* TRUE: an answer proven at an older root is refused (accept_response)
    Retry,        \* TRUE: a refused read is asked again while it has asks left (the fix)
    Capped        \* TRUE: a read is asked at most Attempts times

VARIABLES
    root,       \* the newest authenticated state root, a number that only grows
    advances,   \* how many blocks finalized so far in this run
    pc,         \* the read: "idle" (about to be asked), "inflight", "answered" or "failed"
    readAt,     \* the root the answer on its way back was proven at
    attempt,    \* which ask this is, starting at 1
    overtaken,  \* how many of this read's asks were refused because a block finalized
    stale       \* TRUE once an answer was accepted while a newer root existed

vars == <<root, advances, pc, readAt, attempt, overtaken, stale>>

\* Every variable holds a value of the expected kind.
TypeOK ==
    /\ root \in Nat                                         \* roots are numbers
    /\ advances \in 0..MaxAdvances                          \* never more blocks than the bound
    /\ pc \in {"idle", "inflight", "answered", "failed"}    \* the read's four places
    /\ readAt \in Nat                                       \* a root number
    /\ attempt \in Nat \ {0}                                \* asks are counted from 1
    /\ overtaken \in Nat                                    \* a count
    /\ stale \in BOOLEAN                                    \* a flag

\* At the start: root 0, no block yet, the read about to be asked for the first time.
Init ==
    /\ root = 0          \* the light client authenticated its first state
    /\ advances = 0      \* no block finalized yet
    /\ pc = "idle"       \* the read is about to be asked
    /\ readAt = 0        \* no answer yet
    /\ attempt = 1       \* this will be the first ask
    /\ overtaken = 0     \* nothing refused yet
    /\ stale = FALSE     \* nothing accepted yet

\* A block finalizes: the header loop authenticates a newer root. This can
\* happen at any moment, also while a read is on the wire.
Advance ==
    /\ advances < MaxAdvances             \* the run still has a block to finalize
    /\ root' = root + 1                   \* the newest root moves on
    /\ advances' = advances + 1           \* one more block finalized
    /\ UNCHANGED <<pc, readAt, attempt, overtaken, stale>>  \* the read does not notice yet

\* The read is asked. The light client takes the newest root's height as the
\* lowest revision it will accept, and the proof request leaves.
Ask ==
    /\ pc = "idle"         \* the read is about to be asked
    /\ pc' = "inflight"    \* the request is on the wire
    /\ readAt' = root      \* the server cannot answer below the newest root
    /\ UNCHANGED <<root, advances, attempt, overtaken, stale>>  \* nothing else moves

\* The answer comes back and is checked. The server answered at some root
\* between the one asked for and the newest one now (`served`). If that is
\* the newest root, the answer is accepted. If a block finalized in between,
\* the light client refuses it; the read is then asked again while it has
\* asks left, else the refusal is the read's outcome.
Return ==
    /\ pc = "inflight"                                \* an answer is on its way back
    /\ \E served \in readAt..root :                   \* the root the server proved it at
        IF served = root \/ ~CheckRoot                \* newest root (or the check is skipped)
        THEN /\ pc' = "answered"                      \* the read ends with this answer
             /\ stale' = (served # root)              \* remember if a newer root existed
             /\ readAt' = served                      \* the answer's root
             /\ UNCHANGED <<attempt, overtaken>>      \* no more asks
        ELSE /\ overtaken' = overtaken + 1            \* one more refusal in a row
             /\ readAt' = served                      \* the refused answer's root
             /\ stale' = stale                        \* nothing was accepted
             /\ IF Retry /\ (attempt < Attempts \/ ~Capped)  \* asks left (or no cap)
                THEN /\ pc' = "idle"                  \* ask the same read again
                     /\ attempt' = attempt + 1        \* the next ask
                ELSE /\ pc' = "failed"                \* the refusal goes to the caller
                     /\ attempt' = attempt            \* no further ask
    /\ UNCHANGED <<root, advances>>                   \* the chain does not move in this step

\* Once the read ended, nothing more happens to it; blocks may still finalize.
Done ==
    /\ pc \in {"answered", "failed"}   \* the read ended
    /\ UNCHANGED vars                  \* stutter, so a finished run is not a deadlock

Next == Advance \/ Ask \/ Return \/ Done   \* one of these happens at each step

\* The read keeps going: a read that can be asked is asked, and an answer on
\* the wire comes back. Blocks need no fairness: they may stop at any time.
Spec == Init /\ [][Next]_vars /\ WF_vars(Ask) /\ WF_vars(Return)

\* An answer is only ever accepted at the newest authenticated root.
INV_NeverStale == ~stale

\* A read ends in an error only after the chain overtook every one of its asks.
INV_FailsOnlyWhenEveryAskOvertaken ==
    pc = "failed" => overtaken = Attempts

\* A read is never asked more than Attempts times.
INV_AttemptsBounded == attempt <= Attempts

\* When the chain finalizes fewer blocks than the read may ask, the read ends
\* with an answer.
LIVE_Answers == <>(pc = "answered")
====
