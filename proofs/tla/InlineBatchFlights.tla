---- MODULE InlineBatchFlights ----
\* The story, in plain words. One inline call to the inputs service holds a
\* group of keys bigger than one request carries (a KMS key batch over iroh
\* gossip, #2842), so the group is cut into chunks, one round trip each.
\* Who acts: the calling task, which starts chunks and writes their answers,
\* and each chunk's round trip, which waits for a network permit, sends,
\* and retries a failed send under the one retry policy.
\* What can happen: any send may fail; round trips finish in any order.
\* What must never happen:
\*   * more than `Window` chunks of the group in progress at once
\*     (`flights_per_group`, CHUNKS_IN_FLIGHT for KMS);
\*   * more than `Permits` round trips on the wire at once (`max_in_flight`);
\*   * a chunk answered with an error while its retries are not spent;
\*   * one chunk's failure overwriting another chunk's answer: each chunk's
\*     keys get that chunk's own outcome, and nothing fails the batch whole;
\*   * a chunk left unanswered (every chunk is answered in the end).
\*
\* Rust anchors (crates/inputs/src/service.rs): `Inner::inline` builds one
\* future per `members.chunks(max_batch)` and runs them through
\* `buffer_unordered(flights_per_group)` (Start, Yield); each future takes an
\* `in_flight` permit (Acquire) around `Inner::round_trip`, whose loop retries
\* a failed send (Fail, Succeed); `Inner::answers` turns the outcome into one
\* answer per key of that chunk only (Yield). `DefraKms::with_inputs`
\* (crates/kms/src/defra_kms.rs) sets `max_batch` and `flights_per_group`.
EXTENDS Naturals, FiniteSets

\* Chunks: how many chunks the group was cut into (numbered 1..Chunks).
\* Window: flights_per_group, the most chunks the stream runs at once.
\* Permits: max_in_flight, the most round trips on the wire at once.
\* MaxAttempts: how many sends the retry policy allows one round trip.
\* ObeyWindow, UsePermit, Retry, OwnAnswers: each guard, TRUE in GREEN;
\* each RED turns one of them off.
CONSTANTS Chunks, Window, Permits, MaxAttempts,
          ObeyWindow, UsePermit, Retry, OwnAnswers

\* phase: where each chunk is: "queued" (not started), "waiting" (started,
\*   no permit yet), "wire" (permit held, sending), "finished" (outcome in,
\*   not yet read by the caller), "answered" (its keys have answers).
\* attempts: sends made by each chunk's round trip so far.
\* result: each chunk's own outcome: "none" until finished, then "ok"/"err".
\* answer: what the caller wrote for each chunk's keys.
VARIABLES phase, attempts, result, answer
\* All the state, for stuttering steps.
vars == <<phase, attempts, result, answer>>

\* The chunk numbers.
C == 1..Chunks
\* The chunks that are in phase p.
In(p) == {c \in C : phase[c] = p}
\* The chunks the stream holds: started and not yet read by the caller.
Buffered == In("waiting") \cup In("wire") \cup In("finished")

\* At the start every chunk is queued, nothing sent, nothing answered.
Init == /\ phase = [c \in C |-> "queued"]
        /\ attempts = [c \in C |-> 0]
        /\ result = [c \in C |-> "none"]
        /\ answer = [c \in C |-> "none"]

\* The stream starts the next chunk, in chunk order, while it has room.
Start(c) == /\ phase[c] = "queued"
            \* `stream::iter` hands chunks over in order.
            /\ \A d \in C : d < c => phase[d] /= "queued"
            \* The guard: buffer_unordered holds at most Window futures.
            /\ ObeyWindow => Cardinality(Buffered) < Window
            /\ phase' = [phase EXCEPT ![c] = "waiting"]
            /\ UNCHANGED <<attempts, result, answer>>

\* A started chunk takes an in_flight permit and makes its first send.
Acquire(c) == /\ phase[c] = "waiting"
              \* The guard: the semaphore has a permit free.
              /\ UsePermit => Cardinality(In("wire")) < Permits
              /\ phase' = [phase EXCEPT ![c] = "wire"]
              /\ attempts' = [attempts EXCEPT ![c] = 1]
              /\ UNCHANGED <<result, answer>>

\* The send succeeds: the chunk's outcome is ok and its permit is dropped.
Succeed(c) == /\ phase[c] = "wire"
              /\ phase' = [phase EXCEPT ![c] = "finished"]
              /\ result' = [result EXCEPT ![c] = "ok"]
              /\ UNCHANGED <<attempts, answer>>

\* The send fails: retried while attempts remain, else the outcome is err.
Fail(c) == /\ phase[c] = "wire"
           \* The guard: a failure with attempts left sends again.
           /\ IF Retry /\ attempts[c] < MaxAttempts
                 \* Send again, still holding the permit.
                 THEN /\ attempts' = [attempts EXCEPT ![c] = @ + 1]
                      /\ UNCHANGED <<phase, result>>
                 \* Out of attempts: the round trip ends with an error.
                 ELSE /\ phase' = [phase EXCEPT ![c] = "finished"]
                      /\ result' = [result EXCEPT ![c] = "err"]
                      /\ UNCHANGED attempts
           /\ UNCHANGED answer

\* The caller reads a finished chunk and answers its keys, freeing a slot.
Yield(c) == /\ phase[c] = "finished"
            /\ phase' = [phase EXCEPT ![c] = "answered"]
            \* The guard: only this chunk's keys get this chunk's outcome.
            /\ answer' = IF OwnAnswers \/ result[c] = "ok"
                            \* Its own keys, its own outcome.
                            THEN [answer EXCEPT ![c] = result[c]]
                            \* The planted bug: one error answers every chunk.
                            ELSE [d \in C |-> IF phase'[d] = "answered"
                                              THEN "err" ELSE answer[d]]
            /\ UNCHANGED <<attempts, result>>

\* Every chunk is answered: the call returns and nothing more happens.
Done == /\ \A c \in C : phase[c] = "answered"
        /\ UNCHANGED vars

\* One step: some chunk moves, or the call is over.
Next == \/ \E c \in C : Start(c) \/ Acquire(c) \/ Succeed(c) \/ Fail(c) \/ Yield(c)
        \/ Done

\* The behaviours: start at Init, take steps, and never stall while a step
\* is possible (an awaited future is always polled again).
Spec == Init /\ [][Next]_vars /\ WF_vars(Next)

\* Promise: at most Window chunks of the group are in progress at once.
WindowBound == Cardinality(Buffered) <= Window
\* Promise: at most Permits round trips are on the wire at once.
PermitBound == Cardinality(In("wire")) <= Permits
\* Promise: a chunk ends in an error only after every allowed send failed.
RetriedBeforeError == \A c \in C : result[c] = "err" => attempts[c] = MaxAttempts
\* Promise: every answered chunk carries its own outcome, whatever the
\* other chunks did, so one failed chunk never fails the others.
OwnOutcome == \A c \in C : phase[c] = "answered" => answer[c] = result[c]
\* Promise: every chunk of the group is answered in the end.
AllAnswered == <>(\A c \in C : phase[c] = "answered")
====
