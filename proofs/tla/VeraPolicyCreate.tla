---- MODULE VeraPolicyCreate ----
\* The story: callers ask DefraDB to create a policy on vera.rs. vera.rs names
\* a policy by its definition, its creator and a 32-byte nonce the creator
\* picks: id = SHA-256(policy hash, nonce, creator). A second create of the
\* same definition, nonce and creator is refused as already existing, and the
\* policy already there is kept.
\*
\* Who acts:
\*   - each request: a creator (the node's actor), a definition, and either a
\*     nonce its caller supplied or none, in which case the provider draws one
\*     from the operating system's randomness, once for the request;
\*   - the network: a submission may never reach vera.rs, and the answer to
\*     one that did may be lost, so the provider sends the request again;
\*   - the host: a deployment may start with a policy already seeded under a
\*     definition, nonce and creator of its choosing;
\*   - an owner: may edit a policy after it exists, which keeps its id.
\*
\* What can happen: any request, seeded policy, lost submission, lost answer
\* and edit, in any order, up to a bounded number of sends per request.
\*
\* What must never happen:
\*   - one request leaves more than one policy behind (a send again that drew
\*     a new nonce creates a second policy);
\*   - a request hands back an id whose policy is not its own definition and
\*     creator;
\*   - a request is refused although its own policy, as it asked for it, is
\*     there (a lost answer, then "already exists" read as a failure);
\*   - another creator's policy blocks a request (the creator is part of the
\*     id, so two creators never meet on one id).
\*
\* Code it models:
\*   crates/engine/vera/src/vera_rs/provider/create_policy.rs
\*     VeraRsProvider::create_policy_with   the nonce, drawn once; the send;
\*                                          the "already exists" adoption
\*     VeraRsProvider::adopt_existing       the check of the policy found
\*   crates/engine/vera/src/vera_rs/provider/submission.rs
\*     VeraRsProvider::confirm_tx            a send whose refusal is returned
\*   crates/engine/vera/src/cosmos/dac.rs VeraDocumentACP::add_policy
\*   vera.rs vera_client::predict_policy_id  the id itself (abstract here)
EXTENDS Naturals, FiniteSets   \* numbers, and counting the policies a request made

CONSTANTS
  Requests,      \* the requests callers make
  Creators,      \* the actors that create policies
  Defs,          \* the policy definitions
  Nonces,        \* the nonces a caller may supply or the provider may draw
  MaxSends,      \* how many times one request may be sent
  Enforced       \* which guards the code really runs

\* The guards:
\*   "reuse"    a send again carries the nonce the request started with;
\*   "adopt"    "already exists" is answered by reading the policy there;
\*   "check"    the policy read is adopted only if it is the request's own
\*              definition and creator;
\*   "creator"  the creator is part of the id.
ASSUME Enforced \subseteq {"reuse", "adopt", "check", "creator"}
\* A request is sent at least once.
ASSUME MaxSends \in Nat /\ MaxSends >= 1

\* No nonce: the caller left it to the provider.
NoNonce == "none"

\* What a request asks: its creator, its definition and its caller's nonce.
Asks == [creator : Creators, def : Defs, supplied : Nonces \cup {NoNonce}]

\* The id vera.rs gives a definition, nonce and creator. Without the
\* "creator" guard it leaves the creator out.
Id(def, nonce, creator) ==
  IF "creator" \in Enforced
    THEN <<def, nonce, creator>>
    ELSE <<def, nonce, "anyone">>

VARIABLES
  ask,       \* what each request asks
  nonce,     \* the nonce each request is sent with; NoNonce before it starts
  sends,     \* how many times each request was sent
  status,    \* "idle", "sending", "done" or "refused"
  result,    \* the id a finished request handed back, or NoNonce
  seen,      \* the policy a request's answer showed it, or NoNonce
  chain,     \* the policies vera.rs holds: a set of records
  made       \* the ids each request's own sends created

\* Every variable, for saying a step leaves them as they are.
vars == <<ask, nonce, sends, status, result, seen, chain, made>>

\* A record: its id, its current definition, the nonce and creator it was
\* created with, and the definition it was created with.
Record(def, n, creator) ==
  [id |-> Id(def, n, creator), def |-> def, nonce |-> n,
   owner |-> creator, created |-> def]

\* The ids vera.rs holds.
Ids == {rec.id : rec \in chain}

\* Whether `rec` is request `r`'s own policy as it asked for it: its
\* definition as asked, its creator and the nonce it was sent with.
Own(r, rec) ==
  /\ rec.def = ask[r].def
  /\ rec.owner = ask[r].creator
  /\ rec.nonce = nonce[r]

\* Every variable holds a value of the shape it should.
TypeOK ==
  /\ ask \in [Requests -> Asks]
  /\ nonce \in [Requests -> Nonces \cup {NoNonce}]
  /\ sends \in [Requests -> 0..MaxSends]
  /\ status \in [Requests -> {"idle", "sending", "done", "refused"}]
  /\ made \in [Requests -> SUBSET (Defs \X Nonces \X (Creators \cup {"anyone"}))]

\* Any requests can arrive, and the deployment may start with one seeded
\* policy or none.
Init ==
  /\ ask \in [Requests -> Asks]
  /\ nonce = [r \in Requests |-> NoNonce]
  /\ sends = [r \in Requests |-> 0]
  /\ status = [r \in Requests |-> "idle"]
  /\ result = [r \in Requests |-> NoNonce]
  /\ seen = [r \in Requests |-> NoNonce]
  /\ \/ chain = {}
     \/ \E d \in Defs, n \in Nonces, c \in Creators : chain = {Record(d, n, c)}
  /\ made = [r \in Requests |-> {}]

\* A request starts with its caller's nonce, or one the provider draws: any
\* nonce may come out of the draw.
Start(r) ==
  /\ status[r] = "idle"
  /\ status' = [status EXCEPT ![r] = "sending"]
  /\ IF ask[r].supplied # NoNonce
       THEN nonce' = [nonce EXCEPT ![r] = ask[r].supplied]
       ELSE \E n \in Nonces : nonce' = [nonce EXCEPT ![r] = n]
  /\ UNCHANGED <<ask, sends, result, seen, chain, made>>

\* The nonce a send carries: the request's own, or, without the "reuse"
\* guard, a new draw on every send after the first (the bug a RED plants).
SendNonce(r, n) ==
  IF "reuse" \in Enforced \/ sends[r] = 0 \/ ask[r].supplied # NoNonce
    THEN n = nonce[r]
    ELSE n \in Nonces

\* A send that never reached vera.rs: nothing changes but the count.
SendLost(r) ==
  /\ status[r] = "sending"
  /\ sends[r] < MaxSends
  /\ \E n \in Nonces :
       /\ SendNonce(r, n)
       /\ nonce' = [nonce EXCEPT ![r] = n]
  /\ sends' = [sends EXCEPT ![r] = @ + 1]
  /\ UNCHANGED <<ask, status, result, seen, chain, made>>

\* A send that vera.rs executed. It creates the policy unless its id is
\* taken. The answer may be lost, and the request is then sent again. If the
\* answer arrives: a create hands back its id; "already exists" is adopted
\* when the policy there is the request's own (with "check"; anything there
\* without it), and refused otherwise; without "adopt" it is refused.
SendExecuted(r) ==
  /\ status[r] = "sending"
  /\ sends[r] < MaxSends
  /\ \E n \in Nonces :
       /\ SendNonce(r, n)
       /\ nonce' = [nonce EXCEPT ![r] = n]
       /\ LET id == Id(ask[r].def, n, ask[r].creator)
              fresh == id \notin Ids
              after == IF fresh
                         THEN chain \cup {Record(ask[r].def, n, ask[r].creator)}
                         ELSE chain
              rec == CHOOSE x \in after : x.id = id
              mine == /\ rec.def = ask[r].def
                      /\ rec.owner = ask[r].creator
                      /\ rec.nonce = n
              adopted == ("adopt" \in Enforced) /\ (("check" \notin Enforced) \/ mine)
          IN /\ chain' = after
             /\ made' = IF fresh THEN [made EXCEPT ![r] = @ \cup {id}] ELSE made
             /\ sends' = [sends EXCEPT ![r] = @ + 1]
             /\ \/ \* the answer is lost: the request stays to be sent again
                   /\ UNCHANGED <<status, result, seen>>
                \/ \* the answer arrives, showing the policy under the id
                   /\ seen' = [seen EXCEPT ![r] = rec]
                   /\ IF fresh \/ adopted
                        THEN /\ status' = [status EXCEPT ![r] = "done"]
                             /\ result' = [result EXCEPT ![r] = id]
                        ELSE /\ status' = [status EXCEPT ![r] = "refused"]
                             /\ UNCHANGED result
  /\ UNCHANGED ask

\* The owner of a policy edits its definition; the id stays.
Edit ==
  \E rec \in chain, d \in Defs :
    /\ d # rec.def
    /\ chain' = (chain \ {rec}) \cup {[rec EXCEPT !.def = d]}
    /\ UNCHANGED <<ask, nonce, sends, status, result, seen, made>>

\* Each step is one of these.
Next ==
  \/ \E r \in Requests : Start(r) \/ SendLost(r) \/ SendExecuted(r)
  \/ Edit

\* Requests start in Init and then only take these steps.
Spec == Init /\ [][Next]_vars

\* One request leaves at most one policy behind, however often it is sent.
INV_OneRequestOnePolicy == \A r \in Requests : Cardinality(made[r]) <= 1

\* An id handed back names the policy the answer showed, and that policy was
\* the request's own definition, by its creator, under the nonce it was sent
\* with, when it was handed back.
INV_ReturnedIsOwn ==
  \A r \in Requests :
    status[r] = "done" => (seen[r].id = result[r] /\ Own(r, seen[r]))

\* A request is refused only when the policy under its id was not its own as
\* it asked for it: never because its own policy is already there.
INV_NoRefusalOfOwn ==
  \A r \in Requests : status[r] = "refused" => ~Own(r, seen[r])

\* Only the request's own creator can hold the id it is refused under:
\* another creator's policy never blocks it.
INV_NoForeignBlock ==
  \A r \in Requests :
    status[r] = "refused" => seen[r].owner = ask[r].creator
====
