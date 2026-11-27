----------------------------- MODULE EmbeddedSigner -----------------------------
\* The story: an embedded caller calls one of the node's entry points (run a
\* request, run a query string as a caller, import a backup, add or patch a
\* collection, truncate one, add or refresh a view) and may hand along a
\* signing key of its own. Every block the call writes is signed: with the
\* caller's key when it brought one, otherwise with the node's own key. Every
\* entry point first passes the caller's key through one admission check.
\* What must never happen:
\*   - a block signed with one identity's key but written by another identity
\*     (a forgery: B writes, the block says A wrote it);
\*   - a block signed with a key the caller never handed over, such as one the
\*     process keeps in its registry for the caller's DID;
\*   - a block signed with a key no peer can check (an unsupported type, or a
\*     private key that does not match its public key);
\*   - a block a caller's key signed while the request bypassed DAC.
\* Rust it models:
\*   - `admitted_signer` in crates/entry-points/defra-node/src/embedded_signer.rs
\*     (bypass, `EmbeddedSigner::from_config`'s key type and key checks, actor
\*     match) is the step `Admit`;
\*   - `Node::admit` and `Node::admitted_caller` in
\*     crates/entry-points/defra-node/src/ops/request.rs are the boundary every
\*     context-taking entry of the node's core calls; `RequestPath::execute` in
\*     crates/entry-points/defra-node/src/request_path.rs (every request, and
\*     each write of a backup import), `NodeCore::execute_as` in
\*     crates/entry-points/defra-node/src/node_core/execute.rs and
\*     `NodeCore::import_backup` in
\*     crates/entry-points/defra-node/src/node_core/backup.rs are members of
\*     `Entries`;
\*   - the node-key fallback in `NodeCore::execute_as` and
\*     `RequestPath::execute`, which looks up the node's DID only and never the
\*     request's, is `SigningKey` choosing `NodeKey`.
\* Each call is decided on its own, so the model runs one call through the
\* boundary; TLC tries every entry point with every call.
\* We need plain numbers for the identities.
EXTENDS Naturals

\* Which guard a RED configuration removes; "None" keeps every guard.
CONSTANT Bug

\* The node's entry points that take a caller's context and can write blocks.
Entries == {"executeRequest", "executeAs", "importBackupAs", "addCollectionAs",
            "truncateCollectionAs", "addViewAs", "refreshViewsAs", "patchCollectionAs"}
\* The entry point the "Skip" RED plants its bug in: it writes without
\* admission.
Skipper == "importBackupAs"
\* 0 means "nobody": no actor named, or no key brought.
NoOne == 0
\* Two different callers, who can act and own a key.
Callers == {1, 2}
\* The signature that comes from the node's own key, never a caller's.
NodeKey == 99
\* The key types the model tries: the two the node admits and one it refuses
\* (Secp256r1, BLS, anything else), which all behave the same here.
KeyTypes == {"ed25519", "secp256k1", "other"}
\* The key types a caller's key may sign with.
Supported == {"ed25519", "secp256k1"}

\* phase: where the one call is (idle, arrived, admitted, refused, done).
\* entry: which entry point it called. call: what it carried.
\* blocks: every block written, with who wrote it and whose key signed it.
VARIABLES phase, entry, call, blocks
\* All the variables together, so a step that changes nothing can be named.
vars == <<phase, entry, call, blocks>>

\* Everything a call can carry: the actor it names, whose key it brought, the
\* key's type, whether the key's private and public halves match, and whether
\* it asks to bypass DAC.
Calls == [actor : {NoOne} \cup Callers, key : {NoOne} \cup Callers,
          keyType : KeyTypes, keyValid : BOOLEAN, bypass : BOOLEAN]

\* At the start nothing has been called and nothing written.
Init == /\ phase = "idle"
        \* No entry chosen yet; arrival overwrites this placeholder.
        /\ entry = "executeRequest"
        \* No call yet: the empty call names nobody and brings no key.
        /\ call = [actor |-> NoOne, key |-> NoOne, keyType |-> "other",
                   keyValid |-> FALSE, bypass |-> FALSE]
        \* No block written.
        /\ blocks = {}

\* A caller calls entry point e with what c carries.
Arrive(e, c) ==
  \* Only an idle node takes the call.
  /\ phase = "idle"
  \* Record the entry and the call.
  /\ entry' = e /\ call' = c
  \* The call now waits at the boundary.
  /\ phase' = "arrived"
  \* Nothing is written on arrival.
  /\ UNCHANGED blocks

\* Whether entry point e goes through the boundary at all; the "Skip" RED
\* makes one entry point skip it.
GoesThroughAdmission(e) == ~(Bug = "Skip" /\ e = Skipper)

\* The admission check on a call that brought a key, guard by guard; each RED
\* bug drops one guard.
KeyAdmitted(c) ==
  \* No DAC bypass with a caller's key.
  /\ (~c.bypass \/ Bug = "Bypass")
  \* The key is of a type the node signs with.
  /\ (c.keyType \in Supported \/ Bug = "KeyType")
  \* The private key is local and matches its public key.
  /\ (c.keyValid \/ Bug = "Key")
  \* The key belongs to the actor the call names, who must be someone.
  /\ ((c.actor # NoOne /\ c.key = c.actor) \/ Bug = "Actor")

\* The boundary decides: a call with no key goes on (the node will sign); a
\* call whose key passes goes on; a call on an entry that skips the boundary
\* goes on unchecked; any other call is refused before anything is written.
Admit ==
  \* Only a call waiting at the boundary is decided.
  /\ phase = "arrived"
  \* Go on, or stop with a refusal.
  /\ phase' = IF call.key = NoOne \/ ~GoesThroughAdmission(entry) \/ KeyAdmitted(call)
              THEN "admitted" ELSE "refused"
  \* Deciding writes nothing and changes nothing else.
  /\ UNCHANGED <<entry, call, blocks>>

\* The key the node signs a call's blocks with: the caller's own key when it
\* brought one; otherwise the node's key. The "Registry" RED instead looks up
\* a key the process holds for the actor's DID first (every caller has one).
SigningKey(c) ==
  \* A key the caller brought signs.
  IF c.key # NoOne THEN c.key
  \* The planted bug: the registry's key for the request's DID signs.
  ELSE IF Bug = "Registry" /\ c.actor # NoOne THEN c.actor
  \* Otherwise the node signs.
  ELSE NodeKey

\* An admitted call writes one block, signed with `SigningKey`.
Write ==
  \* Only an admitted call writes.
  /\ phase = "admitted"
  \* Record who wrote the block, whose key signed it, the key the caller
  \* handed over, the facts about that key and the request, and the entry.
  /\ blocks' = blocks \cup {[author |-> call.actor, signer |-> SigningKey(call),
                             presented |-> call.key, keyType |-> call.keyType,
                             keyValid |-> call.keyValid, bypass |-> call.bypass,
                             via |-> entry]}
  \* The call is finished.
  /\ phase' = "done"
  \* The entry and the call stay as they were.
  /\ UNCHANGED <<entry, call>>

\* Try every entry point with every call, then decide it, then write it.
Next == \/ \E e \in Entries, c \in Calls : Arrive(e, c)
        \* Decide at the boundary.
        \/ Admit
        \* Write the block of an admitted call.
        \/ Write

\* The whole behaviour: start empty, then take steps.
Spec == Init /\ [][Next]_vars

\* Promise: no forgery. A block a caller's key signed was written by that same
\* caller: nobody's key ever vouches for someone else's write.
INV_NoForgery == \A b \in blocks : b.signer # NodeKey => b.signer = b.author

\* Promise: a caller's key signs only when the caller handed it over. A key the
\* process happens to hold for the caller's DID never signs for them.
INV_PresentedOnly == \A b \in blocks : b.signer # NodeKey => b.signer = b.presented

\* Promise: every caller-signed block can be checked by any peer: its key is of
\* a type peers verify, and its private key matches the public key it names.
INV_Verifiable == \A b \in blocks :
                    \* Only a caller's key is in question; the node's is its own.
                    b.signer # NodeKey => (b.keyType \in Supported /\ b.keyValid)

\* Promise: a caller's key never signs a write that bypassed DAC, so a
\* caller-signed block was always written under the caller's own access.
INV_UnderDac == \A b \in blocks : b.signer # NodeKey => ~b.bypass

\* Promise: blocks get written at all, so the invariants above are not true
\* only because nothing ever happens. Checked as a property that must fail:
\* see MC_EmbeddedSigner_Green_Writes.cfg.
NothingWritten == blocks = {}
=============================================================================
