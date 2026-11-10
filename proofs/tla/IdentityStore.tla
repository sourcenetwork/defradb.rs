---- MODULE IdentityStore ----
\* The story, in plain words.
\*
\* A request names the identity that signs it by DID, so the process keeps
\* every signing key in one store, keyed by DID. Several holders may use one
\* DID at once: an FFI identity handle, a node that made or loaded its key, a
\* node that borrows a registered identity. Each holder takes one hold on the
\* DID and gives it back when it goes. The key must stay in the store while
\* any holder still has a hold, and must leave the store (it is a private key)
\* once the last hold is given back, unless the DID is pinned for the life of
\* the process.
\*
\* A key comes with its signer: the key bytes, or a host's callback and the
\* context it is called with (a remote signer). Two holders of one DID may
\* bring different signers: two registrations of one remote identity, each
\* with its own host context. A holder must only ever sign through the signer
\* it brought (or, for a retain, the one it found), because its owner may free
\* that context as soon as its own handle is gone. So while a DID has a key
\* its signer is fixed: a hold that brings another signer is refused, and a
\* pin may replace the signer only when nothing holds the DID.
\*
\* The store keeps one slot per DID. A slot is made the first time its DID is
\* named and stays in the map from then on. A slot holds four things that
\* change together in one atomic swap (an rcu): whether the DID is pinned, how
\* many holds it has, whether it has a key, and that key's signer.
\*
\* Who acts:
\*   * holders, each of which, again and again, may
\*       - hold (IdentityHold::new / hold_identity): look the slot up, making
\*         it if it is missing, then in one swap either add a hold (storing
\*         its key and signer if the slot has none) or, when the slot's key
\*         has another signer, refuse and take nothing;
\*       - retain (retain_identity): look the slot up, then in one swap add a
\*         hold only if the key is there, signing through the slot's signer;
\*       - release (IdentityHold::drop / release_identity): look the slot up
\*         by DID again, then in one swap take a hold away and drop the key if
\*         that left no hold and no pin;
\*   * the process, which may
\*       - pin (store_identity): look the slot up, then in one swap pin it
\*         with its signer, or refuse when the DID is held with another;
\*       - clear (clear_identity_store): in one swap per slot, unpin it and
\*         drop the key unless something still holds it.
\*
\* Looking a slot up and swapping it are two steps, so another holder or the
\* process can act between them. Every swap is one read-modify-write on one
\* atomic, and those are totally ordered, so TLC running one step at a time
\* sees every order the Rust can produce.
\*
\* What must never happen:
\*   1. the slot's count differs from the number of holds given out and not
\*      yet given back (a lost hold, or one counted twice) - INV_CountExact;
\*   2. a holder still has a hold and the store answers no key for its DID
\*      (a request it signs fails) - INV_KeyWhileHeld;
\*   3. the key stays in the store with no hold and no pin (a private key
\*      kept after its last user went), or is gone while the DID is pinned
\*      - INV_KeyMatchesHolders;
\*   4. a holder's DID signs through a signer other than the one the holder
\*      signs with (it was re-routed, maybe to a context already freed)
\*      - INV_OwnSigner.
\*
\* Each guard the code relies on has a RED configuration that removes it:
\*   * AtomicSwap: the count and key change in one swap (Red_SplitSwap);
\*   * CountHolds: the slot counts holds, not just "held or not"
\*     (Red_DropCount);
\*   * ClearKeepsSlots: clear never removes a slot from the map
\*     (Red_ClearRemoves);
\*   * ClearKeepsHeld: clear keeps a key something holds (Red_ClearDropsHeld);
\*   * RetainNeedsKey: retain adds a hold only when the key is there
\*     (Red_RetainAbsent);
\*   * RefuseOther: a hold with another signer is refused, not stored over
\*     the held one (Red_ReplaceOnHold);
\*   * PinRefusesHeld: a pin never replaces a held DID's signer
\*     (Red_PinReplacesHeld).
\*
\* Rust: crates/defra-core/src/signing/identity_store.rs: slot, same_signer,
\* store_identity, hold_identity, IdentityHold::new, IdentityHold::drop,
\* retain_identity, release_identity, get_identity, clear_identity_store.
\* Holders: crates/ffi/src/state/identity.rs IdentityState::register,
\* NodeIdentity::hold, NodeIdentity::release; crates/ffi/src/remote_signer/
\* register.rs store; crates/defra-node/src/node_identity.rs
\* create_node_identity.
EXTENDS Naturals, FiniteSets

CONSTANTS
  Holders,          \* the holders that use the one DID modeled; each brings
                    \* its own signer, named after it
  PinSig,           \* the signer the process pins with
  NoSigner,         \* stands for "no key, so no signer"
  MaxSlots,         \* how many slots the map may make in a run (bounds TLC)
  AtomicSwap,       \* TRUE: count and key change in one swap
  CountHolds,       \* TRUE: the slot counts its holds
  ClearKeepsSlots,  \* TRUE: clear leaves every slot in the map
  ClearKeepsHeld,   \* TRUE: clear keeps a key that something holds
  RetainNeedsKey,   \* TRUE: retain adds a hold only when the key is there
  RefuseOther,      \* TRUE: a hold with another signer is refused
  PinRefusesHeld    \* TRUE: a pin never replaces a held DID's signer

\* The slot ids the map can make; 0 means the map has no entry for the DID.
Slots == 1..MaxSlots
\* Every signer: each holder's own, and the process's.
Signers == Holders \cup {PinSig}
\* What a slot or holder can name as its signer.
SignerOrNone == Signers \cup {NoSigner}
\* The fields a slot holds.
SlotRec == [pinned : BOOLEAN, holds : Nat, key : BOOLEAN, signer : SignerOrNone]

VARIABLES
  entry,  \* the slot the map holds for the DID, or 0 for none
  slots,  \* each slot's pinned flag, hold count, key and signer
  made,   \* how many slots the map has made so far
  pc,     \* where each holder is in its current operation
  at,     \* the slot each holder looked up for its current operation
  snap,   \* what a holder read, when a swap is split into read then write
  mine,   \* the signer each holder signs through while it holds
  admin   \* where the process is in a pin

vars == <<entry, slots, made, pc, at, snap, mine, admin>>

\* An empty slot: not pinned, no holds, no key.
Empty == [pinned |-> FALSE, holds |-> 0, key |-> FALSE, signer |-> NoSigner]

\* Where a holder can be:
\*   "idle"     no operation running and no hold;
\*   "hold"     looked the slot up to hold it, swap next;
\*   "holdW"    read the slot to hold it, write next (split swap only);
\*   "retain"   looked the slot up to retain it, swap next;
\*   "retainW"  read the slot to retain it, write next (split swap only);
\*   "holding"  has a hold;
\*   "release"  looked the slot up to give its hold back, swap next;
\*   "releaseW" read the slot to give its hold back, write next.
Pcs == {"idle", "hold", "holdW", "retain", "retainW", "holding", "release",
        "releaseW"}

\* The holders whose hold is given out and not yet given back: a holder
\* counts from the swap that took its hold until the swap that gives it back.
Holding == {h \in Holders : pc[h] \in {"holding", "release", "releaseW"}}

\* Every variable has the kind of value it should.
TypeOK ==
  /\ entry \in 0..MaxSlots
  /\ slots \in [Slots -> SlotRec]
  /\ made \in 0..MaxSlots
  /\ pc \in [Holders -> Pcs]
  /\ at \in [Holders -> 0..MaxSlots]
  /\ snap \in [Holders -> SlotRec]
  /\ mine \in [Holders -> SignerOrNone]
  /\ admin \in {"idle", "pin"}

\* At start the map is empty, nothing is made, and nobody holds anything.
Init ==
  /\ entry = 0
  /\ slots = [s \in Slots |-> Empty]
  /\ made = 0
  /\ pc = [h \in Holders |-> "idle"]
  /\ at = [h \in Holders |-> 0]
  /\ snap = [h \in Holders |-> Empty]
  /\ mine = [h \in Holders |-> NoSigner]
  /\ admin = "idle"

\* slot(): the DID's slot, made and put in the map if the map has none. Only
\* enabled while the model may still make a slot, which bounds TLC.
CanFind == entry # 0 \/ made < MaxSlots
\* The slot slot() answers.
Found == IF entry # 0 THEN entry ELSE made + 1
\* The map after slot(): unchanged, or holding the new slot.
FindEntry == entry' = Found
\* The count of slots made after slot().
FindMade == made' = IF entry # 0 THEN made ELSE made + 1

\* ---- what each swap writes, from what it read ----

\* hold_identity refuses when the slot's key has another signer (same_signer
\* is false). With RefuseOther off it never refuses.
HoldRefused(v, g) == RefuseOther /\ v.key /\ v.signer # g

\* hold_identity's swap with signer g: one more hold, and the key stored with
\* g if the slot had none (it keeps its own otherwise, which is g unless
\* RefuseOther is off). With CountHolds off the count only says "held".
AfterHold(v, g) ==
  [v EXCEPT !.holds = IF CountHolds THEN v.holds + 1 ELSE 1,
            !.key = TRUE,
            !.signer = IF v.key /\ RefuseOther THEN v.signer ELSE g]

\* retain_identity's swap: one more hold, only when the key is there (or
\* always, with RetainNeedsKey off).
Retains(v) == v.key \/ ~RetainNeedsKey
AfterRetain(v) ==
  IF Retains(v)
  THEN [v EXCEPT !.holds = IF CountHolds THEN v.holds + 1 ELSE 1]
  ELSE v

\* release_identity's swap: one hold fewer (never below zero), and the key
\* dropped when that leaves no hold and no pin. With CountHolds off, one
\* release leaves nothing held.
AfterRelease(v) ==
  LET holds == IF CountHolds THEN (IF v.holds > 0 THEN v.holds - 1 ELSE 0)
                             ELSE 0
      keep == v.pinned \/ holds > 0
  IN [v EXCEPT !.holds = holds,
               !.key = IF keep THEN v.key ELSE FALSE,
               !.signer = IF keep THEN v.signer ELSE NoSigner]

\* store_identity refuses when something holds the DID with another signer.
PinRefused(v) == PinRefusesHeld /\ v.holds > 0 /\ v.key /\ v.signer # PinSig

\* store_identity's swap: pinned, with the process's key and signer.
AfterPin(v) == [v EXCEPT !.pinned = TRUE, !.key = TRUE, !.signer = PinSig]

\* clear_identity_store's swap: unpinned, and the key dropped unless held.
\* With ClearKeepsHeld off the key goes whatever holds it.
AfterClear(v) ==
  LET keep == ClearKeepsHeld /\ v.holds > 0
  IN [v EXCEPT !.pinned = FALSE,
               !.key = IF keep THEN v.key ELSE FALSE,
               !.signer = IF keep THEN v.signer ELSE NoSigner]

\* ---- holders ----

\* An idle holder starts a hold: looks the slot up, making it if missing.
StartHold(h) ==
  /\ pc[h] = "idle" /\ CanFind
  /\ FindEntry /\ FindMade
  /\ pc' = [pc EXCEPT ![h] = "hold"]
  /\ at' = [at EXCEPT ![h] = Found]
  /\ UNCHANGED <<slots, snap, mine, admin>>

\* The hold's swap, in one step: the holder now holds and signs through its
\* own signer, or was refused and holds nothing.
SwapHold(h) ==
  /\ pc[h] = "hold" /\ AtomicSwap
  /\ IF HoldRefused(slots[at[h]], h)
     THEN /\ pc' = [pc EXCEPT ![h] = "idle"]
          /\ UNCHANGED <<slots, mine>>
     ELSE /\ slots' = [slots EXCEPT ![at[h]] = AfterHold(slots[at[h]], h)]
          /\ pc' = [pc EXCEPT ![h] = "holding"]
          /\ mine' = [mine EXCEPT ![h] = h]
  /\ UNCHANGED <<entry, made, at, snap, admin>>

\* An idle holder starts a retain: retain_identity only looks the slot up and
\* answers false at once when the map has none.
StartRetain(h) ==
  /\ pc[h] = "idle" /\ entry # 0
  /\ pc' = [pc EXCEPT ![h] = "retain"]
  /\ at' = [at EXCEPT ![h] = entry]
  /\ UNCHANGED <<entry, slots, made, snap, mine, admin>>

\* The retain's swap, in one step: a hold through the slot's signer when it
\* retained, idle otherwise.
SwapRetain(h) ==
  /\ pc[h] = "retain" /\ AtomicSwap
  /\ slots' = [slots EXCEPT ![at[h]] = AfterRetain(slots[at[h]])]
  /\ pc' = [pc EXCEPT ![h] = IF Retains(slots[at[h]]) THEN "holding" ELSE "idle"]
  /\ mine' = [mine EXCEPT ![h] = slots[at[h]].signer]
  /\ UNCHANGED <<entry, made, at, snap, admin>>

\* A holder gives its hold back: release_identity looks the slot up by DID
\* again. When the map has none, nothing is given back and the hold is lost.
StartRelease(h) ==
  /\ pc[h] = "holding"
  /\ IF entry # 0
     THEN /\ pc' = [pc EXCEPT ![h] = "release"]
          /\ at' = [at EXCEPT ![h] = entry]
     ELSE /\ pc' = [pc EXCEPT ![h] = "idle"]
          /\ UNCHANGED at
  /\ UNCHANGED <<entry, slots, made, snap, mine, admin>>

\* The release's swap, in one step: the holder has no hold any more.
SwapRelease(h) ==
  /\ pc[h] = "release" /\ AtomicSwap
  /\ slots' = [slots EXCEPT ![at[h]] = AfterRelease(slots[at[h]])]
  /\ pc' = [pc EXCEPT ![h] = "idle"]
  /\ UNCHANGED <<entry, made, at, snap, mine, admin>>

\* ---- the split swap (AtomicSwap off): read now, write what it read later ----

\* The holder reads the slot it is about to change, and goes on to write.
ReadSlot(h) ==
  /\ ~AtomicSwap /\ pc[h] \in {"hold", "retain", "release"}
  /\ snap' = [snap EXCEPT ![h] = slots[at[h]]]
  /\ pc' = [pc EXCEPT ![h] = IF pc[h] = "hold" THEN "holdW"
                             ELSE IF pc[h] = "retain" THEN "retainW"
                             ELSE "releaseW"]
  /\ UNCHANGED <<entry, slots, made, at, mine, admin>>

\* The holder writes what its read said, over whatever is there now.
WriteSlot(h) ==
  /\ pc[h] \in {"holdW", "retainW", "releaseW"}
  /\ LET v == snap[h]
         refused == pc[h] = "holdW" /\ HoldRefused(v, h)
     IN /\ slots' = IF refused THEN slots
                    ELSE [slots EXCEPT ![at[h]] =
                            IF pc[h] = "holdW" THEN AfterHold(v, h)
                            ELSE IF pc[h] = "retainW" THEN AfterRetain(v)
                            ELSE AfterRelease(v)]
        /\ pc' = [pc EXCEPT ![h] =
                    IF pc[h] = "holdW" THEN (IF refused THEN "idle" ELSE "holding")
                    ELSE IF pc[h] = "retainW"
                         THEN IF Retains(v) THEN "holding" ELSE "idle"
                    ELSE "idle"]
        /\ mine' = [mine EXCEPT ![h] =
                      IF pc[h] = "holdW" THEN h
                      ELSE IF pc[h] = "retainW" THEN v.signer
                      ELSE mine[h]]
  /\ UNCHANGED <<entry, made, at, snap, admin>>

\* ---- the process ----

\* store_identity starts: looks the slot up, making it if missing.
StartPin ==
  /\ admin = "idle" /\ CanFind
  /\ FindEntry /\ FindMade
  /\ admin' = "pin"
  /\ UNCHANGED <<slots, pc, at, snap, mine>>

\* The pin's swap, or its refusal. The slot it found is still the map's in
\* GREEN; with ClearKeepsSlots off a clear may have removed it, and the pin
\* then lands in a slot the map no longer holds: modeled as no change.
SwapPin ==
  /\ admin = "pin"
  /\ slots' = IF entry # 0 /\ ~PinRefused(slots[entry])
              THEN [slots EXCEPT ![entry] = AfterPin(slots[entry])]
              ELSE slots
  /\ admin' = "idle"
  /\ UNCHANGED <<entry, made, pc, at, snap, mine>>

\* clear_identity_store: every slot in the map is unpinned and loses the key
\* nothing holds. With ClearKeepsSlots off, the map's entry is removed
\* instead, and the slot is left to whoever still points at it.
Clear ==
  /\ entry # 0
  /\ IF ClearKeepsSlots
     THEN /\ slots' = [slots EXCEPT ![entry] = AfterClear(slots[entry])]
          /\ UNCHANGED entry
     ELSE /\ entry' = 0
          /\ UNCHANGED slots
  /\ UNCHANGED <<made, pc, at, snap, mine, admin>>

Next ==
  \/ \E h \in Holders :
       \/ StartHold(h) \/ SwapHold(h)
       \/ StartRetain(h) \/ SwapRetain(h)
       \/ StartRelease(h) \/ SwapRelease(h)
       \/ ReadSlot(h) \/ WriteSlot(h)
  \/ StartPin \/ SwapPin \/ Clear

Spec == Init /\ [][Next]_vars

\* ---- what must hold in every state ----

\* The map's slot counts exactly the holds given out and not given back; with
\* no slot in the map, nobody may hold one.
INV_CountExact ==
  IF entry = 0 THEN Holding = {}
  ELSE slots[entry].holds = Cardinality(Holding)

\* While anyone holds the DID, get_identity finds its key.
INV_KeyWhileHeld ==
  Holding # {} => (entry # 0 /\ slots[entry].key)

\* The map's slot has its key exactly when it is pinned or held: no private
\* key outlives its last holder, and a pinned DID always answers its key.
INV_KeyMatchesHolders ==
  entry # 0 =>
    (slots[entry].key <=> (slots[entry].pinned \/ slots[entry].holds > 0))

\* Every holder's DID signs through the very signer that holder signs with:
\* no hold or pin re-routed it to another.
INV_OwnSigner ==
  \A h \in Holding : entry # 0 /\ slots[entry].signer = mine[h]
====
