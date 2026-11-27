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
\* already holds the object. A held one is cleared and not sent: one held for
\* its creator is registered already, and one held for someone else (a
\* replicated document another node registered first) keeps that owner, the
\* record yielding to it (crates/engine/acp/src/external/replica.rs
\* yield_replica_if_held_elsewhere). The rest go as one ordered list per
\* creator (send_group), which the provider applies in order, all or none,
\* under that creator's token
\* (crates/engine/vera/src/vera_rs/provider/registration_list.rs calldata,
\* vera.rs `bearerPolicyCmds`). A list the provider refuses applied nothing;
\* its members are then sent one at a time, first to last
\* (crates/engine/vera/src/dac/registration_batch.rs register_listed),
\* so each is answered as it would be alone. An answer can be lost on the
\* way: the provider may or may not have applied the request, the records
\* stay, and a later pass settles them.
\*
\* The settler is not alone. Another one (an access decision or a
\* relationship change settling the object it is about, a redrive at start,
\* another commit's hook: settle_if_owed in crates/engine/acp/src/external.rs)
\* may register an object of the list between the settler asking and its
\* request landing. Nothing serializes the two, so the list may then carry
\* an object the provider holds. The provider never registers an object
\* twice: it refuses the list, and that object alone when it is sent alone.
\* Before such a refusal is reported the provider is asked again
\* (crates/engine/acp/src/external/registration.rs held_after_refusal): an
\* object now held is cleared as held, not reported as failed.
\*
\* What the provider does with one registration: it refuses an object it
\* holds already, and an object its policy refuses (a "bad" one here). Only
\* a bad one's record is parked as refused; a refusal for any other reason
\* is reported as failed and leaves the record owed, to be asked again.
\*
\* What must never happen:
\*   1. a record is cleared while the provider holds its object for nobody
\*      - INV_ClearedOnlyHeld;
\*   2. this node registers an object to someone other than its creator
\*      - INV_OwnerIsCreator;
\*   3. a record is parked as refused although the provider would take it
\*      alone: another member of its list was the refused one
\*      - INV_RefusedOnlyIfBad;
\*   4. a request carries an object the settler itself found held when it
\*      asked, as a resend after a lost answer would - INV_NoResendOfKnownHeld.
\*      An object another settler registers after the asking can still be
\*      carried: nothing short of a lock between settlers prevents it, and
\*      the provider refuses it;
\*   5. a registration is reported as failed while the provider holds its
\*      object for its creator - INV_HeldNotFailed;
\*   6. a registration sent alone after a refused list goes before an
\*      earlier member of that list was answered - INV_Order.
\*
\* The Rust code each guard models:
\*   Recheck        registration_batch.rs to_send, which asks the provider
\*                  (is_doc_registered) before anything is sent;
\*   AskOnRefusal   registration.rs held_after_refusal, which asks again
\*                  before a refusal is reported;
\*   Fallback       dac/registration_batch.rs register_listed, which
\*                  sends a refused list's members one at a time;
\*   PerCreator     registration_batch.rs Sender, which keeps one creator and
\*                  one credential per list;
\*   InOrder        the one-at-a-time sends, first to last.

EXTENDS Naturals, Sequences, FiniteSets

\* Obj:       the documents one transaction created, in their order.
\* ByB:       the documents creator b created; creator a created the rest.
\* Bad:       the objects the provider's policy refuses to register.
\* Elsewhere: the objects another node registered first, to its own owner.
\* MaxLoss:   how many answers may be lost in one behaviour.
\* MaxOther:  how many registrations the other settler makes.
\* Recheck, AskOnRefusal, Fallback, PerCreator, InOrder: the guards; a RED
\* configuration turns one off.
CONSTANTS Obj, ByB, Bad, Elsewhere, MaxLoss, MaxOther,
          Recheck, AskOnRefusal, Fallback, PerCreator, InOrder

\* Who created each document.
CreatorOf == [o \in Obj |-> IF o \in ByB THEN "b" ELSE "a"]

\* Who may own an object: a creator, another node's owner, or nobody yet.
Creators == {CreatorOf[o] : o \in Obj}
Other == "other"
Nobody == "nobody"
Owners == Creators \cup {Other, Nobody}

\* held:     who the provider holds each object for.
\* record:   each record: "owed", "failed" (owed, its last answer an
\*           error), "cleared" or "refused".
\* pc:       the settler: "idle" between passes, "chosen" once it asked and
\*           picked a list, "await" for that list's answer, "single" while
\*           it sends a refused list's members alone.
\* list:     the members of the list chosen or in flight, in order.
\* token:    whose credential the list goes under.
\* known:    the owed objects the provider held when the settler last asked.
\* left:     the members of a refused list not yet sent alone.
\* losses:   answers lost so far.
\* others:   registrations the other settler made so far.
\* resent:   a request carried an object the settler found held.
\* misheld:  a registration was reported failed while held for its creator.
\* misowned: this node registered an object to someone not its creator.
\* skipped:  a member went alone before an earlier one was answered.
VARIABLES held, record, pc, list, token, known, left, losses, others,
          resent, misheld, misowned, skipped

vars == <<held, record, pc, list, token, known, left, losses, others,
          resent, misheld, misowned, skipped>>

\* Every value each variable may take.
TypeOK ==
    /\ held \in [Obj -> Owners]                                        \* an owner or nobody
    /\ record \in [Obj -> {"owed", "failed", "cleared", "refused"}]    \* where each stands
    /\ pc \in {"idle", "chosen", "await", "single"}                    \* a settler step
    /\ list \in Seq(Obj)                                               \* members chosen
    /\ token \in Creators \cup {Nobody}                                \* the list's credential
    /\ known \in SUBSET Obj                                            \* found held when asked
    /\ left \in Seq(Obj)                                               \* still to send alone
    /\ losses \in 0..MaxLoss                                           \* answers lost so far
    /\ others \in 0..MaxOther                                          \* the other settler's sends
    /\ resent \in BOOLEAN                                              \* a known-held one was sent
    /\ misheld \in BOOLEAN                                             \* a held one reported failed
    /\ misowned \in BOOLEAN                                            \* registered to a stranger
    /\ skipped \in BOOLEAN                                             \* a member jumped the line

\* A record still to settle.
Owed(o) == record[o] \in {"owed", "failed"}

\* The objects of a sequence, as a set.
Members(s) == {s[i] : i \in 1..Len(s)}

\* The objects in order, as one sequence (Obj is 1..N).
Ordered(set) == LET n == Cardinality(set)
                IN [i \in 1..n |-> CHOOSE o \in set :
                        Cardinality({p \in set : p < o}) = i - 1]

\* Whether the provider takes a registration of object o: it holds no owner
\* for it and its policy does not refuse it.
Takes(o) == held[o] = Nobody /\ o \notin Bad

\* After the commit: other nodes' objects held for them, the rest for nobody,
\* every record owed, both settlers idle.
Init ==
    /\ held = [o \in Obj |-> IF o \in Elsewhere THEN Other ELSE Nobody] \* what is held
    /\ record = [o \in Obj |-> "owed"]   \* every registration is owed
    /\ pc = "idle"                       \* no pass running
    /\ list = <<>>                       \* nothing chosen
    /\ token = Nobody                    \* no credential in use
    /\ known = {}                        \* nothing asked yet
    /\ left = <<>>                       \* nothing to send alone
    /\ losses = 0                        \* no answer lost
    /\ others = 0                        \* the other settler sent nothing
    /\ resent = FALSE                    \* nothing known-held was sent
    /\ misheld = FALSE                   \* nothing held was reported failed
    /\ misowned = FALSE                  \* nothing registered to a stranger
    /\ skipped = FALSE                   \* nothing out of order

\* The other settler registers one owed object alone, to its creator, and
\* clears its record: an access decision settling the object it is about.
OtherSettles ==
    /\ others < MaxOther                                     \* it may act again
    /\ \E o \in Obj :
          /\ Owed(o) /\ Takes(o)                             \* owed, and the provider takes it
          /\ held' = [held EXCEPT ![o] = CreatorOf[o]]        \* registered to its creator
          /\ record' = [record EXCEPT ![o] = "cleared"]       \* and cleared
    /\ others' = others + 1                                  \* one more of its sends
    /\ UNCHANGED <<pc, list, token, known, left, losses, resent, misheld,
                   misowned, skipped>>

\* A pass begins. With Recheck the settler asks the provider about every
\* owed object, clears each one held (for its creator, or for another node's
\* owner, yielding to it) and remembers them. It picks a creator with owed
\* records left and chooses their list; without PerCreator every owed record
\* goes in that one list.
Choose ==
    /\ pc = "idle"                                                  \* between passes
    /\ \E o \in Obj : Owed(o)                                       \* something is owed
    /\ LET found == {o \in Obj : Owed(o) /\ Recheck /\ held[o] # Nobody} \* held already
           owed == {o \in Obj : Owed(o)} \ found                     \* still to send
       IN /\ record' = [o \in Obj |-> IF o \in found THEN "cleared" ELSE record[o]]
          /\ known' = {o \in Obj : Owed(o) /\ held[o] # Nobody}      \* held when it asked
          /\ IF owed = {}
             THEN UNCHANGED <<pc, list, token>>                      \* nothing to send
             ELSE \E c \in {CreatorOf[o] : o \in owed} :             \* one creator's turn
                    /\ list' = Ordered(IF PerCreator
                                         THEN {o \in owed : CreatorOf[o] = c}
                                         ELSE owed)                  \* in creation order
                    /\ token' = c                                    \* under c's credential
                    /\ pc' = "chosen"                                \* about to send
    /\ UNCHANGED <<held, left, losses, others, resent, misheld, misowned, skipped>>

\* The chosen list is sent. The other settler may have acted since the
\* settler asked.
Send ==
    /\ pc = "chosen"                                                \* a list is chosen
    /\ resent' = (resent \/ \E o \in Members(list) : o \in known)   \* carries one found held
    /\ pc' = "await"                                                \* wait for the answer
    /\ UNCHANGED <<held, record, list, token, known, left, losses, others,
                   misheld, misowned, skipped>>

\* What the settler reports for a refusal of object o: with AskOnRefusal it
\* asks the provider again, and a held object is cleared as held; otherwise,
\* or when it is not held, a bad object is parked and any other failed.
Answer(o) ==
    IF AskOnRefusal /\ held[o] # Nobody THEN "cleared"   \* held after all
    ELSE IF o \in Bad THEN "refused"                     \* its policy refuses it
    ELSE "failed"                                        \* reported as an error

\* The provider applies the list: every member takes, so each is held for
\* the list's credential, and the settler clears every record.
Applied ==
    /\ pc = "await"                                                 \* a list in flight
    /\ \A o \in Members(list) : Takes(o)                            \* all take
    /\ held' = [o \in Obj |-> IF o \in Members(list) THEN token ELSE held[o]]
    /\ record' = [o \in Obj |-> IF o \in Members(list) THEN "cleared" ELSE record[o]]
    /\ misowned' = (misowned \/ \E o \in Members(list) : token # CreatorOf[o])
    /\ pc' = "idle"                                                 \* the pass ends
    /\ list' = <<>>                                                 \* nothing in flight
    /\ token' = Nobody                                              \* credential released
    /\ UNCHANGED <<known, left, losses, others, resent, misheld, skipped>>

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
    /\ UNCHANGED <<held, known, losses, others, resent, misheld, misowned, skipped>>

\* The answer is lost: the provider may have applied the list or not. The
\* records stay owed for a later pass.
Lost ==
    /\ pc = "await"                                                 \* a list in flight
    /\ losses < MaxLoss                                             \* room for a loss
    /\ \E applied \in BOOLEAN :                                     \* applied or not
          /\ held' = IF applied /\ \A o \in Members(list) : Takes(o)
                       THEN [o \in Obj |-> IF o \in Members(list) THEN token ELSE held[o]]
                       ELSE held
          /\ misowned' = (misowned \/ (applied /\ (\A o \in Members(list) : Takes(o))
                            /\ \E m \in Members(list) : token # CreatorOf[m]))
    /\ losses' = losses + 1                                         \* one more lost
    /\ pc' = "idle"                                                 \* the pass ends
    /\ list' = <<>>                                                 \* nothing in flight
    /\ token' = Nobody                                              \* credential released
    /\ UNCHANGED <<record, known, left, others, resent, misheld, skipped>>

\* One member of a refused list goes alone, under its own creator: the
\* first one left, or with InOrder off any of them. The provider takes it,
\* or refuses it and the settler reports the refusal (Answer).
Single ==
    /\ pc = "single"                                                \* sending alone
    /\ left # <<>>                                                  \* members remain
    /\ \E i \in IF InOrder THEN {1} ELSE 1..Len(left) :             \* which one goes
          LET o == left[i]                                          \* that member
              answer == IF Takes(o) THEN "cleared" ELSE Answer(o)   \* what comes of it
          IN /\ skipped' = (skipped \/ i > 1)                       \* jumped the line
             /\ resent' = (resent \/ o \in known)                   \* found held, sent
             /\ misheld' = (misheld \/ (answer = "failed" /\ held[o] = CreatorOf[o]))
             /\ held' = IF Takes(o)
                          THEN [held EXCEPT ![o] = CreatorOf[o]]
                          ELSE held                                 \* taken, or not
             /\ record' = [record EXCEPT ![o] = answer]             \* its own answer
             /\ left' = [j \in 1..Len(left) - 1 |->
                           IF j < i THEN left[j] ELSE left[j + 1]]  \* it has gone
    /\ pc' = IF Len(left) = 1 THEN "idle" ELSE "single"             \* last one ends it
    /\ UNCHANGED <<list, token, known, losses, others, misowned>>

\* Every record is settled or the other settler is done: nothing is left to
\* do, and the story rests.
Done ==
    /\ pc = "idle"                               \* no pass running
    /\ \A o \in Obj : ~Owed(o)                   \* nothing owed
    /\ UNCHANGED vars                            \* it stays so

\* Any one of the steps above.
Next == OtherSettles \/ Choose \/ Send \/ Applied \/ Refused \/ Lost \/ Single \/ Done

\* Start, then take steps.
Spec == Init /\ [][Next]_vars

\* A cleared record's object is held for someone: its creator, or the owner
\* another node registered it to first.
INV_ClearedOnlyHeld == \A o \in Obj : record[o] = "cleared" => held[o] # Nobody

\* This node registers an object only to its creator.
INV_OwnerIsCreator == ~misowned

\* Only a registration the provider refuses alone is parked as refused.
INV_RefusedOnlyIfBad == \A o \in Obj : record[o] = "refused" => o \in Bad

\* No request carries an object the settler found held when it asked.
INV_NoResendOfKnownHeld == ~resent

\* No registration is reported failed while held for its creator.
INV_HeldNotFailed == ~misheld

\* The members of a refused list go alone first to last.
INV_Order == ~skipped

====
