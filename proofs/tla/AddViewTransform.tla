--------------------------- MODULE AddViewTransform ---------------------------
(***************************************************************************)
(* The story, in plain words.                                              *)
(*                                                                         *)
(* A caller adds a view and may name a lens transform (by its CID) that    *)
(* the view's rows go through. The node checks the caller holds the NAC    *)
(* ViewAdd permission, refuses an empty CID, refuses a CID the node holds  *)
(* no lens for, and then commits the view's definition with the CID        *)
(* stored on its query source.                                             *)
(*                                                                         *)
(* While it works, the lens may be reaching the node, in one of two ways.  *)
(* Someone adds it locally, or P2P sync fetches it from a peer. Either way *)
(* its config commits durably first, and only after the commit is it      *)
(* registered in the node's in-memory lens store. (Before #2724's fix, a   *)
(* synced lens went into memory only, with no config: a RED shows that.)  *)
(* Meanwhile someone may grant or revoke the caller's ViewAdd permission.  *)
(*                                                                         *)
(* What must never happen: a committed view names a transform the node     *)
(* holds no durable lens for (it would be gone after a restart), names an  *)
(* empty one, or names a different one from the one the caller asked for; *)
(* a view commits for a caller that lacks the permission at the point the  *)
(* view commits; or a caller hears "no such lens" for a lens the node      *)
(* holds (nothing valid is rejected).                                      *)
(*                                                                         *)
(* Each check reads either inside the transaction that commits the view    *)
(* ("txn", validated at that commit) or outside it beforehand. The code    *)
(* reads the lens config in the create transaction. Asking the in-memory   *)
(* lens store instead ("memory", a RED) lags the lens commit, so it can    *)
(* reject a valid view. The code still checks NAC before the transaction   *)
(* opens, known gap #2736: Green_NacBefore checks every other promise with *)
(* NAC there, and the KnownGap RED shows the permission promise it breaks. *)
(*                                                                         *)
(* Rust code modelled:                                                     *)
(*   crates/defra-node/src/db_impls.rs DbSchemaOps::add_view               *)
(*   crates/db/src/collection/ops/create.rs                                *)
(*     DB::create_collections_atomic_with_acp_registration_as,             *)
(*     DB::require_view_transforms (the lens config, in the transaction)   *)
(*   crates/db/src/definition/migration/committed_lenses.rs DB::add_lens,  *)
(*     DB::commit_synced_lens, write_lens_config                           *)
(*   crates/p2p-adapter/src/version_syncer.rs sync_lens,                   *)
(*   crates/p2p-adapter/src/transport_version_syncer.rs sync_lens          *)
(* Go reference: internal/db/view.go, internal/db/lens.go.                 *)
(***************************************************************************)
EXTENDS Naturals

CONSTANTS
  \* TRUE when the empty-CID check runs.
  CheckEmpty,
  \* TRUE when the node checks it holds the named lens.
  CheckLens,
  \* TRUE when the accepted CID is stored on the view's query source.
  StoreTransform,
  \* Where the lens check reads: "txn" (the lens config, in the view's
  \* transaction) or "memory" (the in-memory store, beforehand).
  LensAt,
  \* Where the NAC check reads: "txn" (in the view's transaction) or
  \* "before" (beforehand, outside it).
  NacAt,
  \* TRUE when P2P sync commits a fetched lens's config before registering
  \* it; FALSE when it only registers it in memory.
  SyncPersists,
  \* How many permission changes other transactions may commit.
  MaxEnv

\* The constants have the shapes they should.
ASSUME /\ CheckEmpty \in BOOLEAN /\ CheckLens \in BOOLEAN
       /\ StoreTransform \in BOOLEAN /\ SyncPersists \in BOOLEAN
       /\ LensAt \in {"txn", "memory"} /\ NacAt \in {"txn", "before"}
       /\ MaxEnv \in Nat

\* What a caller can name: no transform, an empty CID, or the lens "L".
Asks == {"none", "empty", "L"}

\* The states lens L can be in: "absent"; "committed" (config durable, not
\* yet in memory); "registered" (durable and in memory); "memoryOnly" (in
\* memory with no config, which only a memory-only sync makes).
LensStates == {"absent", "committed", "registered"}
                \cup (IF SyncPersists THEN {} ELSE {"memoryOnly"})

VARIABLES
  \* The transform the caller asked for.
  asked,
  \* Lens L's state, one of LensStates.
  lens,
  \* Whether the caller holds ViewAdd in the latest commit.
  nac,
  \* Where add_view is: "check", "commit" or "done".
  pc,
  \* The outcome: "pending", "stored", "denied", "empty" or "missing".
  result,
  \* The transform the committed view names ("none" if it names none).
  stored,
  \* The facts at the point the view's transaction decides, recorded when
  \* add_view ends: the permission, whether L's config had committed, and
  \* whether the node held L in any form.
  nacAtDecision, lensAtDecision, heldAtDecision,
  \* How many permission changes have happened.
  env

vars == <<asked, lens, nac, pc, result, stored, nacAtDecision,
          lensAtDecision, heldAtDecision, env>>

\* Whether L's config has committed.
LensCommitted == lens \in {"committed", "registered"}

\* Whether the in-memory store holds L.
LensInMemory == lens \in {"registered", "memoryOnly"}

\* Whether the node holds L at all, durably or in memory.
LensHeld == LensCommitted \/ LensInMemory

\* What the lens check sees, at the place it reads.
LensSeen == IF LensAt = "txn" THEN LensCommitted ELSE LensInMemory

\* Every variable holds a value of the shape it should.
TypeOK ==
  /\ asked \in Asks
  /\ lens \in LensStates
  /\ nac \in BOOLEAN
  /\ pc \in {"check", "commit", "done"}
  /\ result \in {"pending", "stored", "denied", "empty", "missing"}
  /\ stored \in Asks
  /\ nacAtDecision \in BOOLEAN
  /\ lensAtDecision \in BOOLEAN
  /\ heldAtDecision \in BOOLEAN
  /\ env \in 0..MaxEnv

\* Any request, any permission, and L in any state.
Init ==
  /\ asked \in Asks
  /\ lens \in LensStates
  /\ nac \in BOOLEAN
  /\ pc = "check"
  /\ result = "pending"
  /\ stored = "none"
  /\ nacAtDecision = FALSE
  /\ lensAtDecision = FALSE
  /\ heldAtDecision = FALSE
  /\ env = 0

\* Ends add_view with outcome `r`, recording the facts at this point.
Finish(r) ==
  /\ result' = r /\ pc' = "done"
  /\ nacAtDecision' = nac /\ lensAtDecision' = LensCommitted
  /\ heldAtDecision' = LensHeld

\* The decision records, left as they are.
Undecided == UNCHANGED <<result, nacAtDecision, lensAtDecision, heldAtDecision>>

\* The checks that run before the view's transaction opens: NAC if it is
\* read beforehand, the empty CID, and the lens if it is read from memory.
Check ==
  /\ pc = "check"
  /\ CASE NacAt = "before" /\ ~nac -> Finish("denied")
       [] CheckEmpty /\ asked = "empty" -> Finish("empty")
       [] CheckLens /\ LensAt = "memory" /\ asked = "L" /\ ~LensSeen
            -> Finish("missing")
       \* every check that runs here passed: on to the transaction
       [] OTHER -> pc' = "commit" /\ Undecided
  /\ UNCHANGED <<asked, lens, nac, stored, env>>

\* The view's transaction: the checks read inside it run now, and the view
\* commits with the transform stored on its query source. Reads inside the
\* transaction are validated at commit, so they see the state the commit
\* lands on.
Commit ==
  /\ pc = "commit"
  /\ CASE NacAt = "txn" /\ ~nac -> Finish("denied") /\ stored' = stored
       [] CheckLens /\ LensAt = "txn" /\ asked = "L" /\ ~LensSeen
            -> Finish("missing") /\ stored' = stored
       \* the view commits, naming the transform if it is stored
       [] OTHER -> /\ Finish("stored")
                   /\ stored' = IF StoreTransform THEN asked ELSE "none"
  /\ UNCHANGED <<asked, lens, nac, env>>

\* Someone adds L locally: its config commits first.
LensCommit ==
  /\ lens = "absent"
  /\ lens' = "committed"
  /\ UNCHANGED <<asked, nac, pc, result, stored, nacAtDecision,
                 lensAtDecision, heldAtDecision, env>>

\* P2P sync fetches L from a peer: its config commits first, or (a memory-
\* only sync) it goes straight into memory with no config.
LensSync ==
  /\ lens = "absent"
  /\ lens' = IF SyncPersists THEN "committed" ELSE "memoryOnly"
  /\ UNCHANGED <<asked, nac, pc, result, stored, nacAtDecision,
                 lensAtDecision, heldAtDecision, env>>

\* After the commit, the follow-up registers L in the in-memory store.
LensRegister ==
  /\ lens = "committed"
  /\ lens' = "registered"
  /\ UNCHANGED <<asked, nac, pc, result, stored, nacAtDecision,
                 lensAtDecision, heldAtDecision, env>>

\* Another transaction grants or revokes the caller's ViewAdd permission.
Env ==
  /\ env < MaxEnv
  /\ pc # "done"
  /\ nac' = ~nac
  /\ env' = env + 1
  /\ UNCHANGED <<asked, lens, pc, result, stored, nacAtDecision,
                 lensAtDecision, heldAtDecision>>

\* Each step is one of add_view's, a lens arrival or registration, or a
\* permission change.
Next == Check \/ Commit \/ LensCommit \/ LensSync \/ LensRegister \/ Env

\* The whole behaviour: start in Init, take steps from Next.
Spec == Init /\ [][Next]_vars

\* A committed view never names an empty transform.
INV_NoEmptyTransform == stored # "empty"

\* A committed view only names a lens whose config had committed when the
\* view committed, so the lens is still there after a restart.
INV_StoredTransformExists == stored = "L" => lensAtDecision

\* A committed view names exactly the transform the caller asked for.
INV_StoredIsAccepted == result = "stored" => stored = asked

\* A view only commits for a caller holding ViewAdd when it commits.
INV_NoViewWithoutPermission == result = "stored" => nacAtDecision

\* "No such lens" is only said when the node did not hold L in any form:
\* nothing valid is rejected.
INV_NoValidRejected == result = "missing" => ~heldAtDecision
=============================================================================
