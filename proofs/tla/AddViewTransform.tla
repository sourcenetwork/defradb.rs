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
(* While it works, another task may be adding that very lens. Adding a     *)
(* lens is two steps: its config commits durably first, and only after the *)
(* commit is it registered in the node's in-memory lens store. Meanwhile   *)
(* someone may grant or revoke the caller's ViewAdd permission.            *)
(*                                                                         *)
(* What must never happen: a committed view names a transform the node     *)
(* holds no lens for, names an empty one, or names a different one from    *)
(* the one the caller asked for; a view commits for a caller that lacks    *)
(* the permission at the point the view commits; or a caller hears "no     *)
(* such lens" for a lens whose config had already committed (nothing valid *)
(* is rejected).                                                           *)
(*                                                                         *)
(* Each check reads either inside the transaction that commits the view    *)
(* ("txn", validated at that commit) or outside it beforehand. The code    *)
(* reads the lens config in the create transaction. Asking the in-memory   *)
(* lens store instead ("memory", a RED) lags the lens commit; lenses are   *)
(* never removed, so the lag can only reject a valid view. The code still  *)
(* A lens P2P sync registers has no config, so the code also accepts one   *)
(* found in memory; in this model registered implies committed, so that    *)
(* fallback changes nothing here. The code still                           *)
(* checks NAC before the transaction opens, known gap #2736: Green_NacBefore*)
(* checks every other promise with NAC there, and the KnownGap RED shows   *)
(* the permission promise it breaks.                                       *)
(*                                                                         *)
(* Rust code modelled:                                                     *)
(*   crates/defra-node/src/db_impls.rs DbSchemaOps::add_view               *)
(*   crates/db/src/collection/ops/create.rs                                *)
(*     DB::create_collections_atomic_with_acp_registration_as,             *)
(*     DB::require_view_transforms (the lens config, in the transaction)   *)
(*   crates/db/src/definition/migration/committed_lenses.rs DB::add_lens,  *)
(*     write_lens_config, CommittedLensStore::has_transform                *)
(* Go reference: internal/db/view.go.                                      *)
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
  \* How many permission changes other transactions may commit.
  MaxEnv

\* The constants have the shapes they should.
ASSUME /\ CheckEmpty \in BOOLEAN /\ CheckLens \in BOOLEAN
       /\ StoreTransform \in BOOLEAN
       /\ LensAt \in {"txn", "memory"} /\ NacAt \in {"txn", "before"}
       /\ MaxEnv \in Nat

\* What a caller can name: no transform, an empty CID, or the lens "L".
Asks == {"none", "empty", "L"}

VARIABLES
  \* The transform the caller asked for.
  asked,
  \* The lens L's state: "absent", "committed" (config durable, not yet in
  \* memory) or "registered" (also in the in-memory store).
  lens,
  \* Whether the caller holds ViewAdd in the latest commit.
  nac,
  \* Where add_view is: "check", "commit" or "done".
  pc,
  \* The outcome: "pending", "stored", "denied", "empty" or "missing".
  result,
  \* The transform the committed view names ("none" if it names none).
  stored,
  \* The facts at the point the view's transaction decides: the permission
  \* and whether L's config was committed, recorded when add_view ends.
  nacAtDecision, lensAtDecision,
  \* How many permission changes have happened.
  env

vars == <<asked, lens, nac, pc, result, stored, nacAtDecision,
          lensAtDecision, env>>

\* Whether L's config has committed (registered implies committed).
LensCommitted == lens # "absent"

\* Whether the in-memory store holds L.
LensInMemory == lens = "registered"

\* What the lens check sees, at the place it reads.
LensSeen == IF LensAt = "txn" THEN LensCommitted ELSE LensInMemory

\* Every variable holds a value of the shape it should.
TypeOK ==
  /\ asked \in Asks
  /\ lens \in {"absent", "committed", "registered"}
  /\ nac \in BOOLEAN
  /\ pc \in {"check", "commit", "done"}
  /\ result \in {"pending", "stored", "denied", "empty", "missing"}
  /\ stored \in Asks
  /\ nacAtDecision \in BOOLEAN
  /\ lensAtDecision \in BOOLEAN
  /\ env \in 0..MaxEnv

\* Any request, any permission, and L in any state.
Init ==
  /\ asked \in Asks
  /\ lens \in {"absent", "committed", "registered"}
  /\ nac \in BOOLEAN
  /\ pc = "check"
  /\ result = "pending"
  /\ stored = "none"
  /\ nacAtDecision = FALSE
  /\ lensAtDecision = FALSE
  /\ env = 0

\* Ends add_view with outcome `r`, recording the facts at this point.
Finish(r) ==
  /\ result' = r /\ pc' = "done"
  /\ nacAtDecision' = nac /\ lensAtDecision' = LensCommitted

\* The checks that run before the view's transaction opens: NAC if it is
\* read beforehand, the empty CID, and the lens if it is read from memory.
Check ==
  /\ pc = "check"
  /\ CASE NacAt = "before" /\ ~nac -> Finish("denied")
       [] CheckEmpty /\ asked = "empty" -> Finish("empty")
       [] CheckLens /\ LensAt = "memory" /\ asked = "L" /\ ~LensSeen
            -> Finish("missing")
       \* every check that runs here passed: on to the transaction
       [] OTHER -> /\ pc' = "commit"
                   /\ UNCHANGED <<result, nacAtDecision, lensAtDecision>>
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

\* Another task adds L: its config commits first.
LensCommit ==
  /\ lens = "absent"
  /\ lens' = "committed"
  /\ UNCHANGED <<asked, nac, pc, result, stored, nacAtDecision,
                 lensAtDecision, env>>

\* After the commit, the follow-up registers L in the in-memory store.
LensRegister ==
  /\ lens = "committed"
  /\ lens' = "registered"
  /\ UNCHANGED <<asked, nac, pc, result, stored, nacAtDecision,
                 lensAtDecision, env>>

\* Another transaction grants or revokes the caller's ViewAdd permission.
Env ==
  /\ env < MaxEnv
  /\ pc # "done"
  /\ nac' = ~nac
  /\ env' = env + 1
  /\ UNCHANGED <<asked, lens, pc, result, stored, nacAtDecision,
                 lensAtDecision>>

\* Each step is one of add_view's, the lens add's, or a permission change.
Next == Check \/ Commit \/ LensCommit \/ LensRegister \/ Env

\* The whole behaviour: start in Init, take steps from Next.
Spec == Init /\ [][Next]_vars

\* A committed view never names an empty transform.
INV_NoEmptyTransform == stored # "empty"

\* A committed view only names a lens whose config had committed when the
\* view committed.
INV_StoredTransformExists == stored = "L" => lensAtDecision

\* A committed view names exactly the transform the caller asked for.
INV_StoredIsAccepted == result = "stored" => stored = asked

\* A view only commits for a caller holding ViewAdd when it commits.
INV_NoViewWithoutPermission == result = "stored" => nacAtDecision

\* "No such lens" is only said when L's config had not committed: nothing
\* valid is rejected.
INV_NoValidRejected == result = "missing" => ~lensAtDecision
=============================================================================
