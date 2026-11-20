---- MODULE RegistrationBatch ----
\* The story, in plain words.
\*
\* A transaction creates several documents under a collection that an
\* external ACP provider (vera.rs) guards. Each create writes, in its own
\* transaction, a record that the provider is owed a registration: the
\* creator becomes the document's owner. After the commit the node settles
\* those records (crates/engine/acp/src/commit_registrations.rs settle). It
\* used to send each registration as its own request, and each request costs
\* the provider a block.
\*
\* Now the settler (crates/engine/acp/src/external/registration_batch.rs
\* settle_all_if_owed) first asks the provider, for each record, whether it
\* already holds the object: a held one is cleared and not sent again. The
\* rest go as one ordered list per creator (send_group), which the provider
\* applies in order, all or none, under that creator's token
\* (crates/engine/vera/src/vera_rs/provider/registration_list.rs calldata,
\* vera.rs `bearerPolicyCmds`). A list the provider refuses applied nothing;
\* its members are then sent one at a time, first to last
\* (crates/engine/vera/src/cosmos/dac/registration_batch.rs register_listed),
\* so each is answered as it would be alone. An answer can be lost on the
\* way: the provider may or may not have applied the request, the records
\* stay, and a later pass settles them.
\*
\* What the provider does with one registration: it refuses an object it
\* holds already, and an object its policy refuses (a "bad" one here). Only
\* a bad one's record is parked as refused; a refusal for any other reason
\* leaves the record owed, to be asked about again.
\*
\* What must never happen:
\*   1. a record is cleared while the provider does not hold its object for
\*      its creator - INV_ClearedOnlyHeld;
\*   2. an object ends up owned by someone other than its creator
\*      - INV_OwnerIsCreator;
\*   3. a record is parked as refused although the provider would take it
\*      alone: another member of its list was the refused one
\*      - INV_RefusedOnlyIfBad;
\*   4. a request carries an object the provider already holds for its
\*      creator: a resend after a lost answer, which the provider refuses,
\*      taking the rest of its list with it - INV_NoResendOfHeld;
\*   5. a registration sent alone after a refused list goes before an
\*      earlier member of that list was answered - INV_Order.
\*
\* The Rust code each guard models:
\*   Recheck        registration_batch.rs to_send, which asks the provider
\*                  (is_doc_registered) before anything is sent;
\*   Fallback       cosmos/dac/registration_batch.rs register_listed, which
\*                  sends a refused list's members one at a time;
\*   PerCreator     registration_batch.rs Sender, which keeps one creator and
\*                  one credential per list;
\*   InOrder        the one-at-a-time sends, first to last.

EXTENDS Naturals, Sequences, FiniteSets

\* Obj:       the documents one transaction created, in their order.
\* ByB:       the documents creator b created; creator a created the rest.
\* Bad:       the objects the provider's policy refuses to register.
\* MaxLoss:   how many answers may be lost in one behaviour.
\* Recheck, Fallback, PerCreator, InOrder: the guards; a RED configuration
\* turns one off.
CONSTANTS Obj, ByB, Bad, MaxLoss, Recheck, Fallback, PerCreator, InOrder

\* Who created each document.
CreatorOf == [o \in Obj |-> IF o \in ByB THEN "b" ELSE "a"]

\* Who may own an object: a creator, or nobody yet.
Creators == {CreatorOf[o] : o \in Obj}
Nobody == "nobody"

\* held:     who the provider holds each object for.
\* record:   each record: "owed", "cleared" or "refused".
\* pc:       the settler: "idle" between passes, "await" for a list's
\*           answer, "single" while it sends a refused list's members alone.
\* list:     the members of the list in flight, in order.
\* token:    whose credential the list in flight goes under.
\* left:     the members of a refused list not yet sent alone.
\* losses:   answers lost so far.
\* resent:   a request carried an object already held for its creator.
\* skipped:  a member went alone before an earlier one was answered.
VARIABLES held, record, pc, list, token, left, losses, resent, skipped

vars == <<held, record, pc, list, token, left, losses, resent, skipped>>

\* Every value each variable may take.
TypeOK ==
    /\ held \in [Obj -> Creators \cup {Nobody}]     \* an owner or nobody
    /\ record \in [Obj -> {"owed", "cleared", "refused"}] \* where each record stands
    /\ pc \in {"idle", "await", "single"}           \* a settler step
    /\ list \in Seq(Obj)                            \* members in flight
    /\ token \in Creators \cup {Nobody}             \* the list's credential
    /\ left \in Seq(Obj)                            \* members still to send alone
    /\ losses \in 0..MaxLoss                        \* answers lost so far
    /\ resent \in BOOLEAN                           \* a held object was sent
    /\ skipped \in BOOLEAN                          \* a member jumped the line

\* The objects of a sequence, as a set.
Members(s) == {s[i] : i \in 1..Len(s)}

\* The objects in order, as one sequence (Obj is 1..N).
Ordered(set) == LET n == Cardinality(set)
                IN [i \in 1..n |-> CHOOSE o \in set :
                        Cardinality({p \in set : p < o}) = i - 1]

\* Whether the provider takes object o for creator c: it holds no owner for
\* it and its policy does not refuse it.
Takes(o) == held[o] = Nobody /\ o \notin Bad

\* After the commit: nothing held, every record owed, the settler idle.
Init ==
    /\ held = [o \in Obj |-> Nobody]     \* the provider holds none
    /\ record = [o \in Obj |-> "owed"]   \* every registration is owed
    /\ pc = "idle"                       \* no pass running
    /\ list = <<>>                       \* nothing in flight
    /\ token = Nobody                    \* no credential in use
    /\ left = <<>>                       \* nothing to send alone
    /\ losses = 0                        \* no answer lost
    /\ resent = FALSE                    \* nothing sent twice
    /\ skipped = FALSE                   \* nothing out of order

\* A pass begins. With Recheck every owed object the provider holds for its
\* creator is cleared without a request. The first creator with owed
\* records left sends them as one list under its credential; without
\* PerCreator every owed record goes in that one list.
Send ==
    /\ pc = "idle"                                                  \* between passes
    /\ \E o \in Obj : record[o] = "owed"                           \* something is owed
    /\ LET cleared == {o \in Obj : record[o] = "owed" /\ Recheck
                                   /\ held[o] = CreatorOf[o]}       \* held already
           owed == {o \in Obj : record[o] = "owed"} \ cleared       \* still to send
       IN IF owed = {}
          THEN /\ record' = [o \in Obj |-> IF o \in cleared THEN "cleared" ELSE record[o]]
               /\ UNCHANGED <<held, pc, list, token, left, losses, resent, skipped>>
          ELSE \E c \in {CreatorOf[o] : o \in owed} :              \* one creator's turn
                 LET group == IF PerCreator
                                THEN {o \in owed : CreatorOf[o] = c}  \* that creator's
                                ELSE owed                             \* everyone's
                 IN /\ record' = [o \in Obj |-> IF o \in cleared THEN "cleared"
                                                ELSE record[o]]       \* clear the held
                    /\ list' = Ordered(group)                         \* in creation order
                    /\ token' = c                                     \* under c's credential
                    /\ resent' = (resent \/ \E o \in group : held[o] = CreatorOf[o])
                    /\ pc' = "await"                                  \* wait for the answer
                    /\ UNCHANGED <<held, left, losses, skipped>>

\* The provider applies the list: every member takes, so each is held for
\* the list's credential, and the settler clears every record.
Applied ==
    /\ pc = "await"                                                 \* a list in flight
    /\ \A o \in Members(list) : Takes(o)                            \* all take
    /\ held' = [o \in Obj |-> IF o \in Members(list) THEN token ELSE held[o]]
    /\ record' = [o \in Obj |-> IF o \in Members(list) THEN "cleared" ELSE record[o]]
    /\ pc' = "idle"                                                 \* the pass ends
    /\ list' = <<>>                                                 \* nothing in flight
    /\ token' = Nobody                                              \* credential released
    /\ UNCHANGED <<left, losses, resent, skipped>>

\* The provider refuses the list: one member does not take, and nothing
\* applies. With Fallback the members go alone; without it each is parked
\* as refused.
Refused ==
    /\ pc = "await"                                                 \* a list in flight
    /\ \E o \in Members(list) : ~Takes(o)                           \* one refuses
    /\ IF Fallback
       THEN /\ left' = list                                         \* each goes alone
            /\ pc' = "single"                                       \* one at a time
            /\ UNCHANGED record
       ELSE /\ record' = [o \in Obj |-> IF o \in Members(list) THEN "refused"
                                         ELSE record[o]]            \* all parked
            /\ pc' = "idle"                                         \* the pass ends
            /\ UNCHANGED left
    /\ list' = <<>>                                                 \* nothing in flight
    /\ token' = Nobody                                              \* credential released
    /\ UNCHANGED <<held, losses, resent, skipped>>

\* The answer is lost: the provider may have applied the list or not. The
\* records stay owed for a later pass.
Lost ==
    /\ pc = "await"                                                 \* a list in flight
    /\ losses < MaxLoss                                             \* room for a loss
    /\ \E applied \in BOOLEAN :                                     \* applied or not
          held' = IF applied /\ \A o \in Members(list) : Takes(o)
                    THEN [o \in Obj |-> IF o \in Members(list) THEN token ELSE held[o]]
                    ELSE held
    /\ losses' = losses + 1                                         \* one more lost
    /\ pc' = "idle"                                                 \* the pass ends
    /\ list' = <<>>                                                 \* nothing in flight
    /\ token' = Nobody                                              \* credential released
    /\ UNCHANGED <<record, left, resent, skipped>>

\* One member of a refused list goes alone, under its own creator: the
\* first one left, or with InOrder off any of them. The provider takes it,
\* or parks it as refused when its policy refuses it, or leaves it owed
\* when it refuses for another reason.
Single ==
    /\ pc = "single"                                                \* sending alone
    /\ left # <<>>                                                  \* members remain
    /\ \E i \in IF InOrder THEN {1} ELSE 1..Len(left) :             \* which one goes
          LET o == left[i]
          IN /\ skipped' = (skipped \/ i > 1)                       \* jumped the line
             /\ resent' = (resent \/ held[o] = CreatorOf[o])        \* held already
             /\ held' = IF Takes(o)
                          THEN [held EXCEPT ![o] = CreatorOf[o]]
                          ELSE held                                 \* taken, or not
             /\ record' = [record EXCEPT ![o] =
                             IF Takes(o) THEN "cleared"
                             ELSE IF o \in Bad THEN "refused"
                             ELSE "owed"]                           \* its own answer
             /\ left' = [j \in 1..Len(left) - 1 |->
                           IF j < i THEN left[j] ELSE left[j + 1]]  \* it has gone
    /\ pc' = IF Len(left) = 1 THEN "idle" ELSE "single"             \* last one ends it
    /\ UNCHANGED <<list, token, losses>>

\* Every record is settled: nothing is left to do, and the story rests.
Done ==
    /\ pc = "idle"                              \* no pass running
    /\ \A o \in Obj : record[o] # "owed"         \* nothing owed
    /\ UNCHANGED vars                           \* it stays so

\* Any one of the steps above.
Next == Send \/ Applied \/ Refused \/ Lost \/ Single \/ Done

\* Start, then take steps.
Spec == Init /\ [][Next]_vars

\* A cleared record's object is held for its creator.
INV_ClearedOnlyHeld == \A o \in Obj : record[o] = "cleared" => held[o] = CreatorOf[o]

\* Whoever holds an object is its creator.
INV_OwnerIsCreator == \A o \in Obj : held[o] # Nobody => held[o] = CreatorOf[o]

\* Only a registration the provider refuses alone is parked as refused.
INV_RefusedOnlyIfBad == \A o \in Obj : record[o] = "refused" => o \in Bad

\* No request carries an object already held for its creator.
INV_NoResendOfHeld == ~resent

\* The members of a refused list go alone first to last.
INV_Order == ~skipped

====
