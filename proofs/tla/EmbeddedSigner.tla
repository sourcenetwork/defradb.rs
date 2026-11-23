----------------------------- MODULE EmbeddedSigner -----------------------------
\* The story: an embedded caller hands the node one request together with a
\* signing key of its own. The node must sign the request's blocks with that
\* key only when the key is a local Ed25519 or secp256k1 key, is usable, and
\* belongs to the actor the request names, the node owns the transaction, ACP
\* lets the actor write, and DAC is not bypassed. What must never happen: a
\* block written under a key that breaks any one of those conditions.
\* Rust it models: `EmbeddedSigner::from_config` (the key type, the local key,
\* the key check) and `admitted_signer` (the actor match and the bypass) in
\* crates/entry-points/defra-node/src/embedded_signer.rs, which
\* `Node::execute_prepared_request` in crates/entry-points/defra-node/src/lib.rs
\* runs for every request that carries a caller's key.
\* We need plain numbers for the identities.
EXTENDS Naturals
\* Which guard a RED configuration removes; "None" keeps every guard.
CONSTANT Bug
\* The key types the model tries: the two the node admits and one it refuses
\* (Secp256r1, BLS, anything else), which all behave the same here.
KeyTypes == {"ed25519", "secp256k1", "other"}
\* The key types an embedded signer may sign with.
Supported == {"ed25519", "secp256k1"}
\* One request: whether it started, whether it wrote a block, and every fact
\* the node checked about it.
VARIABLES started, wrote, actor, signer, keyType, valid, nodeOwned, authorized, bypass
\* All the variables together, so a step that changes nothing can be named.
vars == <<started, wrote, actor, signer, keyType, valid, nodeOwned, authorized, bypass>>
\* At the start no request has run and no block was written.
Init == /\ started = FALSE /\ wrote = FALSE
        \* Identity 0 means "no identity"; nothing is decided yet.
        /\ actor = 0 /\ signer = 0 /\ keyType = "other" /\ valid = FALSE
        \* No owner, no ACP grant, no bypass recorded yet.
        /\ nodeOwned = FALSE /\ authorized = FALSE /\ bypass = FALSE
\* The request arrives with actor a, a key of signer s and type k, key check
\* v, node ownership n, ACP decision p and DAC bypass b.
Execute(a, s, k, v, n, p, b) ==
  \* Only one request is modelled, so it runs once.
  /\ ~started
  \* Mark that it has run.
  /\ started' = TRUE
  \* Record what the request carried.
  /\ actor' = a /\ signer' = s /\ keyType' = k /\ valid' = v
  \* Record what the node decided about it.
  /\ nodeOwned' = n /\ authorized' = p /\ bypass' = b
  \* A block is written only if every guard holds; each RED bug drops one.
  \* The request names an actor.
  /\ wrote' = ((a # 0 \/ Bug = "Actor") /\
                \* The key belongs to the named actor.
                (s = a \/ Bug = "Match") /\
                \* The key is of a type the node signs with.
                (k \in Supported \/ Bug = "KeyType") /\
                \* The private key is local and matches its public key.
                (v \/ Bug = "Key") /\
                \* The node owns the transaction the request runs in.
                (n \/ Bug = "Owner") /\
                \* ACP lets the actor make the write.
                (p \/ Bug = "Acp") /\
                \* DAC is not bypassed.
                (~b \/ Bug = "Bypass"))
\* Try every combination: no actor, two identities, every key type, and every
\* yes or no decision.
Next == \E a, s \in 0..2, k \in KeyTypes, v, n, p, b \in BOOLEAN : Execute(a, s, k, v, n, p, b)
\* The whole behaviour: start empty, then take steps.
Spec == Init /\ [][Next]_vars
\* Promise: every block written was signed by the named actor's own usable key
\* of a supported type, in a node-owned transaction ACP allowed, without bypass.
INV_Bound == wrote => (actor # 0 /\ signer = actor /\ keyType \in Supported /\ valid
                       \* ...and the node's own checks all passed.
                       /\ nodeOwned /\ authorized /\ ~bypass)
=============================================================================
