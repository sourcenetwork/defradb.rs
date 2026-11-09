---------------------------- MODULE VerifySignature ----------------------------
(***************************************************************************)
(* The story, in plain words.                                              *)
(*                                                                         *)
(* A caller asks the node: "was block B signed by this key?". Before it    *)
(* answers "verified", the node must be sure of three things: the caller   *)
(* holds the node permission to verify signatures (NAC), the caller may    *)
(* read the document the block belongs to (document ACP), and the block is *)
(* there. The caller may ask inside its own explicit transaction, which    *)
(* can hold writes nobody else sees yet: a block it just wrote, or a grant *)
(* or revocation of document access. Or it may ask with no transaction, in *)
(* which case the node opens a read-only one for the question.             *)
(*                                                                         *)
(* While the node works, other transactions keep committing: someone       *)
(* grants or revokes the caller's NAC permission, grants or revokes its    *)
(* document access, writes the block or purges it.                         *)
(*                                                                         *)
(* What must never happen: the node says "verified" although, at the one   *)
(* point the answer is about (the caller's transaction view), the caller   *)
(* lacked the permission, could not read the block, or the block was not   *)
(* there. And the node must not say "no" when at that point all three      *)
(* hold: nothing valid is rejected.                                        *)
(*                                                                         *)
(* The one consistent point is the transaction view: the snapshot the      *)
(* transaction reads plus its own pending writes. Each check reads either  *)
(* that view ("view") or whatever was committed last ("latest"). The code  *)
(* reads all three at the view: NAC through check_node_access_in_txn and   *)
(* document ACP bound to the view's ACP store. Each RED moves one read off *)
(* the view, as the code did before #2724's fix for NAC and document ACP.  *)
(*                                                                         *)
(* Rust code modelled:                                                     *)
(*   crates/defra-node/src/ops/block.rs NodeOps::verify_block_signature    *)
(*     (the in-transaction or the pool path)                               *)
(*   crates/db/src/txn/registry/blocks.rs                                  *)
(*     DbTransactionRegistry::verify_block_signature_in_txn_as             *)
(*   crates/db/src/block/verify.rs verify_block_signature_as,              *)
(*     verify_block_signature_in_txn_as,                                   *)
(*     verify_block_signature_with_blockstore (NAC at the view),           *)
(*     load_authorized_block_signature (block read, bound document ACP)    *)
(*   crates/db/src/access/nac.rs DB::check_node_access_in_txn              *)
(* Go reference: internal/db/verify.go.                                    *)
(***************************************************************************)
EXTENDS Naturals

CONSTANTS
  \* TRUE: the caller asks inside its own explicit transaction.
  InTxn,
  \* TRUE when the NAC SignatureVerify check runs at all.
  CheckNac,
  \* TRUE when the document ACP read check runs at all.
  CheckAcp,
  \* TRUE when the node checks the block is there at all.
  CheckBlock,
  \* Where each check reads: "view" (the transaction view) or "latest".
  NacAt, AcpAt, BlockAt,
  \* How many concurrent commits other transactions may make.
  MaxEnv

\* The places a check can read from.
Places == {"view", "latest"}

\* The constants have the shapes they should.
ASSUME /\ InTxn \in BOOLEAN /\ CheckNac \in BOOLEAN
       /\ CheckAcp \in BOOLEAN /\ CheckBlock \in BOOLEAN
       /\ NacAt \in Places /\ AcpAt \in Places /\ BlockAt \in Places
       /\ MaxEnv \in Nat

VARIABLES
  \* What the database has committed last: NAC permission, document
  \* access and block presence, each TRUE or FALSE.
  committed,
  \* The caller's own pending writes in its explicit transaction.
  pending,
  \* The transaction view the answer is about, or NoView before it opens.
  view,
  \* Which check the node runs next.
  pc,
  \* The answer so far: "pending", "verified" or "rejected".
  verdict,
  \* How many concurrent commits have happened.
  env

vars == <<committed, pending, view, pc, verdict, env>>

\* A database state: the three facts the answer depends on.
States == [nac : BOOLEAN, acp : BOOLEAN, block : BOOLEAN]

\* A pending write can grant or revoke document access, or write the block.
\* NAC relationships are not changed inside a user transaction, so a
\* pending write never touches NAC.
Pendings == [acp : {"none", "grant", "revoke"}, block : {"none", "put"}]

\* No pending writes at all: the shape of a request with no transaction.
NoPending == [acp |-> "none", block |-> "none"]

\* Marks a view that has not opened yet.
NoView == [nac |-> FALSE, acp |-> FALSE, block |-> FALSE, open |-> FALSE]

\* The view a transaction sees: the snapshot `s` with its own writes `p`
\* laid over it.
Apply(s, p) ==
  [nac   |-> s.nac,
   \* a pending grant or revoke wins over the snapshot's access
   acp   |-> CASE p.acp = "grant" -> TRUE
               [] p.acp = "revoke" -> FALSE
               [] OTHER -> s.acp,
   \* a block the transaction wrote is there for it
   block |-> s.block \/ p.block = "put",
   open  |-> TRUE]

\* The view, opening it on the latest commit if it is not open yet.
Opened == IF view.open THEN view ELSE Apply(committed, pending)

\* What a check reads for fact `f`: the view or the latest commit.
Read(place, v, f) == IF place = "view" THEN v[f] ELSE committed[f]

\* Every variable holds a value of the shape it should.
TypeOK ==
  /\ committed \in States
  /\ pending \in Pendings
  /\ view.open \in BOOLEAN
  /\ pc \in {"nac", "block", "acp", "done"}
  /\ verdict \in {"pending", "verified", "rejected"}
  /\ env \in 0..MaxEnv

\* Any committed state and, inside a transaction, any pending writes. A
\* caller's explicit transaction is already open, so its view is fixed now;
\* a request with no transaction opens its view later.
Init ==
  /\ committed \in States
  /\ pending \in (IF InTxn THEN Pendings ELSE {NoPending})
  /\ view = (IF InTxn THEN Apply(committed, pending) ELSE NoView)
  /\ pc = "nac"
  /\ verdict = "pending"
  /\ env = 0

\* Step 1, the NAC check. Reading at the view opens it now (the pool path
\* opens its read-only transaction first); reading at the latest commit (a
\* RED) leaves a request with no transaction to open its view later.
NacStep ==
  /\ pc = "nac"
  \* the view after this step: opened only if the check reads it
  /\ LET v == IF NacAt = "view" THEN Opened ELSE view
     IN /\ view' = v
        \* a failing enforced check ends the request with a "no"
        /\ IF CheckNac /\ ~Read(NacAt, v, "nac")
             THEN pc' = "done" /\ verdict' = "rejected"
             ELSE pc' = "block" /\ verdict' = verdict
  /\ UNCHANGED <<committed, pending, env>>

\* Step 2, the block read. The pool path opens its read-only transaction
\* here at the latest, before it reads the block (as the code does).
BlockStep ==
  /\ pc = "block"
  \* the transaction is open from here on, whatever the check reads
  /\ LET v == Opened
     IN /\ view' = v
        \* a missing block ends the request with a "no"
        /\ IF CheckBlock /\ ~Read(BlockAt, v, "block")
             THEN pc' = "done" /\ verdict' = "rejected"
             ELSE pc' = "acp" /\ verdict' = verdict
  /\ UNCHANGED <<committed, pending, env>>

\* Step 3, document ACP as the caller, then the signature itself (assumed
\* valid: the cryptography is not what this model is about).
AcpStep ==
  /\ pc = "acp"
  \* a caller who may not read the block hears "no"; otherwise "verified"
  /\ IF CheckAcp /\ ~Read(AcpAt, view, "acp")
       THEN verdict' = "rejected"
       ELSE verdict' = "verified"
  /\ pc' = "done"
  /\ UNCHANGED <<committed, pending, view, env>>

\* Another transaction commits: it flips one of the three facts. Only the
\* latest commit changes; an open view keeps its snapshot.
Env ==
  /\ env < MaxEnv
  /\ pc # "done"
  \* pick the fact that changes
  /\ \E f \in {"nac", "acp", "block"} :
       committed' = [committed EXCEPT ![f] = ~@]
  /\ env' = env + 1
  /\ UNCHANGED <<pending, view, pc, verdict>>

\* Each step is one of the node's steps or a concurrent commit.
Next == NacStep \/ BlockStep \/ AcpStep \/ Env

\* The whole behaviour: start in Init, take steps from Next.
Spec == Init /\ [][Next]_vars

\* All three facts hold at the transaction view.
AllHold == view.nac /\ view.acp /\ view.block

\* "Verified" is only ever said when, at the transaction view, the caller
\* had the permission, could read the block, and the block was there.
INV_VerifiedAtOnePoint == verdict = "verified" => AllHold

\* "No" is only ever said when, at the transaction view, one of the three
\* facts failed: nothing valid is rejected.
INV_NoValidRejected == verdict = "rejected" => ~AllHold
=============================================================================
