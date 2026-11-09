---- MODULE NodeAcpOps ----
\* The story: node access control (NAC) decides who may do what on a node.
\* Once set up it is on or temporarily off; while it is off everyone may do
\* everything, and its relationships wait, kept, for it to come back. A
\* client asks the database for NAC's status, to turn NAC off, to turn it
\* back on, or to give or take someone's relation on the node; or the node
\* restarts. The database does each once, in one place, for every way in
\* (HTTP, the CLI, FFI, the embedded node).
\*
\* Part one, one request at a time (Spec). Before it changes anything the
\* database checks, in Go's order, and stops at the first that fails:
\*   status     nac (the caller's live permission);
\*   disable    configured, already (NAC is not off yet), nac;
\*   re-enable  configured, stored (the caller's permission in the stored
\*              relationships, since the live check lets everyone through
\*              while NAC is off), already (NAC is not on yet);
\*   add        configured, nac, acp (NAC is on), identity (someone acts),
\*              relation (one the node's policy has), target (a DID or *);
\*   delete     the same but target: a target that is not a DID holds no
\*              relationship, so Go finds none to take and refuses nothing.
\* A restart, with or without NAC asked for at start, leaves a set-up NAC as
\* it was: a disabled NAC stays off.
\*
\* Part two, requests racing (RaceSpec). Two clients ask for the same status
\* change at once, while an admin may give or take one client's permission.
\* Each client reads the status (the "already" guard) and its permission (the
\* check) and then commits its write. The store keeps a version of each thing
\* a write can change; a commit whose reads no longer match those versions
\* loses, and the client runs its decision again from the start. A read
\* counts against the commit only when it was taken inside the write's own
\* transaction. The store's one retry function runs every try; when the tries
\* run out the client hears a conflict, which this part leaves out: it models
\* only the decision each try makes.
\*
\* Part three, a relationship change racing a disable (RelSpec). One client
\* adds or removes a relationship while another caller turns NAC off. The
\* client reads the status in its write; a disable that commits first fails
\* that commit, and the decision runs again on what committed.
\*
\* What must never happen: NAC turned off by a caller without the live
\* permission; NAC turned back on by a caller without the stored permission;
\* a relation given or taken while NAC is off, without an identity, for a
\* relation or target the policy does not have, or by a caller without the
\* permission; a status answered to a caller without the permission; a status
\* change on a node where NAC was never set up; a restart that turns a
\* disabled NAC back on; a refusal reported with another check's error than
\* Go's. And when racing: a status change committed by a caller that lost its
\* permission before the commit; two clients both making the same change; a
\* client that lost the race told anything but Go's "already"; a relationship
\* changed after a disable committed, or refused with the store's error
\* instead of Go's "ACP not available".
\*
\* Code it models:
\*   crates/db/src/access/nac_ops/mod.rs
\*     DB::nac_status_as                    the status check
\*     DB::disable_nac_as                   hands the caller to NacManager::disable_as
\*     DB::re_enable_nac_as                 hands the caller to NacManager::re_enable_as
\*     DB::add_nac_actor_relationship_as    parses the request; NacManager::decide_relationship checks it
\*     DB::delete_nac_actor_relationship_as the same checks but target, then the
\*                                          revocation, or "not found" for a non-DID
\*   crates/db/src/nac/toggle.rs
\*     NacManager::toggle_as                the status change, through in_write
\*     NacManager::decide_toggle            the guard and the check in Go's order,
\*                                          then the write, all in one try's scope
\*     NacManager::require                  the live check, or the stored one for re-enable
\*   crates/db/src/nac/write.rs
\*     NacManager::in_write                 one write scope per try; a lost
\*                                          commit runs the decision again
\*     NacManager::decide_relationship      part three: set up, permission, NAC on,
\*                                          then the change, all in one try's scope
\*   crates/acp/src/zanzibar/store/persistent.rs
\*     PersistentZanzibarStore::transact    the tries, through datastore::transact
\*                                          (native only: no wasm client runs NAC)
\*   crates/db/src/nac/mod.rs
\*     NacManager::initialize               the restart
EXTENDS Naturals, Sequences, FiniteSets   \* indexing, counting and set sizes

CONSTANTS
  Enforced,        \* which checks the database really runs
  LiveReEnable,    \* re-enable checks the live permission instead of the stored one
  StartReEnables,  \* a restart turns a disabled NAC back on, as before
  AlreadyFirst,    \* re-enable checks "already on" before the caller
  SplitScope,      \* the permission is read in its own transaction, not the write's
  OutsideGuard,    \* the "already" guard reads the status outside the write
  NoRetry,         \* a lost commit is reported as an error instead of decided again
  RelOutsideGuard  \* a relationship change checks "NAC on" only before its write

\* Only the named checks can be enforced.
ASSUME Enforced \subseteq {"configured", "already", "nac", "stored", "acp",
                           "identity", "relation", "target"}
\* The switches of part one are yes or no.
ASSUME LiveReEnable \in BOOLEAN /\ StartReEnables \in BOOLEAN /\ AlreadyFirst \in BOOLEAN
\* The switches of part two are yes or no.
ASSUME SplitScope \in BOOLEAN /\ OutsideGuard \in BOOLEAN /\ NoRetry \in BOOLEAN

\* ---------------------------------------------------------------------------
\* Part one: one request at a time.

\* Go's checks for each request, in Go's order.
GoOrder(o) ==
  CASE o = "status"   -> <<"nac">>
    [] o = "disable"  -> <<"configured", "already", "nac">>
    [] o = "reenable" -> <<"configured", "stored", "already">>
    [] o = "add"      -> <<"configured", "nac", "acp", "identity", "relation", "target">>
    [] o = "delete"   -> <<"configured", "nac", "acp", "identity", "relation">>
    [] o = "restart"  -> <<>>

\* The order the database runs a request's checks in: Go's, or for re-enable
\* with "already on" moved ahead of the caller's check when the RED switch says.
CheckOrder(o) ==
  IF AlreadyFirst /\ o = "reenable"
    THEN <<"configured", "already", "stored">>
    ELSE GoOrder(o)

VARIABLES
  op,           \* the request: "status", "disable", "reenable", "add", "delete" or a "restart"
  status,       \* NAC before it: "none" (never set up), "on" or "off"
  permitted,    \* the caller holds the permission the request needs
  stored,       \* the stored relationships give the caller that permission
  hasIdentity,  \* someone acts in the request
  relationOk,   \* the relation is one the node's policy has
  targetOk,     \* the target is a DID or *
  step,         \* "check", then "apply", then "done"
  reason,       \* the check the request was refused at, or "none"
  after,        \* NAC after the request
  granted       \* a relationship was given or taken

\* Part one's variables, for saying a step leaves them all as they are.
vars == <<op, status, permitted, stored, hasIdentity, relationOk, targetOk,
          step, reason, after, granted>>

\* The live NAC check: everyone passes while NAC is not on.
Live == status # "on" \/ permitted

\* Whether check `c` passes for this request.
Passes(c) ==
  CASE c = "configured" -> status # "none"
    [] c = "already"    -> IF op = "disable" THEN status # "off" ELSE status # "on"
    [] c = "nac"        -> Live
    \* the stored check, or the live one when the RED switch plants that bug
    [] c = "stored"     -> IF LiveReEnable THEN Live ELSE stored
    [] c = "acp"        -> status = "on"
    [] c = "identity"   -> hasIdentity
    [] c = "relation"   -> relationOk
    [] c = "target"     -> targetOk

\* The positions in `order` of the checks that are enforced and fail.
Failing(order) == {i \in 1..Len(order) : order[i] \in Enforced /\ ~Passes(order[i])}

\* The first enforced check that fails in `order`; "none" if all pass.
First(order) ==
  IF Failing(order) = {} THEN "none"
  \* the failing position no other failing position comes before
  ELSE order[CHOOSE i \in Failing(order) : \A j \in Failing(order) : i <= j]

\* Every variable of part one holds a value of the shape it should.
TypeOK ==
  /\ op \in {"status", "disable", "reenable", "add", "delete", "restart"}
  /\ status \in {"none", "on", "off"}
  /\ permitted \in BOOLEAN /\ stored \in BOOLEAN /\ hasIdentity \in BOOLEAN
  /\ relationOk \in BOOLEAN /\ targetOk \in BOOLEAN
  /\ step \in {"check", "apply", "done"}
  /\ reason \in {"none", "configured", "already", "nac", "stored", "acp",
                 "identity", "relation", "target"}
  /\ after \in {"none", "on", "off"}
  /\ granted \in BOOLEAN

\* Any request against any node can arrive; nothing has changed yet.
Init ==
  /\ op \in {"status", "disable", "reenable", "add", "delete", "restart"}
  /\ status \in {"none", "on", "off"}
  /\ permitted \in BOOLEAN /\ stored \in BOOLEAN /\ hasIdentity \in BOOLEAN
  /\ relationOk \in BOOLEAN /\ targetOk \in BOOLEAN
  /\ step = "check"
  /\ reason = "none"
  /\ after = status
  /\ granted = FALSE

\* The checks run first. The first enforced check that fails ends the
\* request with its error; otherwise the request goes on, even if a check
\* left out would have failed (the bug a RED config plants).
Check ==
  /\ step = "check"
  /\ IF First(CheckOrder(op)) # "none"
       THEN /\ reason' = First(CheckOrder(op)) /\ step' = "done"
       ELSE /\ reason' = reason /\ step' = "apply"
  /\ UNCHANGED <<op, status, permitted, stored, hasIdentity, relationOk,
                 targetOk, after, granted>>

\* The change. Disable turns NAC off and re-enable turns it on, even on a
\* node where NAC was never set up when the check for that is left out. Add
\* changes the relationship; delete does too, unless its target is not a DID,
\* which holds none, so nothing is taken. A status read changes nothing. A
\* restart leaves a set-up NAC as it was, unless the RED switch makes it turn
\* a disabled NAC back on.
Apply ==
  /\ step = "apply"
  /\ step' = "done"
  /\ CASE op = "status"   -> UNCHANGED <<after, granted>>
       [] op = "disable"  -> after' = "off" /\ UNCHANGED granted
       [] op = "reenable" -> after' = "on" /\ UNCHANGED granted
       [] op = "add"      -> granted' = TRUE /\ UNCHANGED after
       [] op = "delete"   -> granted' = targetOk /\ UNCHANGED after
       [] op = "restart"  -> /\ after' = IF StartReEnables /\ status = "off"
                                            THEN "on" ELSE status
                             /\ UNCHANGED granted
  /\ UNCHANGED <<op, status, permitted, stored, hasIdentity, relationOk,
                 targetOk, reason>>

\* Each step of a request is one of the two.
Next == Check \/ Apply

\* A finished request stays finished.
Done == step = "done" /\ UNCHANGED vars

\* ---------------------------------------------------------------------------
\* Part two: two clients racing for the same status change.

\* The two clients.
Clients == {"c1", "c2"}

\* Marks a read taken outside the write's transaction: it never fails a commit.
Unchecked == 99

VARIABLES
  rop,        \* the change both clients ask for: "disable" or "reenable"
  nac,        \* the committed status: "on" or "off" (NAC is set up)
  startNac,   \* the committed status before anyone moved
  grant,      \* grant[c]: the committed relationships give c the permission
  startGrant, \* grant as it was before anyone moved
  touched,    \* an admin already gave or took a permission (once at most)
  sver,       \* the status's version: one more on each committed status change
  gver,       \* gver[c]: c's permission's version: one more on each grant or revocation
  pc,         \* pc[c]: what c does next: "guard", "check", "commit" or "done"
  readS,      \* readS[c]: the status version c read in its write, or Unchecked
  readG,      \* readG[c]: the permission version c read in its write, or Unchecked
  outcome,    \* outcome[c]: "none" yet, "ok", "already", "denied", or "lost" (an error)
  heldAt      \* heldAt[c]: whether c held the permission when its write committed

\* Part two's variables, for saying a step leaves them all as they are.
raceVars == <<rop, nac, startNac, grant, startGrant, touched, sver, gver, pc,
              readS, readG, outcome, heldAt>>

\* Every variable of both parts.
allVars == <<vars, raceVars>>

\* The status a change starts from.
From(o) == IF o = "disable" THEN "on" ELSE "off"
\* The status a change leaves.
To(o) == IF o = "disable" THEN "off" ELSE "on"

\* The first thing a client does, in Go's order: disable guards first, and
\* re-enable checks the caller first.
FirstStep(o) == IF o = "disable" THEN "guard" ELSE "check"

\* What a client does after step `s`: the other read, then the commit.
After(o, s) ==
  IF o = "disable"
    THEN IF s = "guard" THEN "check" ELSE "commit"
    ELSE IF s = "check" THEN "guard" ELSE "commit"

\* Every variable of part two holds a value of the shape it should.
RaceTypeOK ==
  /\ rop \in {"disable", "reenable"}
  /\ nac \in {"on", "off"} /\ startNac \in {"on", "off"}
  /\ grant \in [Clients -> BOOLEAN] /\ startGrant \in [Clients -> BOOLEAN]
  /\ touched \in BOOLEAN
  /\ sver \in Nat /\ gver \in [Clients -> Nat]
  /\ pc \in [Clients -> {"guard", "check", "commit", "done"}]
  /\ readS \in [Clients -> Nat] /\ readG \in [Clients -> Nat]
  /\ outcome \in [Clients -> {"none", "ok", "already", "denied", "lost"}]
  /\ heldAt \in [Clients -> BOOLEAN]

\* Both clients ask for the same change against either status, each holding
\* its permission or not; nothing is read or written yet.
RaceInit ==
  /\ rop \in {"disable", "reenable"}
  /\ nac \in {"on", "off"}
  /\ startNac = nac
  /\ grant \in [Clients -> BOOLEAN]
  /\ startGrant = grant
  /\ touched = FALSE
  /\ sver = 0
  /\ gver = [c \in Clients |-> 0]
  /\ pc = [c \in Clients |-> FirstStep(rop)]
  /\ readS = [c \in Clients |-> Unchecked]
  /\ readG = [c \in Clients |-> Unchecked]
  /\ outcome = [c \in Clients |-> "none"]
  /\ heldAt = [c \in Clients |-> FALSE]

\* Client c stops with result `r`.
Finish(c, r) ==
  /\ outcome' = [outcome EXCEPT ![c] = r]
  /\ pc' = [pc EXCEPT ![c] = "done"]

\* The guard: c reads the status. Already in the state asked for, c stops
\* with Go's "already". Otherwise it notes the version it read, as part of
\* its write, unless the RED switch moves this read outside the write.
Guard(c) ==
  /\ pc[c] = "guard"
  /\ IF nac = To(rop)
       THEN /\ Finish(c, "already")
            /\ UNCHANGED <<readS, readG>>
       \* the version read, or Unchecked when read outside the write
       ELSE /\ readS' = [readS EXCEPT ![c] = IF OutsideGuard THEN Unchecked ELSE sver]
            /\ pc' = [pc EXCEPT ![c] = After(rop, "guard")]
            /\ UNCHANGED <<outcome, readG>>
  /\ UNCHANGED <<rop, nac, startNac, grant, startGrant, touched, sver, gver, heldAt>>

\* The check: c reads its permission. Without it, c stops, not authorized.
\* Otherwise it notes the version it read, as part of its write, unless the
\* RED switch reads it in a transaction of its own.
Authorize(c) ==
  /\ pc[c] = "check"
  /\ IF ~grant[c]
       THEN /\ Finish(c, "denied")
            /\ UNCHANGED <<readS, readG>>
       \* the version read, or Unchecked when read in its own transaction
       ELSE /\ readG' = [readG EXCEPT ![c] = IF SplitScope THEN Unchecked ELSE gver[c]]
            /\ pc' = [pc EXCEPT ![c] = After(rop, "check")]
            /\ UNCHANGED <<outcome, readS>>
  /\ UNCHANGED <<rop, nac, startNac, grant, startGrant, touched, sver, gver, heldAt>>

\* A read c took in its write no longer matches what is committed.
Stale(c) ==
  \/ readS[c] # Unchecked /\ readS[c] # sver
  \/ readG[c] # Unchecked /\ readG[c] # gver[c]

\* The commit. A stale read loses the commit: c forgets its reads and starts
\* its decision again, or with the RED switch stops with an error. Otherwise
\* the status changes, its version moves on, and c remembers whether it held
\* its permission at that moment.
Commit(c) ==
  /\ pc[c] = "commit"
  /\ IF Stale(c)
       THEN /\ IF NoRetry
                 THEN Finish(c, "lost")
                 ELSE /\ pc' = [pc EXCEPT ![c] = FirstStep(rop)]
                      /\ UNCHANGED outcome
            \* the next attempt reads afresh
            /\ readS' = [readS EXCEPT ![c] = Unchecked]
            /\ readG' = [readG EXCEPT ![c] = Unchecked]
            /\ UNCHANGED <<nac, sver, heldAt>>
       ELSE /\ nac' = To(rop)
            /\ sver' = sver + 1
            /\ heldAt' = [heldAt EXCEPT ![c] = grant[c]]
            /\ Finish(c, "ok")
            /\ UNCHANGED <<readS, readG>>
  /\ UNCHANGED <<rop, startNac, grant, startGrant, touched, gver>>

\* An admin gives or takes client c's permission, once at most. The store
\* refuses relationship writes while NAC is off, so only while it is on.
Regrant(c) ==
  /\ ~touched
  /\ nac = "on"
  /\ grant' = [grant EXCEPT ![c] = ~grant[c]]
  /\ gver' = [gver EXCEPT ![c] = gver[c] + 1]
  /\ touched' = TRUE
  /\ UNCHANGED <<rop, nac, startNac, startGrant, sver, pc, readS, readG,
                 outcome, heldAt>>

\* Each step of the race is a client's guard, check or commit, or an admin's
\* change, interleaved in any order.
RaceNext == \E c \in Clients : Guard(c) \/ Authorize(c) \/ Commit(c) \/ Regrant(c)

\* Once both clients are done, the race stays as it ended.
RaceDone == (\A c \in Clients : pc[c] = "done") /\ UNCHANGED raceVars

\* Part two held still while part one runs: every value fixed.
RaceIdle ==
  /\ rop = "disable" /\ nac = "on" /\ startNac = "on"
  /\ grant = [c \in Clients |-> FALSE] /\ startGrant = grant
  /\ touched = FALSE /\ sver = 0 /\ gver = [c \in Clients |-> 0]
  /\ pc = [c \in Clients |-> "done"]
  /\ readS = [c \in Clients |-> Unchecked] /\ readG = [c \in Clients |-> Unchecked]
  /\ outcome = [c \in Clients |-> "none"] /\ heldAt = [c \in Clients |-> FALSE]

\* Part one held still while part two runs: every value fixed.
RequestIdle ==
  /\ op = "restart" /\ status = "none"
  /\ permitted = FALSE /\ stored = FALSE /\ hasIdentity = FALSE
  /\ relationOk = FALSE /\ targetOk = FALSE
  /\ step = "done" /\ reason = "none" /\ after = "none" /\ granted = FALSE

\* ---------------------------------------------------------------------------
\* Part three: a relationship change racing a disable.

VARIABLES
  relNac,   \* the committed status the relationship write sees: "on" or "off"
  relVer,   \* the status's version: one more when the disable commits
  relPc,    \* what the relationship client does next: "guard", "commit" or "done"
  relRead,  \* the status version it read in its write, or Unchecked
  relOut,   \* "none" yet, "ok", "off" (Go's "ACP not available"), or "store"
            \* (the store's own refusal, which is not Go's error)
  relHeld,  \* whether NAC was on when the change committed
  disabled  \* the disable already committed (once at most)

\* Part three's variables, for saying a step leaves them all as they are.
relVars == <<relNac, relVer, relPc, relRead, relOut, relHeld, disabled>>

\* Every variable of part three holds a value of the shape it should.
RelTypeOK ==
  /\ relNac \in {"on", "off"} /\ relVer \in Nat
  /\ relPc \in {"guard", "commit", "done"} /\ relRead \in Nat
  /\ relOut \in {"none", "ok", "off", "store"}
  /\ relHeld \in BOOLEAN /\ disabled \in BOOLEAN

\* NAC is on, the client is about to check it, and nothing has committed.
RelInit ==
  /\ relNac = "on" /\ relVer = 0 /\ relPc = "guard" /\ relRead = Unchecked
  /\ relOut = "none" /\ relHeld = FALSE /\ disabled = FALSE

\* The guard: the client reads the status. NAC off, it stops with Go's error.
\* Otherwise it notes the version it read, as part of its write, unless the
\* RED switch reads it only before the write (the pre-check nac_ops once had).
RelGuard ==
  /\ relPc = "guard"
  /\ IF relNac = "off"
       THEN /\ relOut' = "off" /\ relPc' = "done"
            /\ UNCHANGED relRead
       \* the version read, or Unchecked when read outside the write
       ELSE /\ relRead' = IF RelOutsideGuard THEN Unchecked ELSE relVer
            /\ relPc' = "commit"
            /\ UNCHANGED relOut
  /\ UNCHANGED <<relNac, relVer, relHeld, disabled>>

\* The commit. A stale read loses it and the client decides again. A write
\* that reaches the store while NAC is off is refused by the store with its
\* own error. Otherwise the change commits, noting whether NAC was on.
RelCommit ==
  /\ relPc = "commit"
  /\ IF relRead # Unchecked /\ relRead # relVer
       THEN /\ relPc' = "guard" /\ relRead' = Unchecked
            /\ UNCHANGED <<relOut, relHeld>>
       ELSE IF relNac = "off"
              THEN /\ relOut' = "store" /\ relPc' = "done"
                   /\ UNCHANGED <<relRead, relHeld>>
              ELSE /\ relOut' = "ok" /\ relPc' = "done"
                   /\ relHeld' = TRUE
                   /\ UNCHANGED relRead
  /\ UNCHANGED <<relNac, relVer, disabled>>

\* Another caller turns NAC off, once, at any moment.
RelDisable ==
  /\ ~disabled /\ relNac = "on"
  /\ relNac' = "off" /\ relVer' = relVer + 1 /\ disabled' = TRUE
  /\ UNCHANGED <<relPc, relRead, relOut, relHeld>>

\* Each step is the client's guard or commit, or the disable.
RelNext == RelGuard \/ RelCommit \/ RelDisable

\* Once the client is done, part three stays as it ended.
RelDone == relPc = "done" /\ UNCHANGED relVars

\* Part three held still while another part runs: every value fixed.
RelIdle ==
  /\ relNac = "on" /\ relVer = 0 /\ relPc = "done" /\ relRead = Unchecked
  /\ relOut = "none" /\ relHeld = FALSE /\ disabled = FALSE

\* Part one: a request starts in Init and then only checks, applies or stays
\* finished, while the other parts hold still.
Spec == Init /\ RaceIdle /\ RelIdle
        /\ [][(Next \/ Done) /\ UNCHANGED <<raceVars, relVars>>]_<<allVars, relVars>>

\* Part two: the race starts in RaceInit and moves one step at a time, while
\* the other parts hold still.
RaceSpec == RaceInit /\ RequestIdle /\ RelIdle
            /\ [][(RaceNext \/ RaceDone) /\ UNCHANGED <<vars, relVars>>]_<<allVars, relVars>>

\* Part three: the relationship change and the disable, while the other parts
\* hold still.
RelSpec == RelInit /\ RequestIdle /\ RaceIdle
           /\ [][(RelNext \/ RelDone) /\ UNCHANGED allVars]_<<allVars, relVars>>

\* ---------------------------------------------------------------------------
\* Part one's promises.

\* A status is answered only to a caller with the live permission.
INV_NoStatusWithoutPermission ==
  (op = "status" /\ step = "done" /\ reason = "none") => Live

\* NAC is turned off only by a caller with the live permission.
INV_NoDisableWithoutPermission ==
  (op = "disable" /\ status = "on" /\ after = "off") => permitted

\* NAC is turned back on only by a caller with the stored permission.
INV_NoReEnableWithoutStoredPermission ==
  (op = "reenable" /\ status = "off" /\ after = "on") => stored

\* A relation is given or taken only while NAC is on, by a permitted caller
\* with an identity, for a relation and a target the policy has.
INV_NoGrantUnlessAllChecksPass ==
  granted => (status = "on" /\ permitted /\ hasIdentity /\ relationOk /\ targetOk)

\* A node where NAC was never set up keeps it so.
INV_NoChangeUnlessConfigured == status = "none" => after = "none"

\* A restart leaves NAC as it was.
INV_RestartKeepsStatus == op = "restart" => after = status

\* A disable or re-enable that was not refused changed the status: one that
\* would change nothing is refused with Go's "already" error.
INV_NoSilentNoOp ==
  (step = "done" /\ reason = "none" /\ op \in {"disable", "reenable"}) => after # status

\* A refused request reports the first failing check in Go's order.
INV_GoOrder == (step = "done" /\ reason # "none") => reason = First(GoOrder(op))

\* ---------------------------------------------------------------------------
\* Part two's promises.

\* A client changes the status only if it still holds its permission when
\* the change commits: a revocation that commits first wins.
INV_RaceNoChangeWithoutPermission ==
  \A c \in Clients : outcome[c] = "ok" => heldAt[c]

\* Of two clients racing for the same change, at most one makes it.
INV_RaceOneWinner == Cardinality({c \in Clients : outcome[c] = "ok"}) <= 1

\* When no permission changed and both clients could make the change, once
\* both are done one made it and the other heard Go's "already".
INV_RaceLoserHearsAlready ==
  ( /\ \A c \in Clients : pc[c] = "done"
    /\ ~touched
    /\ startNac = From(rop)
    /\ \A c \in Clients : startGrant[c] )
  => {outcome[c] : c \in Clients} = {"ok", "already"}

\* ---------------------------------------------------------------------------
\* Part three's promises.

\* A relationship change commits only while NAC is on.
INV_RelNoChangeWhileOff == relOut = "ok" => relHeld

\* A relationship change that lost to a disable hears Go's "ACP not
\* available", never the store's own refusal.
INV_RelOffHearsGo == relOut # "store"
====
