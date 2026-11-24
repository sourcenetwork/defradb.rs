----------------------------- MODULE EmbeddedSigner -----------------------------
\* The caller holds a local signing capability while the node owns the transaction.
EXTENDS Naturals
\* Each RED configuration removes one condition of the intended dispatch.
CONSTANT Bug
\* The finite state records a single request and whether it produced a write.
VARIABLES started, wrote, actor, signer, valid, nodeOwned, authorized, bypass
vars == <<started, wrote, actor, signer, valid, nodeOwned, authorized, bypass>>
\* No request has started and no block was written.
Init == /\ started = FALSE /\ wrote = FALSE
        /\ actor = 0 /\ signer = 0 /\ valid = FALSE
        /\ nodeOwned = FALSE /\ authorized = FALSE /\ bypass = FALSE
\* The native boundary dispatches only with a matching usable key and registry owner.
Execute(a, s, v, n, p, b) ==
  /\ ~started
  /\ started' = TRUE
  /\ actor' = a /\ signer' = s /\ valid' = v
  /\ nodeOwned' = n /\ authorized' = p /\ bypass' = b
  /\ wrote' = ((a # 0 \/ Bug = "Actor") /\
                (s = a \/ Bug = "Match") /\
                (v \/ Bug = "Key") /\
                (n \/ Bug = "Owner") /\
                (p \/ Bug = "Acp") /\
                (~b \/ Bug = "Bypass"))
\* Explore a missing actor, two distinct identities, and all security decisions.
Next == \E a, s \in 0..2, v, n, p, b \in BOOLEAN : Execute(a, s, v, n, p, b)
\* A refusal ends the single request without producing a block.
Spec == Init /\ [][Next]_vars
\* Every produced block has its selected actor's usable key and native authority.
INV_Bound == wrote => (actor # 0 /\ signer = actor /\ valid /\ nodeOwned /\ authorized /\ ~bypass)
=============================================================================
