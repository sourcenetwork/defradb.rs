---- MODULE AcpOutbox ----
\* The story, in plain words.
\*
\* A node keeps documents and asks an access-control provider who may read
\* them. A local provider writes inside the row transaction. A remote one
\* (Vera, a chain) cannot: its state changes only when a signed request lands
\* on it, some time after the rows committed. So the node writes down what it
\* owes the provider, in the same transaction as the rows, and pays later.
\*
\* Two kinds of debt:
\*   * an owed registration: "this document's creator becomes its owner";
\*   * an owed relationship change: "add" or "delete" one tuple (a subject
\*     that may read the document). A subject is one identity, or "*", which
\*     stands for everyone.
\* One document is modelled. Its creator is "c". "g" is an identity the
\* creator may grant, "x" is a stranger nobody names, "m" is an actor with no
\* authority over the document, and "o" is another owner the provider may
\* already hold for the same document.
\*
\* Who acts:
\*   * transactions: one at a time, a transaction creates the document and
\*     stages changes, then commits or rolls back. Each staged change takes
\*     the next sequence number. Before a change is staged the node checks
\*     that its actor may make it: the creator of a document still owed, or
\*     the owner the provider names. A change that fails is not staged. When
\*     the provider cannot be asked, an add is staged unchecked and a delete
\*     is not staged at all;
\*   * the node may crash and restart at any point: an open transaction is
\*     gone, and so is everything the settlement loop only remembered;
\*   * the provider applies a request, refuses one from an actor that does
\*     not own the document, keeps a receipt of either, and is sometimes
\*     unavailable;
\*   * the settlement loop (the redrive, the drainer) registers the document,
\*     then submits the changes in sequence order. Before it sends a change it
\*     marks the record "in flight"; after a send it asks for the receipt and
\*     sends again only when the provider has none. Through a provider with
\*     a journal (Journaled) the mark holds the request's digest, then its
\*     transaction hash, and the journal, a receipt or the provider's note
\*     decides a marked change after a crash; changes of one transaction may
\*     share one request, applied together or not at all, and a refused one
\*     is sent again one change at a time. A change the provider
\*     refuses is parked if its actor was checked at staging, and removed if
\*     it was not. A parked change holds the later changes of its tuple if
\*     its actor was checked. An unchecked one that waits for authority
\*     holds nothing, and is removed when a later change of its tuple is
\*     sent, so it can never land after the change that passed it;
\*   * authority: a change is signed with a key the node holds, or with a
\*     stored delegation that can expire. A change whose delegation expired
\*     waits for a fresh one, which only its actor may hand in. The owed
\*     registration records its authority the same way: the node's key for
\*     the creator, the delegation the create carried, or nothing. It is sent
\*     with that and with nothing else. One with nothing usable is parked
\*     and waits for a fresh credential of its creator;
\*   * a submission of the registration can fail after the commit (the
\*     provider is away, the delegation ran out). The commit stands and the
\*     record says why;
\*   * the actor that staged a change the provider refused may abandon it:
\*     the record is deleted and the change counts for nothing any more;
\*   * readers ask the node for access. The node answers from the provider's
\*     state and from what it still owes (the overlay).
\*
\* The overlay, between commit and settlement:
\*   * while the registration is owed, only the creator reads the document,
\*     and an identity the creator itself granted by name in an owed add;
\*   * any other owed add grants nothing: the grantee waits for the provider;
\*   * an owed delete denies its subject from the commit on, also when the
\*     provider refused it, until the provider has applied it.
\*
\* What must never happen:
\*   1. the node honours an access that the committed changes, all applied,
\*      would not give, or that the provider will not give once everything
\*      is settled - INV_NoWidening;
\*   2. after a delete committed and no later change added the tuple again,
\*      the node honours an access that rests on that tuple
\*      - INV_RevokedAtCommit;
\*   3. a change is sent while the document's registration is still owed
\*      - INV_RegistrationFirst;
\*   4. one change takes effect twice on the provider - INV_AtMostOneEffect;
\*   5. something stays owed for a transaction that rolled back
\*      - INV_RollbackLeavesNothing;
\*   6. while the registration is owed, anyone reads the document but its
\*      creator and the identities the creator named - INV_OwedDocumentNarrow;
\*   7. a committed change stays unsettled for ever though the provider keeps
\*      coming back and fresh authority keeps being supplied, unless it waits
\*      behind a parked change of its tuple - LIVE_Drains;
\*   8. a delete the node stores, and so denies by, was staged without its
\*      actor being checked - INV_DenialsChecked;
\*   9. a change staged unchecked is parked as refused, where it would hold
\*      its tuple - INV_RefusedChecked;
\*  10. a parked change nobody checked holds a later change of its tuple
\*      - INV_UncheckedYields;
\*  11. the registration reaches the provider on anything but its creator's
\*      word: what its record holds, a delegation recorded with a change the
\*      creator still owes, or the credential a request carries for the
\*      creator - INV_PresentedAuthority;
\*  12. a registration whose submission failed stays owed without saying so,
\*      or a change is deleted without the provider holding it and its actor
\*      is told nothing - INV_FailureReported;
\*  13. a change lands on the provider after it was abandoned
\*      - INV_AbandonIsFinal;
\*  14. a reader a committed delete denies reads again because someone other
\*      than the delete's actor abandoned it - INV_RevokedAtCommit;
\*  15. the registration is sent under a credential that is not its
\*      creator's, and the provider registers that signer - INV_NoWidening;
\*  16. a change is parked as refused because another change it shared a
\*      request with was refused - INV_RefusedOnItsOwn;
\*  17. a change a client staged over HTTP acts as a DID other than the one
\*      its verified credential names - INV_ActsAsItsCredential;
\*  18. a change a client staged over HTTP is owed with no authority the
\*      node can send it with: neither the node's key for its actor nor a
\*      delegation the client presented - INV_SendableAuthority.
\*
\* What exists in the code today, and what is specification ahead of code.
\* Exists (the registry anchors these):
\*   the owed registration and its key      crates/engine/acp/src/pending.rs
\*                                          (PendingRegistration,
\*                                          PendingRegistrations::record);
\*   recording it in the create transaction crates/database/db/src/write/autocommit/
\*                                          helpers.rs register_created_doc;
\*   creator-only until settled             crates/engine/acp/src/external.rs
\*                                          ExternalDocumentAcp::check_doc_access,
\*                                          for a collection that does not gate
\*                                          creates. A gated one is hidden from
\*                                          its creator too until settled, a
\*                                          refusal of its create is final and
\*                                          never resent, and a replica refused
\*                                          as a create is parked after a bound:
\*                                          proofs/tla/CreateGate.tla models
\*                                          those (PendingRegistration::hidden,
\*                                          create_refused, note_failure,
\*                                          parked_create). Hidden is narrower
\*                                          than creator-only, so
\*                                          INV_OwedDocumentNarrow holds for it;
\*   register, then clear                   settle, clear in crates/engine/acp/src/
\*                                          external/registration.rs;
\*   settling after the commit              DbTxn::owe_acp_registration in
\*                                          crates/database/db/src/txn/mod.rs,
\*                                          CommitRegistrations::owe in
\*                                          crates/engine/acp/src/commit_registrations.rs,
\*                                          DocumentACP::redrive_registrations
\*                                          (several at once: RegistrationBatch.tla);
\*   the redrive at start                   ExternalDocumentAcp::redrive_owed,
\*                                          started by gate_external_acp in
\*                                          crates/entry-points/defra-node/src/node_acp.rs;
\*   staging a change in its transaction    crates/engine/acp/src/outbox/staging.rs
\*                                          TransactionOutbox::stage, over
\*                                          crates/engine/acp/src/outbox/ops.rs stage,
\*                                          called by relationship_in_txn in
\*                                          crates/entry-points/defra-node/src/acp_ops/
\*                                          transactional.rs;
\*   the deny overlay on committed state    crates/engine/acp/src/outbox/deny.rs
\*                                          owed_denial, read by
\*                                          ExternalDocumentAcp::revoked;
\*   the settlement pass                    crates/engine/acp/src/external/settle.rs
\*                                          settle_owed_changes_once,
\*                                          settle_change, submittable,
\*                                          register_first;
\*   parking a change                       crates/engine/acp/src/outbox/ops.rs mark;
\*   what a registration is sent with       crates/engine/acp/src/pending.rs
\*                                          PendingRegistration::authority,
\*                                          recorded by register_created_doc and
\*                                          used by ExternalDocumentAcp::
\*                                          settle_with and registrar (crates/
\*                                          acp/src/external/registration.rs), which
\*                                          also keep the reason of a failure
\*                                          (note_failure);
\*   the note of a discarded change         crates/engine/acp/src/outbox/ops.rs discard,
\*                                          acknowledge;
\*   the report, a fresh credential and     crates/engine/acp/src/external/owed.rs
\*   abandoning                             entries, reauthorize,
\*                                          abandon_change, over
\*                                          crates/engine/acp/src/outbox/ops.rs
\*                                          reauthorize and settle, behind the
\*                                          node calls in
\*                                          crates/entry-points/defra-node/src/node_core/owed.rs;
\*   the settlement loop on demand: a       crates/entry-points/defra-node/src/node_core/
\*   host's call, and the node once its     owed.rs send_owed_acp (a pass over the
\*   embedded Vera device is active         objects one actor staged changes on,
\*                                          the first MAX_SCOPED_PASS_OBJECTS,
\*                                          OwedScope::Actor) and send_all_owed_acp
\*                                          (redrive_owed_registrations, then a
\*                                          pass over all), called by
\*                                          crates/entry-points/defra-node/src/
\*                                          embedded_vera_switch.rs;
\*   the drainer task, woken by commits     crates/entry-points/defra-node/src/
\*   and backing off while the provider     owed_drainer.rs drain, started by
\*   is away                                gate_external_acp;
\*   the journaled request and its marks    crates/engine/acp/src/external/drain.rs
\*                                          send_request, recover_marked, batch,
\*                                          over crates/engine/acp/src/outbox/
\*                                          ops.rs mark_in_flight, clear_in_flight
\*                                          and the record's InFlight;
\*   the worker's journal, the receipt by   crates/engine/vera/src/vera_rs/provider/
\*   hash and the note                      owed_request.rs prepare_owed_request,
\*                                          journal_owed, owed_outcome, with the
\*                                          note in crates/engine/vera/src/vera_rs/
\*                                          outcome_note.rs, written by
\*                                          resolve_pending in provider/
\*                                          submission.rs for the start-up recovery
\*                                          (recover_pending) and every command that
\*                                          finds another's request journaled.
\* In the model these are StageCreate, StageChange with its check (CanCheck,
\* Passes, CheckedDeletes), Commit, Rollback, Crash, RegisterOnChain,
\* ClearOwed, the deny overlay (DenyRule = "AnyOwedDelete",
\* RefusedDeleteDenies), and the rules RegFirst, InOrder, ParkedBlocks,
\* DropUnchecked and UncheckedYields = "drop", the registration's record
\* (RegAuths, RegValid, RegistrationFails, ReauthorizeReg with ReauthChecked,
\* RegRetry = "recorded", the three ways of MaySendBy), the notes of
\* discarded changes (Noted, Acknowledge) and Abandon with
\* AbandonStates = "refused" and
\* AbandonBy = "actor". Through a provider with a journal (vera.rs) the
\* drainer is JMark, JPrepare, JHash, JLand and JResolve, with JRecover for
\* the provider at start and every other command that finds the request
\* still journaled, JDrop for a commit that stops waiting, and JAdopt and
\* JDecide for a change found marked: the configurations with
\* Journaled = TRUE. Through a provider without one (a host's own, the test
\* providers) a pass that submits a change and reads the provider's answer
\* is Mark, Land and Lookup taken together with nothing stored in between,
\* which is the model with Marker = FALSE: MC_AcpOutbox_Green_NoMarker holds
\* every property but INV_AtMostOneEffect, and
\* MC_AcpOutbox_Red_NoInFlightMarker is the trace of what such a provider
\* sees after a crash (the change is sent again). A change without usable
\* authority is parked as AwaitAuthority parks it, checked or not: the
\* provider has not refused it.
\* Where the code differs from the model:
\*   * it honours no owed add at all, not even the creator's own named grant
\*     (OwedAdds has no such value; what the code honours is a subset of
\*     "CreatorExplicit", so every property about not widening carries over);
\*   * its check also lets through an actor that holds a managing relation
\*     on the provider; the model knows only the owner;
\*   * through a provider without a journal it parks a change as waiting for
\*     authority without knowing whether a request sent for it earlier is
\*     still on the way (a commit that stopped waiting leaves one).
\*     AwaitAuthority here parks only a change known not to have landed. That
\*     is why the code lets only a refused change be abandoned, and a change
\*     waiting for authority only be reauthorized. Through a journal a marked
\*     change is decided before anything else is done with it;
\*   * the model's transaction is the first slot it staged; the code batches
\*     changes that carry the same transaction and run, one after the other
\*     in the queue, and never more than fit one request;
\*   * the provider's note is bounded: an entry pushed out is a note that can
\*     no longer be read, which ReceiptLost covers;
\*   * a registration the provider refuses outright is kept as refused with
\*     the provider's words and tried again at start; the model has no
\*     provider that refuses a registration.
\* Specification ahead of code (#2954): the creator's owed grants, and the
\* fairness of SpecLive for a provider without a journal, whose changes
\* the drainer submits without a mark.
\*
\* Assumptions, by name. Each is a switch, and a RED configuration turns it
\* off to show what rests on it.
\*   AssumeReceiptRetrievable   after a change landed, its receipt, the
\*       worker's journal entry or the provider's note of it can still be read
\*       when the loop asks. When none is, the loop cannot tell "landed" from
\*       "lost", sends again, and the change takes effect twice.
\*       INV_AtMostOneEffect holds only under this assumption
\*       (MC_AcpOutbox_Red_ReceiptLost, MC_AcpOutbox_Red_JournalReceiptLost).
\*   AssumeUncontested          the provider does not already hold the
\*       document under another owner. When it does, settle keeps that owner,
\*       and the creator read a document the provider gives to someone else.
\*   AssumeOwnerStays           the provider's owner of the document does
\*       not change after the node checked a change against it. When it
\*       does, a change that passed the staging check is refused at
\*       submission. Only then can a checked change be parked as refused in
\*       an uncontested document, which the RED configurations about
\*       refused changes need.
\*   AssumeNoTransferWhileOwed  the document does not change owner on the
\*       provider while this node still holds its owed registration. The
\*       provider registers the creator and the node clears its record in
\*       two steps. A transfer between them (only the creator can make it,
\*       since it owns the document then) leaves the node letting the
\*       creator read, by the owed-registration rule, a document the
\*       provider now gives to the new owner alone, until the record is
\*       cleared. INV_NoWidening holds only under this assumption once the
\*       owner can change.
\* Assumed without a switch:
\*   * one request sent twice is executed once (the worker journal and the
\*     provider's sequence, PendingSubmission.tla). A second effect here is a
\*     second, newly prepared request;
\*   * transactions commit one after another, so sequence order is commit
\*     order. In the code that holds per tuple only (two transactions that
\*     stage one tuple conflict); the pass reads the queue from its head
\*     each time and orders one tuple's changes by the tuple's own record;
\*   * nobody else changes this document on the provider;
\*   * an unregistered document is closed on the provider (Vera's rule).
\*
\* Not modelled: several documents, deleting the document, the read inside
\* the open transaction (DeferredAcp.tla), a fresh credential for a refused
\* change (the code takes one and submits the change again), the
\* registration-with-grants batch (without it a creator's grantee
\* loses access between ClearOwed and the grant landing: narrower, never
\* wider), limits on how much may be owed, a stale provider answer
\* (Acp.tla), a change sent to the provider outside a transaction while a
\* change of its tuple stays parked (it overtakes the parked one), and a
\* provider answer that does not say whether it is final (the code takes
\* the same such answer, several times in a row, as a refusal).

EXTENDS Naturals, FiniteSets  \* numbers and finite sets

CONSTANTS
  MaxChanges,                \* how many sequence numbers exist
  Subjects,                  \* the subjects a change may name: "g", "*"
  Actors,                    \* who may stage a change: "c", "m"
  Auths,                     \* how a change is signed: "node", "deleg"
  Preexisting,               \* TRUE: the document is already registered
  InitTuples,                \* the tuples the provider holds at the start
  MaxCrashes,                \* how often the node may crash
  MaxExpiries,               \* how often the stored delegation may expire
  TxnOwnsOwed,               \* TRUE: owed records are the transaction's own writes
  DenyRule,                  \* which owed deletes deny: see DenyTuple
  RefusedDeleteDenies,       \* TRUE: a refused delete keeps denying
  OwedAdds,                  \* which owed adds grant: see Grants
  RegFirst,                  \* TRUE: no change is sent while the registration is owed
  InOrder,                   \* TRUE: changes are sent in sequence order
  ParkedBlocks,              \* TRUE: a parked change holds its tuple
  DrainPastRefused,          \* TRUE: a refused change does not hold the rest
  Marker,                    \* TRUE: the in-flight mark survives a crash
  CheckedDeletes,            \* TRUE: a delete is staged only with its actor checked
  DropUnchecked,             \* TRUE: an unchecked change the provider refuses is removed
  UncheckedYields,           \* what a parked unchecked change does to its tuple: see Holds
  AssumeReceiptRetrievable,  \* TRUE: a landed change's receipt can be read
  AssumeUncontested,         \* TRUE: the provider holds no other owner
  AssumeOwnerStays,          \* TRUE: the provider's owner does not change
  AssumeNoTransferWhileOwed, \* TRUE: no owner change while the registration is owed here
  RegAuths,                  \* what a registration may record to be sent with: "node", "deleg", "none"
  RegRetry,                  \* what a retry of the registration is sent with: see RegisterOnChain
  ReauthChecked,             \* TRUE: a fresh credential is taken only from the creator
  ReportFailures,            \* TRUE: a submission that failed leaves its reason in the record
  AbandonStates,             \* which changes may be abandoned: see Abandon
  AbandonBy,                 \* who may abandon a change: see Abandon
  Journaled,                 \* TRUE: changes go through the worker's journal (the drainer)
  Note,                      \* TRUE: a request resolved by someone else leaves a note of its outcome
  Floor,                     \* TRUE: a note found by digest counts only from the mark's sequence on
  Captured,                  \* TRUE: a journaled request lands under the credential it carries
  Remote,                    \* TRUE: a client stages changes over HTTP under its own credential
  ActorFromCredential,       \* TRUE: a client's change acts as the DID its credential names
  RemoteCheck,               \* TRUE: a client's change is checked at staging as an embedder's is
  RemoteAuthority,           \* TRUE: a client's change is staged only under an authority the node can send it with
  HeldKeys,                  \* the actors whose keys the node holds: it signs as them itself
  MaxBatch,                  \* how many changes one request may carry
  Isolate                    \* TRUE: a refused batch is sent again one change at a time

ASSUME MaxChanges \in Nat /\ MaxCrashes \in Nat /\ MaxExpiries \in Nat  \* bounds are numbers
ASSUME Subjects \subseteq {"g", "*"} /\ Subjects # {}       \* known subjects
ASSUME Actors \subseteq {"c", "m"} /\ Actors # {}           \* known actors
ASSUME Auths \subseteq {"node", "deleg"} /\ Auths # {}      \* known ways to sign
ASSUME InitTuples \subseteq Subjects                        \* tuples name subjects
ASSUME Preexisting \/ InitTuples = {}                       \* a new document has no tuples
ASSUME Preexisting => AssumeUncontested                     \* an existing one is the creator's
ASSUME DenyRule \in {"AnyOwedDelete", "LatestOp", "None"}   \* a known deny rule
ASSUME OwedAdds \in {"CreatorExplicit", "CreatorAny", "Any"}  \* a known grant rule
ASSUME UncheckedYields \in {"drop", "pass", "hold"}       \* a known yielding rule
ASSUME RegAuths \subseteq {"node", "deleg", "none", "ambient"} /\ RegAuths # {}  \* known authorities
ASSUME RegRetry \in {"recorded", "ambient"}                \* a known retry rule
ASSUME AbandonStates \in {"refused", "any"}                \* a known abandon rule
ASSUME AbandonBy \in {"actor", "anyone"}                   \* a known abandon rule
ASSUME Journaled \in BOOLEAN /\ Note \in BOOLEAN /\ Isolate \in BOOLEAN  \* switches
ASSUME Floor \in BOOLEAN /\ Captured \in BOOLEAN                \* switches
ASSUME Remote \in BOOLEAN /\ ActorFromCredential \in BOOLEAN /\ RemoteCheck \in BOOLEAN  \* switches
ASSUME RemoteAuthority \in BOOLEAN                         \* a switch
ASSUME HeldKeys \subseteq Actors                            \* the node holds keys of known actors only
ASSUME MaxBatch \in {1, 2}                                 \* one change a request, or two

Slots == 1..MaxChanges       \* the sequence numbers
Readers == {"c", "g", "x"}   \* who asks for access: creator, grantee, stranger
Presenters == {"c", "o"}     \* who may hand the node a fresh credential for the registration
Abandoners == Actors \cup {"g"}  \* who may ask to abandon a change: an actor, or the grantee
Shown == {"deleg", "token"}  \* what a client signs in to HTTP with: a delegation, or an identity token

\* A slot nobody staged holds this placeholder.
NoChange == [subj |-> "-", op |-> "-", actor |-> "-", auth |-> "-"]
\* A slot never marked names no request.
NoDigest == [c |-> <<>>, g |-> 0]
\* Everything a transaction may stage: a subject, add or delete, who asks,
\* and how the request will be signed.
Changes == [subj : Subjects, op : {"add", "del"}, actor : Actors, auth : Auths]

VARIABLES
  doc,          \* "absent", "open" (created in the open transaction), "exists"
  owedReg,      \* TRUE while the stored record says the registration is owed
  st,           \* per slot: where its change stands (see TypeOK)
  chg,          \* per slot: the change staged there
  receipt,      \* per slot: what the provider recorded: "none", "ok", "rejected",
                \* or "lapsed" (rejected: the credential it was sent with no longer held)
  effects,      \* per slot: how often the change took effect on the provider
  checked,      \* per slot, in memory only: "I asked, the provider has no receipt"
  chk,          \* per slot: TRUE when the actor's authority was checked at staging
  rolled,       \* the slots whose transaction rolled back (for the properties)
  chainOwner,   \* the provider's owner of the document, "none" if unregistered
  chainTuples,  \* the tuples the provider holds
  up,           \* TRUE while the provider can be reached
  delegOk,      \* TRUE while the stored delegation is valid
  crashes,      \* how often the node crashed
  expiries,     \* how often the delegation expired
  regAuth,      \* what the owed registration recorded to be sent with: "-" before there is one
  regOk,        \* TRUE while the delegation recorded with the registration is valid
  regBy,        \* who signed that delegation: the creator "c", or another, "o"
  regPark,      \* TRUE while the registration is parked: it waits for authority
  regNote,      \* TRUE while its record says why its last submission failed
  regFail,      \* TRUE once a submission of it failed and it is still owed
  gone,         \* per slot: who abandoned the change, "-" if nobody did
  late,         \* the slots whose change landed after it was abandoned
  regVia,       \* whose word the registration was sent on: see RegisterOnChain
  tomb,         \* the slots whose record was deleted without the provider holding
                \* the change, and whose actor has not been told yet
  acked,        \* the slots whose actor was told so
  flight,       \* per slot, stored: the in-flight mark, "-" none, "digest" the
                \* request's digest, "hash" its digest and its transaction hash
  journal,      \* the slots of the request the worker's journal holds, {} if none
  landed,       \* TRUE once the request in the journal has executed on the provider
  noted,        \* the provider's note: one entry per request resolved by another, with
                \* the request's digest, its outcome, and the slots whose stored
                \* sequence floor its sequence reaches ("fresh")
  solo,         \* the slots to be sent in a request of their own, stored
  tx,           \* per slot: the transaction that staged it (its first slot)
  cur,          \* in memory: the slots of the request the drainer is working on
  dg,           \* per slot, stored with the mark: the digest of its request (NoDigest if none)
  cg,           \* per slot: the generation of the delegation its journaled request carries
  gen,          \* the generation of the stored delegation: one more each time a fresh
                \* one replaces an expired one
  jfresh,       \* the slots marked when the journaled request was signed: its
                \* sequence reaches their floors
  stale,        \* the slots that took effect under a delegation that had expired
  cred,         \* per slot: the DID whose verified credential a client staged it with,
                \* "-" for a change an embedder staged
  bare          \* the slots a client staged signed in with an identity token, which
                \* is no delegation the node could send a change with

\* How the registration was sent, and what is kept of discarded changes.
rest == <<regVia, tomb, acked>>
\* The record of the owed registration, and what became of abandoned changes.
more == <<regAuth, regOk, regBy, regPark, regNote, regFail, gone, late, rest>>
\* What a journaled request names and carries: its digest, its credential,
\* and the sequence it was signed with.
keyed == <<dg, cg, gen, jfresh, stale, cred, bare>>
\* What the journaled drain keeps: the marks, the journal, the note, batches.
drain == <<flight, journal, landed, noted, solo, tx, cur, keyed>>

vars == <<doc, owedReg, st, chg, receipt, effects, checked, chk, rolled,
          chainOwner, chainTuples, up, delegOk, crashes, expiries, more,
          drain>>  \* every variable

TypeOK ==
  /\ doc \in {"absent", "open", "exists"}                 \* a known document state
  /\ owedReg \in BOOLEAN                                  \* a flag
  /\ st \in [Slots -> {"free", "open", "void", "staged", "inflight",
                       "awaiting", "refused", "settled",
                       "dropped", "abandoned"}]           \* a known step each
  /\ chg \in [Slots -> Changes \cup {NoChange}]           \* a change or nothing
  /\ receipt \in [Slots -> {"none", "ok", "rejected", "lapsed"}]  \* a known receipt each
  /\ effects \in [Slots -> 0..2]                          \* a small count each
  /\ checked \in [Slots -> BOOLEAN]                       \* a flag each
  /\ chk \in [Slots -> BOOLEAN]                           \* a flag each
  /\ rolled \subseteq Slots                               \* some slots
  /\ chainOwner \in {"none", "c", "o"}                    \* a known owner
  /\ chainTuples \subseteq Subjects                       \* some tuples
  /\ up \in BOOLEAN                                       \* a flag
  /\ delegOk \in BOOLEAN                                  \* a flag
  /\ crashes \in 0..MaxCrashes                            \* a small count
  /\ expiries \in 0..MaxExpiries                          \* a small count
  /\ regAuth \in {"-", "node", "deleg", "none", "ambient"} \* a known authority
  /\ regOk \in BOOLEAN /\ regPark \in BOOLEAN              \* flags
  /\ regNote \in BOOLEAN /\ regFail \in BOOLEAN            \* flags
  /\ regBy \in Presenters                                 \* a known signer
  /\ gone \in [Slots -> Abandoners \cup {"-"}]             \* somebody or nobody each
  /\ late \subseteq Slots                                 \* some slots
  /\ regVia \in {"-", "record", "change", "request", "ambient"}  \* a known way
  /\ tomb \subseteq Slots /\ acked \subseteq Slots          \* some slots
  /\ flight \in [Slots -> {"-", "digest", "hash"}]          \* a known mark each
  /\ journal \subseteq Slots /\ landed \in BOOLEAN          \* a request or none, a flag
  /\ \A e \in noted : e.o \in {"ok", "rejected"} /\ e.fresh \subseteq Slots  \* known entries
  /\ DOMAIN dg = Slots /\ cg \in [Slots -> 0..MaxExpiries]   \* a digest and a generation each
  /\ gen \in 0..MaxExpiries                                  \* a small count
  /\ jfresh \subseteq Slots /\ stale \subseteq Slots         \* some slots
  /\ cred \in [Slots -> Actors \cup {"-"}]                  \* a client or none each
  /\ bare \subseteq Slots                                  \* some slots
  /\ solo \subseteq Slots /\ cur \subseteq Slots            \* some slots
  /\ tx \in [Slots -> 0..MaxChanges]                        \* a transaction each

Init ==
  /\ doc = (IF Preexisting THEN "exists" ELSE "absent")   \* there from the start, or not yet
  /\ owedReg = FALSE                                      \* nothing is owed
  /\ st = [s \in Slots |-> "free"]                        \* no sequence number is taken
  /\ chg = [s \in Slots |-> NoChange]                     \* nothing is staged
  /\ receipt = [s \in Slots |-> "none"]                   \* the provider recorded nothing
  /\ effects = [s \in Slots |-> 0]                        \* nothing took effect
  /\ checked = [s \in Slots |-> FALSE]                    \* the loop remembers nothing
  /\ chk = [s \in Slots |-> FALSE]                        \* nothing was checked
  /\ rolled = {}                                          \* nothing rolled back
  /\ chainOwner = (IF Preexisting THEN "c"                \* registered to the creator,
                   ELSE IF AssumeUncontested THEN "none"  \* or not registered,
                   ELSE "o")                              \* or held by another owner
  /\ chainTuples = InitTuples                             \* the tuples given
  /\ up = TRUE                                            \* the provider answers
  /\ delegOk = TRUE                                       \* the delegation is valid
  /\ crashes = 0                                          \* no crash yet
  /\ expiries = 0                                         \* no expiry yet
  /\ regAuth = "-"                                        \* no registration is recorded
  /\ regOk = FALSE /\ regBy = "c"                         \* so nothing is valid
  /\ regPark = FALSE /\ regNote = FALSE /\ regFail = FALSE  \* nothing failed
  /\ gone = [s \in Slots |-> "-"]                         \* nothing was abandoned
  /\ late = {}                                            \* nothing landed late
  /\ regVia = "-"                                         \* the registration was not sent
  /\ tomb = {} /\ acked = {}                              \* nothing was discarded
  /\ flight = [s \in Slots |-> "-"]                       \* nothing is in flight
  /\ journal = {} /\ landed = FALSE                       \* the journal is empty
  /\ noted = {}                                           \* the note is empty
  /\ dg = [s \in Slots |-> NoDigest] /\ cg = [s \in Slots |-> 0]  \* nothing is marked
  /\ gen = 0 /\ jfresh = {} /\ stale = {}                  \* the first delegation, nothing signed
  /\ cred = [s \in Slots |-> "-"]                         \* no client staged anything
  /\ bare = {}                                            \* so none signed in with a token
  /\ solo = {} /\ cur = {}                                \* no batch refused, no request held
  /\ tx = [s \in Slots |-> 0]                             \* no slot is staged

\* ---- Words used below ----

\* The highest sequence number in a set of slots that is not empty.
Last(S) == CHOOSE s \in S : \A r \in S : r <= s
\* The change is owed and still to be delivered.
Live(s) == st[s] \in {"staged", "inflight", "awaiting"}
\* The node still stores a record for the change: owed, or parked as refused.
Kept(s) == Live(s) \/ st[s] = "refused"
\* The provider holds the change and the node keeps no record of it: the
\* record was deleted after the provider answered, or it was removed as an
\* unchecked change after it had in fact landed.
Delivered(s) == st[s] = "settled" \/ (st[s] \in {"dropped", "abandoned"} /\ effects[s] > 0)
\* The change was abandoned by the actor that staged it: that actor took its
\* own word back, so the change counts for nothing any more. Abandoned by
\* anyone else (the bug) it still counts: nobody may take back another's word.
Withdrawn(s) == st[s] = "abandoned" /\ gone[s] = chg[s].actor
\* The change belongs to a transaction that committed, and still counts: the
\* node keeps it or the provider holds it. An unchecked change removed before
\* it ever landed counts for nothing.
Committed(s) ==
  /\ Kept(s) \/ Delivered(s) \/ (st[s] = "abandoned" /\ ~Withdrawn(s))
  /\ s \notin rolled
\* A transaction is open: it created the document or staged a change.
OpenTxn == doc = "open" \/ \E s \in Slots : st[s] = "open"
\* The change can be signed now: the node's own key always can, a delegation
\* only while it is valid.
Valid(s) == chg[s].auth = "node" \/ delegOk
\* A journaled change's request can be executed: signed with the node's own
\* key, or with the delegation it carries while that one is still valid. A
\* fresh delegation handed in since is not in its bytes. The bug (Captured =
\* FALSE) takes the stored delegation, which a fresh one renews.
JValid(s) ==
  IF Captured THEN chg[s].auth = "node" \/ (cg[s] = gen /\ delegOk)
  ELSE Valid(s)
\* A set of tuples lets this reader in: by name, or through "everyone".
Carries(T, who) == who \in T \/ "*" \in T

\* ---- The two states the node's answers are measured against ----

\* The tuples that hold once the counted changes have all applied in
\* sequence order: for each tuple, its last counted change decides; a tuple
\* no counted change touched stays as the provider had it at the start.
Final(Counted) ==
  {t \in Subjects :
     LET S == {s \in Counted : chg[s].subj = t}           \* the changes on this tuple
     IN IF S = {} THEN t \in InitTuples                   \* untouched: as at the start
        ELSE chg[Last(S)].op = "add"}                     \* else the last one decides

\* The intended state: every committed change applied.
IntendedTuples == Final({s \in Slots : Committed(s)})
\* The owner the provider ends with: the creator, unless another holds the
\* document, from the start or since it took it over.
SettledOwner == IF AssumeUncontested /\ chainOwner # "o" THEN "c" ELSE "o"
\* The provider accepts a change only from the document's owner at the time.
\* So the changes it holds or will hold are those that took effect, and
\* those still to be delivered whose actor owns the document now.
Accepted(s) ==
  /\ Committed(s)                                         \* it counts at all,
  /\ \/ effects[s] > 0                                    \* and it landed,
     \/ Live(s) /\ chg[s].actor = SettledOwner            \* or it will, as things stand
\* The settled state: every committed change the provider accepts applied.
SettledTuples == Final({s \in Slots : Accepted(s)})

\* The intended state lets this reader in: the creator, or a tuple.
IntendedAllows(who) == doc = "exists" /\ (who = "c" \/ Carries(IntendedTuples, who))
\* The provider, once everything is settled, lets this reader in.
SettledAllows(who) == doc = "exists" /\ (who = SettledOwner \/ Carries(SettledTuples, who))

\* ---- The overlay: what the node answers before settlement ----

\* An owed delete that denies: still to be delivered, or refused when a
\* refused delete keeps denying.
Denying(s) == chg[s].op = "del" /\ (Live(s) \/ (st[s] = "refused" /\ RefusedDeleteDenies))
\* The stored records for one tuple.
OnTuple(t) == {s \in Slots : Kept(s) /\ chg[s].subj = t}
\* The tuple is denied by the overlay.
\*   AnyOwedDelete  while any stored delete on it denies. This is the rule
\*                  the model shows to be needed.
\*   LatestOp       while the latest stored record on it is a delete. This
\*                  is the wording of #2954, and it is too weak: a later
\*                  owed add lifts the denial though an owed add must grant
\*                  nothing (MC_AcpOutbox_Red_LatestOpOverlay).
\*   None           never: a delete counts only once the provider applied it.
DenyTuple(t) ==
  CASE DenyRule = "AnyOwedDelete" -> \E s \in OnTuple(t) : Denying(s)
    [] DenyRule = "LatestOp" -> OnTuple(t) # {} /\ Denying(Last(OnTuple(t)))
    [] OTHER -> FALSE
\* The reader is denied by the overlay: its own tuple is, or "everyone" is.
\* The node cannot see through which tuple the provider lets a reader in, so
\* an owed delete of "everyone" denies every reader but the document's owner.
Denied(who) == DenyTuple(who) \/ (DenyTuple("*") /\ who # chainOwner)

\* An owed add that the node honours before the provider has it.
\*   CreatorExplicit  only the creator's own add, and only for a named
\*                    identity, never for "everyone" (the design);
\*   CreatorAny       the creator's own add, also for "everyone";
\*   Any              every owed add, whoever staged it and whether or not
\*                    the registration is still owed.
Grants(s) ==
  /\ Live(s) /\ chg[s].op = "add"                         \* an add still to be delivered
  /\ CASE OwedAdds = "CreatorExplicit" -> chg[s].actor = "c" /\ chg[s].subj # "*"
       [] OwedAdds = "CreatorAny" -> chg[s].actor = "c"
       [] OTHER -> TRUE
\* The subjects such adds name.
OwedGranted == {chg[s].subj : s \in {r \in Slots : Grants(r)}}

\* The node's answer to one reader, from the provider's tuples T and the
\* owed grants G. They are passed in so a property can ask "and without this
\* tuple?".
HonoursUsing(who, T, G) ==
  /\ doc = "exists"                                       \* there is a document to read
  /\ IF owedReg
     THEN \/ who = "c"                                    \* owed: the creator,
          \/ Carries(G, who) /\ ~Denied(who)              \* or someone the creator granted
     ELSE /\ ~Denied(who)                                 \* settled: no owed delete denies,
          /\ \/ up /\ (who = chainOwner \/ Carries(T, who))  \* and the provider says yes,
             \/ OwedAdds = "Any" /\ Carries(G, who)       \* or (bug) any owed add does
\* The node's answer as it stands.
Honours(who) == HonoursUsing(who, chainTuples, OwedGranted)

\* ---- Transactions ----

\* The open transaction creates the document. The owed registration is one
\* of its writes (register_created_doc), so nothing is stored yet.
StageCreate ==
  /\ doc = "absent"                                       \* there is no document yet
  /\ doc' = "open"                                        \* now there is, uncommitted
  /\ UNCHANGED <<owedReg, st, chg, receipt, effects, checked, rolled,
                 chainOwner, chainTuples, up, delegOk, crashes, expiries>>
  /\ UNCHANGED chk                                       \* what was checked stays so
  /\ UNCHANGED more                                      \* and so do the registration's record and what was abandoned
  /\ UNCHANGED drain                                     \* the journaled drain is not involved

\* The staging check can run: the document's registration is still owed
\* (its creator is then the only authority, and the node knows it without
\* asking), or the provider answers.
CanCheck == doc = "open" \/ owedReg \/ up
\* The check lets this change through: its actor is the owed creator, or the
\* owner the provider names.
Passes(c) ==
  IF doc = "open" \/ owedReg THEN c.actor = "c"            \* owed: only the creator
  ELSE c.actor = chainOwner                               \* else: only the provider's owner

\* The open transaction stages one change on the document. It takes the next
\* sequence number. When the check can run, a change that fails it is not
\* staged at all. When it cannot (the provider is down), an add is staged
\* unchecked, and a delete is not staged: it would deny at once on the word
\* of an actor nobody vouched for.
\* Staging change c at s, after the check when `check` holds; without it
\* (the bug of a route that skips the check) anything is staged unchecked.
Staged(s, c, check) ==
  /\ doc \in {"open", "exists"}                           \* the transaction sees the document
  /\ st[s] = "free"                                       \* this number is not taken,
  /\ \A r \in Slots : r < s => st[r] # "free"             \* and every lower one is
  /\ IF check /\ CanCheck                                \* the check can run:
     THEN /\ Passes(c)                                    \*   the change must pass it,
          /\ chk' = [chk EXCEPT ![s] = TRUE]              \*   and is recorded as checked
     ELSE /\ c.op = "add" \/ ~CheckedDeletes \/ ~check   \* else only an add (or, bug, any)
          /\ chk' = [chk EXCEPT ![s] = FALSE]             \*   recorded as unchecked
  /\ st' = [st EXCEPT ![s] = "open"]                      \* staged, uncommitted
  /\ chg' = [chg EXCEPT ![s] = c]                         \* with this change
  /\ UNCHANGED <<doc, owedReg, receipt, effects, checked, rolled,
                 chainOwner, chainTuples, up, delegOk, crashes, expiries>>
  /\ UNCHANGED more                                      \* and so do the registration's record and what was abandoned

\* An embedder's change, always checked.
StageChange(s, c) ==
  /\ Staged(s, c, TRUE)                                   \* staged after the check
  /\ UNCHANGED drain                                     \* the journaled drain is not involved

\* What the node can send a client's change c with, for a client signed in
\* as `who` with `shown`: its own key when it holds `who`'s, whatever the
\* client signed in with (the code asks for a held key before it looks at
\* the bearer); otherwise only the delegation the client presented. An
\* identity token for a DID whose key the node does not hold gives it
\* nothing to send the change with once the request is gone.
Sendable(c, who, shown) ==
  IF who \in HeldKeys THEN c.auth = "node"                 \* the node signs as who itself
  ELSE shown = "deleg" /\ c.auth = "deleg"                 \* else under the presented delegation

\* A client stages change c over HTTP in its transaction, authenticated by
\* the verified credential of `who`, a delegation or an identity token
\* (`shown`). The change acts as `who` (ActorFromCredential; the bug takes
\* the acting DID from the request, so any actor's change goes in under
\* another's credential), records an authority the node can send it with
\* (RemoteAuthority; the bug stages a change with none, such as one signed
\* in with an identity token for a DID whose key the node does not hold),
\* and is checked as an embedder's change is (RemoteCheck). A request with
\* no credential stages nothing, so there is no such step for it. From the
\* commit on the change is like any other: settled by the commit's pass and
\* the drainer, parked when its delegation lapses, gone with a rollback.
StageRemote(s, c, who, shown) ==
  /\ Remote                                               \* clients stage over HTTP
  /\ Sendable(c, who, shown) \/ ~RemoteAuthority          \* under an authority the node can send with (or, bug, any)
  /\ ActorFromCredential => c.actor = who                 \* as the DID it names
  /\ Staged(s, c, RemoteCheck)                            \* checked as any change is
  /\ cred' = [cred EXCEPT ![s] = who]                     \* remembered, for the properties
  /\ bare' = IF shown = "token" THEN bare \cup {s}         \* and how the client signed in
           ELSE bare \ {s}
  /\ UNCHANGED <<flight, journal, landed, noted, solo, tx, cur, dg, cg, gen, jfresh, stale>>

\* The first slot the open transaction staged: the name of that transaction.
FirstOpen == CHOOSE s \in Slots : st[s] = "open" /\ \A r \in Slots : st[r] = "open" => s <= r

\* The open transaction commits: the document, its owed registration and the
\* staged changes become durable together. The registration records what it
\* is to be sent with: the node's key for the creator, the delegation the
\* create's request itself carried, or nothing. With nothing it is parked at
\* once and says so. A request that carries no bearer records nothing and
\* its commit sends nothing: the bug ("ambient" in RegAuths) records the
\* token the process holds for the creator, which every later retry would
\* then send.
Commit ==
  /\ OpenTxn                                              \* something is open
  /\ doc' = (IF doc = "open" THEN "exists" ELSE doc)      \* the document is there
  /\ owedReg' = (owedReg \/ doc = "open")                 \* its registration is owed
  /\ st' = [s \in Slots |-> IF st[s] = "open" THEN "staged" ELSE st[s]]  \* changes are owed
  /\ IF doc = "open"                                      \* a create records its authority:
     THEN \E a \in RegAuths :
            /\ regAuth' = a                                \*   what the create came with,
            /\ regOk' = (a = "deleg")                      \*   valid if it is a delegation,
            /\ regBy' = "c"                                \*   signed by the creator,
            /\ regPark' = (a = "none")                     \*   parked when there is nothing,
            /\ regNote' = (a = "none")                     \*   and saying why
            /\ regFail' = FALSE                            \*   nothing was tried yet
     ELSE UNCHANGED <<regAuth, regOk, regBy, regPark, regNote, regFail>>
  /\ UNCHANGED rest                                      \* how it was sent and what was discarded stay
  /\ UNCHANGED <<gone, late>>                             \* nothing is abandoned here
  /\ UNCHANGED <<chg, receipt, effects, checked, rolled,
                 chainOwner, chainTuples, up, delegOk, crashes, expiries>>
  /\ UNCHANGED chk                                       \* what was checked stays so
  /\ tx' = IF Journaled                                  \* the journaled drain batches by
           THEN [s \in Slots |-> IF st[s] = "open"        \*   transaction: each slot it commits
                                 THEN FirstOpen           \*   is named by the first of them
                                 ELSE tx[s]]
           ELSE tx                                        \* else nobody asks
  /\ UNCHANGED <<flight, journal, landed, noted, solo, cur, keyed>>  \* nothing is sent at a commit

\* What a discarded transaction leaves of its changes. Owned by the
\* transaction, they are gone with it. Written beside it (the bug), they
\* stay owed.
DiscardSt ==
  [s \in Slots |-> IF st[s] = "open"
                   THEN (IF TxnOwnsOwed THEN "void" ELSE "staged")
                   ELSE st[s]]
\* The changes it leaves: a discarded one is forgotten.
DiscardChg ==
  [s \in Slots |-> IF st[s] = "open" /\ TxnOwnsOwed THEN NoChange ELSE chg[s]]
\* Whether the registration is owed after a discard: as before, or (the bug)
\* owed for a document that was never committed.
DiscardReg == owedReg \/ (~TxnOwnsOwed /\ doc = "open")
\* The slots the discarded transaction had staged.
Discarded == {s \in Slots : st[s] = "open"}

\* The open transaction rolls back.
Rollback ==
  /\ OpenTxn                                              \* something is open
  /\ doc' = (IF doc = "open" THEN "absent" ELSE doc)      \* its document is gone
  /\ owedReg' = DiscardReg                                \* and so is what it owed,
  /\ st' = DiscardSt                                      \* its changes,
  /\ chg' = DiscardChg                                    \* and what they said
  /\ rolled' = rolled \cup Discarded                      \* remembered, for the properties
  /\ UNCHANGED <<receipt, effects, checked, chainOwner, chainTuples,
                 up, delegOk, crashes, expiries>>
  /\ UNCHANGED chk                                       \* what was checked stays so
  /\ UNCHANGED more                                      \* and so do the registration's record and what was abandoned
  /\ UNCHANGED drain                                     \* the journaled drain is not involved

\* The node crashes and restarts. An open transaction is rolled back. The
\* settlement loop forgets what it had checked. Without a durable mark, a
\* change in flight reads as never sent.
Crash ==
  /\ crashes < MaxCrashes                                 \* crashes are not endless
  /\ \/ OpenTxn                                           \* there is something to lose:
     \/ \E s \in Slots : checked[s]                       \* an open transaction, a memory,
     \/ ~Marker /\ \E s \in Slots : st[s] = "inflight"    \* or a mark that does not last,
     \/ Journaled /\ cur # {}                             \* or a request the drainer holds
  /\ crashes' = crashes + 1                               \* one more
  /\ doc' = (IF doc = "open" THEN "absent" ELSE doc)      \* the open document is gone
  /\ owedReg' = DiscardReg                                \* with what it owed
  /\ st' = [s \in Slots |-> IF DiscardSt[s] = "inflight" /\ ~Marker
                            THEN "staged"                 \* the mark is lost
                            ELSE DiscardSt[s]]            \* or everything stored stays
  /\ chg' = DiscardChg                                    \* open changes are forgotten
  /\ rolled' = rolled \cup Discarded                      \* remembered, for the properties
  /\ checked' = [s \in Slots |-> FALSE]                   \* the loop remembers nothing
  /\ cur' = {}                                            \* nor which request it held
  /\ flight' = IF Marker THEN flight                      \* the marks are stored,
              ELSE [s \in Slots |-> "-"]                  \* or (bug) they are lost
  /\ UNCHANGED <<journal, landed, noted, solo, tx, keyed>>  \* the provider's journal and note stay
  /\ UNCHANGED <<receipt, effects, chainOwner, chainTuples, up, delegOk, expiries>>
  /\ UNCHANGED chk                                       \* what was checked stays so
  /\ UNCHANGED more                                      \* and so do the registration's record and what was abandoned

\* ---- Settling the registration (ExternalDocumentAcp::settle) ----

\* The registration can be sent with what its record holds: the node's key
\* for the creator, or a delegation that is still valid.
RegValid == regAuth \in {"node", "ambient"} \/ (regAuth = "deleg" /\ regOk)
\* Whom the provider registers: the signer of the credential it is sent.
RegOwner == IF regAuth = "deleg" THEN regBy ELSE "c"
\* The creator staged a change of its own that is still owed and carries a
\* delegation that is still valid: that delegation is the creator's word too.
ByChange ==
  \E s \in Slots : st[s] = "staged" /\ chg[s].actor = "c"
                    /\ chg[s].auth = "deleg" /\ delegOk

\* The ways a registration may be sent, and whose word each rests on.
\*   record   what its record holds;
\*   change   the delegation recorded with a change the creator still owes,
\*            when the record's own no longer serves;
\*   request  the bearer a request carries for the creator, checked as at
\*            admission: the submission a commit makes goes this way when
\*            its request carried one. The creator may make such a request
\*            at any time, so this way is always open;
\*   ambient  a token the process happens to hold for the creator, which no
\*            request carried: sent by the commit of a request that carries
\*            no bearer, by a retry that serves no request, on another
\*            caller's request, or from the record a create wrote it into.
\*            The bug. No way of sending reads anything a process keeps
\*            beside its requests.
\* In every way but the last the creator presented the credential.
Ways == {"record", "change", "request", "ambient"}
MaySendBy(via) ==
  CASE via = "record" -> RegValid
    [] via = "change" -> ~RegValid /\ ByChange
    [] via = "request" -> TRUE
    [] OTHER -> ~RegValid /\ RegRetry = "ambient"

\* The provider registers the owner. A provider that cannot be reached does
\* not take this step, and the record stays.
RegisterOnChain(via) ==
  /\ owedReg /\ up                                        \* owed, and the provider answers
  /\ chainOwner = "none"                                  \* it holds no owner yet
  /\ MaySendBy(via)                                       \* a way that is open
  /\ chainOwner' = (IF via = "record" THEN RegOwner ELSE "c")  \* the credential's signer owns it
  /\ regVia' = (IF via = "record" /\ regAuth = "ambient"
                THEN "ambient" ELSE via)                  \* remembered, for the properties
  /\ UNCHANGED <<regAuth, regOk, regBy, regPark, regNote, regFail, gone, late,
                 tomb, acked>>
  /\ UNCHANGED <<doc, owedReg, st, chg, receipt, effects, checked, rolled,
                 chainTuples, up, delegOk, crashes, expiries>>
  /\ UNCHANGED chk                                       \* what was checked stays so
  /\ UNCHANGED drain                                     \* the journaled drain is not involved

\* The provider holds the document, so the record is cleared. An owner the
\* provider already had stays: from here on the provider decides.
ClearOwed ==
  /\ owedReg /\ up                                        \* owed, and the provider answers
  /\ chainOwner # "none"                                  \* it holds the document
  /\ owedReg' = FALSE                                     \* nothing is owed any more
  /\ regPark' = FALSE /\ regNote' = FALSE /\ regFail' = FALSE  \* and nothing is reported
  /\ UNCHANGED <<regAuth, regOk, regBy, gone, late>>      \* the rest stays
  /\ UNCHANGED rest                                      \* how it was sent and what was discarded stay
  /\ UNCHANGED <<doc, st, chg, receipt, effects, checked, rolled,
                 chainOwner, chainTuples, up, delegOk, crashes, expiries>>
  /\ UNCHANGED chk                                       \* what was checked stays so
  /\ UNCHANGED drain                                     \* the journaled drain is not involved

\* A submission of the registration fails after the commit: the provider
\* cannot be reached, or what the record holds no longer lets the node send
\* it. The commit stands. The record keeps the reason (the bug keeps none),
\* and a registration with no usable authority is parked: it is not sent
\* again until someone hands the node a fresh credential.
RegistrationFails ==
  /\ owedReg /\ chainOwner = "none"                       \* owed, and not registered
  /\ ~up \/ ~RegValid                                     \* the submission cannot succeed
  /\ ~regFail \/ (~RegValid /\ ~regPark)                  \* and that is news
  /\ regFail' = TRUE                                      \* a submission failed
  /\ regNote' = ReportFailures                            \* the record says why
  /\ regPark' = (regPark \/ ~RegValid)                    \* parked when authority is the reason
  /\ UNCHANGED <<regAuth, regOk, regBy, gone, late>>      \* the rest stays
  /\ UNCHANGED rest                                      \* how it was sent and what was discarded stay
  /\ UNCHANGED <<doc, owedReg, st, chg, receipt, effects, checked, chk, rolled,
                 chainOwner, chainTuples, up, delegOk, crashes, expiries>>
  /\ UNCHANGED drain                                     \* the journaled drain is not involved

\* Someone hands the node a fresh credential for the parked registration.
\* The node takes it only when it is the creator's own, checked as the
\* credential of the create was (VerifiedRemoteWrite.tla); the bug takes
\* anyone's. The registration is owed again, with that credential.
ReauthorizeReg(who) ==
  /\ owedReg /\ regPark                                   \* it waits for authority
  /\ who = "c" \/ ~ReauthChecked                          \* the creator's own (or, bug, any)
  /\ regAuth' = "deleg" /\ regOk' = TRUE /\ regBy' = who  \* recorded, valid, signed by who
  /\ regPark' = FALSE /\ regNote' = FALSE /\ regFail' = FALSE  \* owed again, nothing to report
  /\ UNCHANGED <<gone, late>>                             \* the rest stays
  /\ UNCHANGED rest                                      \* how it was sent and what was discarded stay
  /\ UNCHANGED <<doc, owedReg, st, chg, receipt, effects, checked, chk, rolled,
                 chainOwner, chainTuples, up, delegOk, crashes, expiries>>
  /\ UNCHANGED drain                                     \* the journaled drain is not involved

\* The delegation recorded with the registration expires.
ExpireReg ==
  /\ owedReg /\ regAuth = "deleg" /\ regOk               \* a valid recorded delegation
  /\ expiries < MaxExpiries                               \* expiries are not endless
  /\ regOk' = FALSE                                       \* no longer valid
  /\ expiries' = expiries + 1                             \* one more
  /\ UNCHANGED <<regAuth, regBy, regPark, regNote, regFail, gone, late>>
  /\ UNCHANGED rest                                      \* how it was sent and what was discarded stay
  /\ UNCHANGED <<doc, owedReg, st, chg, receipt, effects, checked, chk, rolled,
                 chainOwner, chainTuples, up, delegOk, crashes>>
  /\ UNCHANGED drain                                     \* the journaled drain is not involved

\* ---- Settling the changes (the drainer of #2954) ----

\* An earlier stored change holds this one back.
\* An earlier parked change holds this one: it waits for authority or was
\* refused, on the same tuple, and its actor was checked at staging. A
\* parked change nobody vouched for holds nothing: UncheckedYields says what
\* happens instead.
\*   drop  it does not hold, and it is removed when a later change of its
\*         tuple is sent (the design);
\*   pass  it does not hold, and it stays: the bug of letting it be
\*         reauthorized after the change that passed it;
\*   hold  it holds like a checked one: the bug of letting an actor nobody
\*         checked keep the owner's change from the provider.
Holds(r, s) ==
  /\ r < s /\ ParkedBlocks                                \* earlier, and parking holds
  /\ st[r] \in {"awaiting", "refused"}                    \* it is parked,
  /\ chg[r].subj = chg[s].subj                            \* on the same tuple,
  /\ chk[r] \/ UncheckedYields = "hold"                   \* and was checked (or, bug, not)

\* Sending this change passes that one: an earlier unchecked change of the
\* same tuple that the provider has not taken. It waits for authority, or
\* it is still owed and not in flight: the node could not send it, or the
\* provider answered it with an error. It is removed then, so that it can
\* never be sent after the change that passed it. The node passes a change
\* still owed only after trying it; here it may at any time, which covers
\* that.
Passed(r, s) ==
  /\ UncheckedYields = "drop"                             \* the design removes it
  /\ r < s /\ chg[r].subj = chg[s].subj                   \* earlier, same tuple,
  /\ st[r] \in {"awaiting", "staged"} /\ ~chk[r]          \* not taken, and unchecked

\* An earlier stored change outside the request R holds this one back. The
\* changes of one request go together, in order, so they hold nothing back
\* among themselves.
BlockedBy(s, R) ==
  \E r \in Slots \ R :
    /\ r < s                                              \* an earlier change
    /\ \/ /\ InOrder /\ st[r] \in {"staged", "inflight"}  \* that is still to be delivered
          /\ ~Passed(r, s)                                \* and does not yield,
       \/ Holds(r, s)                                     \* or is parked and holds it,
       \/ ~DrainPastRefused /\ st[r] = "refused"          \* or (bug) was refused

\* An earlier stored change holds this one back.
Blocked(s) == BlockedBy(s, {})

\* Where a change the provider refused ends. One whose actor was checked at
\* staging is parked and kept. One staged unchecked is removed: nobody
\* vouched for it and the provider said no, so it may not hold its tuple or
\* stay on the books. A change that only waits for authority is kept either
\* way: the provider has not refused it, and it may be a good change whose
\* delegation ran out while the provider was away.
Refusal(s) ==
  IF chk[s] \/ ~DropUnchecked THEN "refused" ELSE "dropped"

\* What is kept of changes whose record is deleted without the provider
\* holding them: a note for their actor, with the reason. The bug keeps none.
Noted(S) == IF ReportFailures THEN tomb \cup S ELSE tomb

\* The loop marks a change in flight, durably, before anything is sent. A
\* fresh mark means nothing was sent, so there is no receipt to ask for.
Mark(s) ==
  /\ ~Journaled                                           \* the abstract drain
  /\ st[s] = "staged"                                     \* owed and not yet sent
  /\ ~Blocked(s)                                          \* its turn has come
  /\ RegFirst => ~owedReg                                 \* the registration is settled
  /\ Valid(s)                                             \* it can be signed
  /\ st' = [r \in Slots |-> IF r = s THEN "inflight"      \* marked,
                            ELSE IF Passed(r, s) THEN "dropped"  \* what it passes is removed,
                            ELSE st[r]]                   \* the rest stays
  /\ checked' = [checked EXCEPT ![s] = TRUE]              \* known not to have landed
  /\ tomb' = Noted({r \in Slots : Passed(r, s)})           \* what was removed is remembered
  /\ UNCHANGED <<regAuth, regOk, regBy, regPark, regNote, regFail, gone, late,
                 regVia, acked>>
  /\ UNCHANGED <<doc, owedReg, chg, receipt, effects, rolled,
                 chainOwner, chainTuples, up, delegOk, crashes, expiries>>
  /\ UNCHANGED chk                                       \* what was checked stays so
  /\ UNCHANGED drain                                     \* the journaled drain is not involved

\* The provider's tuples after it applied a change.
Applied(s) ==
  IF chg[s].op = "add" THEN chainTuples \cup {chg[s].subj}  \* an add puts the tuple in,
  ELSE chainTuples \ {chg[s].subj}                          \* a delete takes it out

\* The loop sends the change and the request lands. The provider applies a
\* change from the document's owner and refuses any other, and records a
\* receipt either way. The loop does not learn which: it must ask.
Land(s) ==
  /\ ~Journaled                                           \* the abstract drain
  /\ st[s] \in {"inflight", "abandoned"} /\ checked[s]    \* in flight, known not landed
  /\ Valid(s) /\ up                                       \* signable, provider reachable
  /\ IF chg[s].actor = chainOwner                         \* from the owner:
     THEN /\ chainTuples' = Applied(s)                    \*   applied,
          /\ effects' = [effects EXCEPT ![s] = IF @ < 2 THEN @ + 1 ELSE @]  \* counted,
          /\ receipt' = [receipt EXCEPT ![s] = "ok"]      \*   and receipted
     ELSE /\ receipt' = [receipt EXCEPT ![s] = "rejected"]  \* else refused, receipted
          /\ UNCHANGED <<chainTuples, effects>>           \*   and nothing changes
  /\ checked' = [checked EXCEPT ![s] = FALSE]             \* the loop must ask again
  /\ late' = (IF st[s] = "abandoned" THEN late \cup {s} ELSE late)  \* landed after its abandonment
  /\ UNCHANGED <<regAuth, regOk, regBy, regPark, regNote, regFail, gone>>
  /\ UNCHANGED rest                                      \* how it was sent and what was discarded stay
  /\ UNCHANGED <<doc, owedReg, st, chg, rolled, chainOwner, up, delegOk,
                 crashes, expiries>>
  /\ UNCHANGED chk                                       \* what was checked stays so
  /\ UNCHANGED drain                                     \* the journaled drain is not involved

\* The loop sends the change while the provider cannot be reached: nothing
\* lands, and the loop cannot know that either.
SendLost(s) ==
  /\ ~Journaled                                           \* the abstract drain
  /\ st[s] = "inflight" /\ checked[s]                     \* in flight, known not landed
  /\ Valid(s) /\ ~up                                      \* signable, provider unreachable
  /\ checked' = [checked EXCEPT ![s] = FALSE]             \* the loop must ask again
  /\ UNCHANGED <<doc, owedReg, st, chg, receipt, effects, rolled,
                 chainOwner, chainTuples, up, delegOk, crashes, expiries>>
  /\ UNCHANGED chk                                       \* what was checked stays so
  /\ UNCHANGED more                                      \* and so do the registration's record and what was abandoned
  /\ UNCHANGED drain                                     \* the journaled drain is not involved

\* The loop asks the provider for the receipt of a change in flight. This is
\* also the first thing it does with one after a restart.
Lookup(s) ==
  /\ ~Journaled                                           \* the abstract drain
  /\ st[s] = "inflight" /\ ~checked[s]                    \* in flight, outcome unknown
  /\ up                                                   \* the provider answers
  /\ CASE receipt[s] = "ok" ->                            \* it landed:
            /\ st' = [st EXCEPT ![s] = "settled"]         \*   the record is deleted
            /\ UNCHANGED checked
       [] receipt[s] = "rejected" ->                      \* it was refused:
            /\ st' = [st EXCEPT ![s] = Refusal(s)]        \*   parked, or removed
            /\ UNCHANGED checked
       [] OTHER ->                                        \* no receipt:
            /\ checked' = [checked EXCEPT ![s] = TRUE]    \*   it may be sent again
            /\ UNCHANGED st
  /\ tomb' = (IF receipt[s] = "rejected" /\ Refusal(s) = "dropped"
              THEN Noted({s}) ELSE tomb)                   \* a removal is remembered
  /\ UNCHANGED <<regAuth, regOk, regBy, regPark, regNote, regFail, gone, late,
                 regVia, acked>>
  /\ UNCHANGED <<doc, owedReg, chg, receipt, effects, rolled,
                 chainOwner, chainTuples, up, delegOk, crashes, expiries>>
  /\ UNCHANGED chk                                       \* what was checked stays so
  /\ UNCHANGED drain                                     \* the journaled drain is not involved

\* ---- Settling through the worker's journal (the drainer of #3065) ----
\*
\* The code's drainer sends through a provider whose worker keeps one signed
\* request in a journal on disk until a verified receipt lets it go. A
\* request may carry several changes of one transaction; the provider
\* applies them together or not at all. For each request the drainer:
\*   1. builds it and stores the digest of its bytes with every change it
\*      carries (JMark);
\*   2. has the worker put it in its journal (JPrepare);
\*   3. stores the request's transaction hash with those changes (JHash);
\*   4. sends it and waits for its receipt (JLand, JResolve).
\* Whoever finds a request still in the journal sends it and waits for its
\* receipt: the provider at start, another command before its own
\* (JRecover). That one keeps a note of the outcome under the digest, with
\* the sequence the request was signed with. A change still marked when
\* nothing in memory holds its request is decided by the journal (JAdopt),
\* else by a receipt found by its hash or by the note found by its digest
\* among the requests signed from the floor stored with its mark on (Floor),
\* else it is sent again (JDecide). Two requests with the same changes and
\* the same delegation have the same digest. A journaled request carries
\* its delegation: it lands only while that one is valid (Captured), and a
\* fresh one handed in later is for the requests built after it.

\* The changes in R can go as one request: one change, or (MaxBatch = 2)
\* changes staged one after the other by one transaction, none of which had
\* its batch refused before. Each is staged, unmarked, signable, and held
\* back by nothing outside R.
Batch(R) ==
  /\ R # {} /\ Cardinality(R) <= MaxBatch                  \* one change, or a few
  /\ Cardinality(R) > 1 => R \cap solo = {}                \* none must go alone
  /\ \A s \in R :
       /\ st[s] = "staged" /\ flight[s] = "-"             \* owed and not marked
       /\ Valid(s)                                         \* signable
       /\ ~BlockedBy(s, R)                                 \* its turn has come
       /\ \A r \in R : tx[r] = tx[s]                       \* one transaction
       /\ \A q \in Slots : q < s /\ (\E r \in R : r < q) => q \in R  \* no gap

\* The provider's tuples after it applied every change of R in order: for
\* each tuple the last change of R on it decides.
AppliedAll(R) ==
  {t \in Subjects :
     LET S == {s \in R : chg[s].subj = t}                  \* the changes of R on it
     IN IF S = {} THEN t \in chainTuples                  \* untouched: as it was
        ELSE chg[Last(S)].op = "add"}                     \* else the last one decides

\* The digest of the request for R: the bytes of its changes in order, and
\* of the delegation it carries. Two requests with the same changes and the
\* same delegation have the same digest (a delegation used again for the
\* same command), and only their sequences tell them apart.
Digest(R) ==
  [c |-> IF Cardinality(R) = 1 THEN <<chg[Last(R)]>>
         ELSE <<chg[CHOOSE s \in R : \A r \in R : s <= r], chg[Last(R)]>>,
   g |-> IF \E r \in R : chg[r].auth = "deleg" THEN gen ELSE 0]

\* The drainer builds the request for R and stores its digest with every
\* change of R, durably, before anything is put in the journal, and with it
\* the worker's next sequence: every request signed from now on reaches
\* that floor, and every earlier one, the note's entries among them, does
\* not. The unchecked changes the request passes are removed as Mark
\* removes them. Any receipt left from an earlier request is not this one's.
JMark(R) ==
  /\ Journaled /\ cur = {}                                 \* the journaled drain, idle
  /\ Batch(R)                                              \* the request may go
  /\ RegFirst => ~owedReg                                  \* the registration is settled
  /\ st' = [r \in Slots |-> IF r \in R THEN "inflight"     \* marked,
                            ELSE IF \E s \in R : Passed(r, s) THEN "dropped"  \* passed: removed,
                            ELSE st[r]]                    \* the rest stays
  /\ flight' = [r \in Slots |-> IF r \in R THEN "digest" ELSE flight[r]]  \* with the digest
  /\ receipt' = [r \in Slots |-> IF r \in R THEN "none" ELSE receipt[r]]  \* a new request:
  /\ noted' = {[e EXCEPT !.fresh = e.fresh \ R] : e \in noted}  \* below the new floor
  /\ dg' = [r \in Slots |-> IF r \in R THEN Digest(R) ELSE dg[r]]  \* the request it names
  /\ cg' = [r \in Slots |-> IF r \in R THEN gen ELSE cg[r]]  \* and the delegation it carries
  /\ cur' = R                                              \* held in memory
  /\ tomb' = Noted({r \in Slots \ R : \E s \in R : Passed(r, s)})  \* removals are noted
  /\ UNCHANGED <<journal, landed, solo, tx, gen, jfresh, stale, cred, bare>>  \* nothing is journaled yet
  /\ UNCHANGED <<regAuth, regOk, regBy, regPark, regNote, regFail, gone, late,
                 regVia, acked>>
  /\ UNCHANGED <<doc, owedReg, chg, effects, checked, chk, rolled,
                 chainOwner, chainTuples, up, delegOk, crashes, expiries>>

\* The worker puts the drainer's request in its journal. It holds one
\* request at a time: one left there must be resolved first.
JPrepare ==
  /\ Journaled /\ cur # {} /\ journal = {}                 \* a request held, the journal free
  /\ \A s \in cur : st[s] = "inflight" /\ flight[s] = "digest"  \* marked with its digest
  /\ journal' = cur /\ landed' = FALSE                     \* journaled, not yet executed
  /\ jfresh' = {s \in Slots : flight[s] # "-"}            \* signed after every mark stored now
  /\ UNCHANGED <<flight, noted, solo, tx, cur, dg, cg, gen, stale, cred, bare>>  \* the marks stay
  /\ UNCHANGED <<doc, owedReg, st, chg, receipt, effects, checked, chk, rolled,
                 chainOwner, chainTuples, up, delegOk, crashes, expiries>>
  /\ UNCHANGED more                                        \* the registration's record stays

\* The drainer stores the journaled request's hash with its changes.
JHash ==
  /\ Journaled /\ cur # {} /\ journal = cur                \* its request is journaled
  /\ \E s \in cur : flight[s] = "digest"                   \* and the hash is not stored yet
  /\ flight' = [s \in Slots |-> IF s \in cur THEN "hash" ELSE flight[s]]  \* now it is
  /\ UNCHANGED <<journal, landed, noted, solo, tx, cur, keyed>>  \* the rest stays
  /\ UNCHANGED <<doc, owedReg, st, chg, receipt, effects, checked, chk, rolled,
                 chainOwner, chainTuples, up, delegOk, crashes, expiries>>
  /\ UNCHANGED more                                        \* the registration's record stays

\* The request in the journal executes on the provider, once: a request sent
\* again is the same signed bytes and is executed once (PendingSubmission.tla).
\* Whoever sends it, the drainer or another, it may do so before the hash is
\* stored. The provider applies every change of it when all are from the
\* document's owner and the credentials it carries are valid, and none
\* otherwise, and receipts the request.
JLand ==
  /\ Journaled /\ journal # {} /\ ~landed /\ up            \* journaled, not executed, reachable
  /\ landed' = TRUE                                        \* it executes once
  /\ IF \A s \in journal : chg[s].actor = chainOwner /\ JValid(s)
     THEN /\ chainTuples' = AppliedAll(journal)            \* all applied,
          /\ effects' = [s \in Slots |->                   \* each counted,
                IF s \in journal /\ effects[s] < 2 THEN effects[s] + 1 ELSE effects[s]]
          /\ receipt' = [s \in Slots |-> IF s \in journal THEN "ok" ELSE receipt[s]]
          /\ stale' = stale \cup {s \in journal :           \* (bug) under an expired delegation
                         chg[s].auth = "deleg" /\ ~(cg[s] = gen /\ delegOk)}
     ELSE /\ receipt' = [s \in Slots |->                  \* or none, and rejected,
                IF s \notin journal THEN receipt[s]
                ELSE IF JValid(s) THEN "rejected"          \*   for what it asks,
                ELSE "lapsed"]                             \*   or for its credential
          /\ UNCHANGED <<chainTuples, effects, stale>>     \* and nothing changes
  /\ late' = late \cup {s \in journal : st[s] = "abandoned"}  \* landed after its abandonment
  /\ UNCHANGED <<flight, journal, noted, solo, tx, cur, dg, cg, gen, jfresh, cred, bare>>  \* the rest stays
  /\ UNCHANGED <<regAuth, regOk, regBy, regPark, regNote, regFail, gone, rest>>
  /\ UNCHANGED <<doc, owedReg, st, chg, checked, chk, rolled,
                 chainOwner, up, delegOk, crashes, expiries>>

\* What a rejected request leaves of change s, sent in a request of n
\* changes: a batch goes back to be sent one change at a time (Isolate; the
\* bug parks every change of it as refused); a change sent alone waits for
\* authority when the credential it was sent with no longer held, and is
\* refused otherwise. The code asks that of the token the request carried,
\* not of one handed in since.
AfterRejection(s, n) ==
  IF n > 1 /\ Isolate THEN "staged"
  ELSE IF n = 1 /\ receipt[s] = "lapsed" THEN "awaiting"
  ELSE Refusal(s)

\* The drainer gets the receipt of its own request and lets the journal go.
\* An applied change's record is deleted; a rejected one is as
\* AfterRejection says.
JResolve ==
  /\ Journaled /\ cur # {} /\ journal = cur /\ landed /\ up  \* its request executed
  /\ LET n == Cardinality(cur)                             \* how many changes it carried
         next == [s \in Slots |->
                    IF s \notin cur THEN st[s]
                    ELSE IF receipt[s] = "ok" THEN "settled"
                    ELSE AfterRejection(s, n)]
     IN /\ st' = next                                      \* each change ends as its receipt says
        /\ solo' = IF n > 1 /\ \E s \in cur : receipt[s] # "ok"
                   THEN solo \cup cur ELSE solo            \* a refused batch goes alone
        /\ tomb' = Noted({s \in cur : next[s] = "dropped"})  \* removals are noted
  /\ flight' = [s \in Slots |-> IF s \in cur THEN "-" ELSE flight[s]]  \* no longer in flight
  /\ journal' = {} /\ cur' = {}                            \* the journal and memory are free
  /\ UNCHANGED <<landed, noted, tx, keyed>>                \* the rest of the drain stays
  /\ UNCHANGED <<regAuth, regOk, regBy, regPark, regNote, regFail, gone, late,
                 regVia, acked>>
  /\ UNCHANGED <<doc, owedReg, chg, receipt, effects, checked, chk, rolled,
                 chainOwner, chainTuples, up, delegOk, crashes, expiries>>

\* Someone other than the drainer resolves the request in the journal: the
\* provider at start, another command before its own, or the drainer's
\* own pass after a wait for it was dropped. It keeps a note of the
\* outcome under the request's digest, with the sequence the request was
\* signed with (Note; the bug keeps none). A drainer that held the request
\* no longer finds it journaled and decides by what is stored.
JRecover ==
  /\ Journaled /\ journal # {} /\ landed /\ up              \* an executed request
  /\ noted' = IF Note                                      \* its outcome is noted,
              THEN noted \cup
                     {[d |-> dg[CHOOSE s \in journal : TRUE],
                       o |-> IF \A s \in journal : receipt[s] = "ok"
                             THEN "ok" ELSE "rejected",
                       fresh |-> jfresh]}
              ELSE noted                                   \* or (bug) forgotten
  /\ journal' = {}                                         \* the journal is free
  /\ cur' = IF cur = journal THEN {} ELSE cur              \* the drainer lets it go too
  /\ UNCHANGED <<flight, landed, solo, tx, keyed>>         \* the marks stay
  /\ UNCHANGED <<doc, owedReg, st, chg, receipt, effects, checked, chk, rolled,
                 chainOwner, chainTuples, up, delegOk, crashes, expiries>>
  /\ UNCHANGED more                                        \* the registration's record stays

\* A wait for the drainer's pass is dropped (a commit stops waiting): what
\* it held in memory is gone, its marks stay. It is a crash of the pass
\* alone, and counts against the same bound.
JDrop ==
  /\ Journaled /\ cur # {}                                 \* a request held
  /\ crashes < MaxCrashes                                  \* drops are not endless
  /\ crashes' = crashes + 1                                \* one more
  /\ cur' = {}                                             \* and forgotten
  /\ UNCHANGED <<flight, journal, landed, noted, solo, tx, keyed>>  \* the rest stays
  /\ UNCHANGED <<doc, owedReg, st, chg, receipt, effects, checked, chk, rolled,
                 chainOwner, chainTuples, up, delegOk, expiries>>
  /\ UNCHANGED more                                        \* the registration's record stays

\* The drainer finds a marked change's request still in the journal, by its
\* hash or by its digest, and takes it up again: JLand and JResolve follow.
JAdopt ==
  /\ Journaled /\ cur = {} /\ journal # {}                 \* idle, a request journaled
  /\ \E s \in journal : st[s] = "inflight" /\ flight[s] # "-"  \* its own
  /\ cur' = journal                                        \* held again
  /\ UNCHANGED <<flight, journal, landed, noted, solo, tx, keyed>>  \* the rest stays
  /\ UNCHANGED <<doc, owedReg, st, chg, receipt, effects, checked, chk, rolled,
                 chainOwner, chainTuples, up, delegOk, crashes, expiries>>
  /\ UNCHANGED more                                        \* the registration's record stays

\* The note's entries for marked change s: those under its request's
\* digest whose sequence reaches the floor stored with its mark. Without the
\* floor (the bug) an earlier request with the same bytes counts too. With
\* the hash stored the code reads the entry of that transaction alone; a
\* request with the same bytes signed after the mark would have to carry a
\* later change of the same tuple, which waits behind this one.
NotedFor(s) ==
  {e \in noted : e.d = dg[s] /\ (Floor => s \in e.fresh)}

\* What is known of marked change s with its request out of the journal: a
\* receipt found by the stored hash, else the note, else nothing.
Known(s) ==
  IF flight[s] = "hash" /\ receipt[s] # "none" THEN receipt[s]
  ELSE IF NotedFor(s) = {} THEN "none"
  ELSE (CHOOSE e \in NotedFor(s) : TRUE).o

\* The drainer decides a marked change no request in memory or in the
\* journal holds. Applied: its record is deleted. Rejected: it had no
\* effect, and is sent again in a request of its own, where a refusal is
\* its own. Nothing known: it never landed, as far as the node can tell,
\* and is sent again. That is where a receipt or note that can no longer
\* be read gives a second effect.
JDecide(s) ==
  /\ Journaled /\ cur = {} /\ up                            \* idle, the provider answers
  /\ st[s] = "inflight" /\ flight[s] # "-"                  \* a marked change,
  /\ s \notin journal                                      \* not journaled
  /\ st' = [st EXCEPT ![s] = IF Known(s) = "ok" THEN "settled" ELSE "staged"]
  /\ solo' = IF Known(s) \in {"rejected", "lapsed"}       \* rejected: alone next
             THEN solo \cup {s} ELSE solo
  /\ flight' = [flight EXCEPT ![s] = "-"]                  \* the mark is cleared
  /\ UNCHANGED <<journal, landed, noted, tx, cur, keyed>>   \* the rest of the drain stays
  /\ UNCHANGED <<doc, owedReg, chg, receipt, effects, checked, chk, rolled,
                 chainOwner, chainTuples, up, delegOk, crashes, expiries>>
  /\ UNCHANGED more                                        \* the registration's record stays

\* A change whose delegation expired waits for a fresh one. One in flight
\* may do so only when it is known not to have landed.
AwaitAuthority(s) ==
  /\ ~Valid(s)                                            \* it cannot be signed
  /\ st[s] = "staged" \/ (st[s] = "inflight" /\ checked[s])  \* not sent, or known lost
  /\ st' = [st EXCEPT ![s] = "awaiting"]                  \* parked until reauthorized
  /\ checked' = [checked EXCEPT ![s] = FALSE]             \* nothing to remember
  /\ UNCHANGED <<doc, owedReg, chg, receipt, effects, rolled,
                 chainOwner, chainTuples, up, delegOk, crashes, expiries>>
  /\ UNCHANGED chk                                       \* what was checked stays so
  /\ UNCHANGED more                                      \* and so do the registration's record and what was abandoned
  /\ UNCHANGED drain                                     \* the journaled drain is not involved

\* The change's actor hands the node a fresh delegation, checked as the one
\* it was staged with: the change is owed again. A fresh one in place of an
\* expired one is another generation; a request journaled with the old one
\* still carries the old one.
Reauthorize(s) ==
  /\ st[s] = "awaiting"                                   \* it waits for authority
  /\ st' = [st EXCEPT ![s] = "staged"]                    \* owed again
  /\ delegOk' = TRUE                                      \* with a valid delegation
  /\ gen' = IF Journaled /\ ~delegOk THEN gen + 1 ELSE gen  \* a new one, where it matters
  /\ UNCHANGED <<doc, owedReg, chg, receipt, effects, checked, rolled,
                 chainOwner, chainTuples, up, crashes, expiries>>
  /\ UNCHANGED chk                                       \* what was checked stays so
  /\ UNCHANGED more                                      \* and so do the registration's record and what was abandoned
  /\ UNCHANGED <<flight, journal, landed, noted, solo, tx, cur, dg, cg, jfresh, stale, cred, bare>>

\* A change is abandoned: its record is deleted without the provider holding
\* it. Only a change the provider refused may go (AbandonStates = "refused"):
\* the provider answered it, so nothing of it is still on the way. One that
\* is owed, in flight or waiting for authority may not: a request sent for it
\* earlier can still land. The bug ("any") lets those go too. Only the actor
\* that staged the change may abandon it (AbandonBy = "actor"); the bug
\* ("anyone") lets whoever asks, the subject of a delete included.
\*
\* What it does to the deny overlay: an abandoned delete no longer denies.
\* Its subject is answered by the provider again, which refused the delete
\* and so still holds the tuple. That is right exactly when the delete's own
\* actor took it back (Withdrawn): the delete then counts for nothing, as if
\* it had never committed.
Abandon(s, who) ==
  /\ \/ st[s] = "refused"                                 \* the provider said no,
     \/ AbandonStates = "any" /\ Live(s)                  \* or (bug) it is still owed
  /\ who = chg[s].actor \/ AbandonBy = "anyone"           \* its own actor (or, bug, anyone)
  /\ st' = [st EXCEPT ![s] = "abandoned"]                 \* the record is deleted
  /\ gone' = [gone EXCEPT ![s] = who]                     \* remembered, for the properties
  /\ tomb' = Noted({s})                                   \* and noted for its actor
  /\ UNCHANGED <<regAuth, regOk, regBy, regPark, regNote, regFail, late,
                 regVia, acked>>
  /\ UNCHANGED <<doc, owedReg, chg, receipt, effects, checked, chk, rolled,
                 chainOwner, chainTuples, up, delegOk, crashes, expiries>>
  /\ UNCHANGED drain                                     \* the journaled drain is not involved

\* The actor of a discarded change is told what became of it (the wait for
\* its transaction, or the answer to its fresh credential): the note is
\* deleted.
Acknowledge(s) ==
  /\ s \in tomb                                           \* there is a note
  /\ tomb' = tomb \ {s}                                   \* it is deleted,
  /\ acked' = acked \cup {s}                              \* its actor having been told
  /\ UNCHANGED <<regAuth, regOk, regBy, regPark, regNote, regFail, gone, late, regVia>>
  /\ UNCHANGED <<doc, owedReg, st, chg, receipt, effects, checked, chk, rolled,
                 chainOwner, chainTuples, up, delegOk, crashes, expiries>>
  /\ UNCHANGED drain                                     \* the journaled drain is not involved

\* ---- The world around the node ----

\* The stored delegation expires.
Expire ==
  /\ "deleg" \in Auths                                    \* delegations are in use
  /\ delegOk /\ expiries < MaxExpiries                    \* valid, and expiries not endless
  /\ delegOk' = FALSE                                     \* no longer valid
  /\ expiries' = expiries + 1                             \* one more
  /\ UNCHANGED <<doc, owedReg, st, chg, receipt, effects, checked, rolled,
                 chainOwner, chainTuples, up, crashes>>
  /\ UNCHANGED chk                                       \* what was checked stays so
  /\ UNCHANGED more                                      \* and so do the registration's record and what was abandoned
  /\ UNCHANGED drain                                     \* the journaled drain is not involved

\* The provider becomes unreachable.
ProviderDown ==
  /\ up /\ up' = FALSE                                    \* it stops answering
  /\ UNCHANGED <<doc, owedReg, st, chg, receipt, effects, checked, rolled,
                 chainOwner, chainTuples, delegOk, crashes, expiries>>
  /\ UNCHANGED chk                                       \* what was checked stays so
  /\ UNCHANGED more                                      \* and so do the registration's record and what was abandoned
  /\ UNCHANGED drain                                     \* the journaled drain is not involved

\* The provider comes back.
ProviderUp ==
  /\ ~up /\ up' = TRUE                                    \* it answers again
  /\ UNCHANGED <<doc, owedReg, st, chg, receipt, effects, checked, rolled,
                 chainOwner, chainTuples, delegOk, crashes, expiries>>
  /\ UNCHANGED chk                                       \* what was checked stays so
  /\ UNCHANGED more                                      \* and so do the registration's record and what was abandoned
  /\ UNCHANGED drain                                     \* the journaled drain is not involved

\* The receipt of a landed change can no longer be read (the provider pruned
\* it, the note's entry for it was pushed out) while the node still has the
\* change in flight and its request is out of the worker's journal.
\* Possible only without AssumeReceiptRetrievable.
ReceiptLost(s) ==
  /\ ~AssumeReceiptRetrievable                            \* the assumption is off
  /\ st[s] = "inflight" /\ receipt[s] = "ok"              \* landed, not yet noticed
  /\ s \notin journal                                     \* and no longer journaled
  /\ receipt' = [receipt EXCEPT ![s] = "none"]            \* and now unreadable,
  /\ noted' = noted \ NotedFor(s)                        \* in the note too
  /\ UNCHANGED <<doc, owedReg, st, chg, effects, checked, rolled,
                 chainOwner, chainTuples, up, delegOk, crashes, expiries>>
  /\ UNCHANGED chk                                       \* what was checked stays so
  /\ UNCHANGED more                                      \* and so do the registration's record and what was abandoned
  /\ UNCHANGED <<flight, journal, landed, solo, tx, cur, keyed>>  \* the rest of the drain stays

\* Someone else takes the document over on the provider, after the node
\* checked a change against the owner it then had. Possible only without
\* AssumeOwnerStays. It is how a change that passed the staging check can
\* still be refused.
OwnerLost ==
  /\ ~AssumeOwnerStays                                    \* the assumption is off
  /\ AssumeNoTransferWhileOwed => ~owedReg                \* not while the node still owes it
  /\ chainOwner = "c"                                     \* the creator owns it,
  /\ chainOwner' = "o"                                    \* and now another does
  /\ UNCHANGED <<doc, owedReg, st, chg, receipt, effects, checked, chk, rolled,
                 chainTuples, up, delegOk, crashes, expiries>>
  /\ UNCHANGED more                                      \* and so do the registration's record and what was abandoned
  /\ UNCHANGED drain                                     \* the journaled drain is not involved

\* Any one thing happens.
Next ==
  \/ StageCreate \/ Commit \/ Rollback \/ Crash           \* transactions and crashes
  \/ \E s \in Slots, c \in Changes : StageChange(s, c)    \* staging a change
  \/ \E s \in Slots, c \in Changes, who \in Actors, shown \in Shown :  \* or a client staging one
       StageRemote(s, c, who, shown)
  \/ (\E via \in Ways : RegisterOnChain(via)) \/ ClearOwed  \* settling the registration
  \/ (\E s \in Slots : Acknowledge(s))                    \* telling an actor of a discard
  \/ RegistrationFails \/ ExpireReg                       \* a submission of it fails
  \/ \E who \in Presenters : ReauthorizeReg(who)          \* a fresh credential for it
  \/ \E s \in Slots, who \in Abandoners : Abandon(s, who)  \* giving a change up
  \/ ~Journaled /\ \E s \in Slots :                        \* settling a change
       Mark(s) \/ Land(s) \/ SendLost(s) \/ Lookup(s)
  \/ Journaled /\ ((\E R \in SUBSET Slots : JMark(R))     \* or through the journal:
                   \/ JPrepare \/ JHash \/ JLand          \*   building, sending,
                   \/ JResolve \/ JRecover \/ JDrop       \*   resolving, resolved by another,
                   \/ JAdopt \/ \E s \in Slots : JDecide(s))  \*   taken up, and decided
  \/ \E s \in Slots : AwaitAuthority(s) \/ Reauthorize(s)  \* authority
  \/ \E s \in Slots : ReceiptLost(s)                      \* a receipt disappears
  \/ Expire \/ ProviderDown \/ ProviderUp \/ OwnerLost    \* the world

\* Safety: start, then take steps. Nothing is promised to happen.
Spec == Init /\ [][Next]_vars

\* Liveness adds what is promised. The provider comes back, and a step that
\* needs it and can be taken again and again is taken: it registers, lands a
\* send and answers a lookup. The loop takes its turns. Fresh authority is
\* supplied to a change that waits for it. Transactions, crashes, outages
\* and expiries are promised nothing.
SpecLive ==
  /\ Spec                                                 \* all of the above
  /\ WF_vars(ProviderUp)                                  \* the provider comes back
  /\ SF_vars(RegisterOnChain("record")) /\ SF_vars(ClearOwed)  \* the registration settles
  /\ WF_vars(RegistrationFails)                           \* one without authority is parked,
  /\ WF_vars(ReauthorizeReg("c"))                         \* and its creator reauthorizes it
  /\ \A s \in Slots : WF_vars(Mark(s))                    \* the loop marks,
  /\ \A s \in Slots : SF_vars(Land(s))                    \* a send lands,
  /\ \A s \in Slots : SF_vars(Lookup(s))                  \* a lookup is answered,
  /\ \A s \in Slots : WF_vars(AwaitAuthority(s))          \* an expired change is parked,
  /\ \A s \in Slots : WF_vars(Reauthorize(s))             \* and fresh authority arrives

\* Liveness of the journaled drain: the same promises about the provider,
\* the registration and authority, and the drainer takes its turns by
\* itself (the drainer task, woken by commits and by its back-off). A
\* request in the journal lands and is resolved, by the drainer or by
\* another, and a marked change is decided.
SpecLiveJournaled ==
  /\ Spec                                                 \* all of the above
  /\ WF_vars(ProviderUp)                                  \* the provider comes back
  /\ SF_vars(RegisterOnChain("record")) /\ SF_vars(ClearOwed)  \* the registration settles
  /\ WF_vars(RegistrationFails)                           \* one without authority is parked,
  /\ WF_vars(ReauthorizeReg("c"))                         \* and its creator reauthorizes it
  /\ \A R \in SUBSET Slots : WF_vars(JMark(R))            \* the drainer builds a request,
  /\ WF_vars(JPrepare) /\ WF_vars(JHash)                   \* journals it and stores its hash,
  /\ SF_vars(JLand) /\ SF_vars(JResolve)                   \* it lands and is resolved,
  /\ SF_vars(JRecover) /\ WF_vars(JAdopt)                  \* or someone else resolves it,
  /\ \A s \in Slots : SF_vars(JDecide(s))                 \* a marked change is decided,
  /\ \A s \in Slots : WF_vars(AwaitAuthority(s))          \* an expired change is parked,
  /\ \A s \in Slots : WF_vars(Reauthorize(s))             \* and fresh authority arrives

\* ---- What must hold ----

\* The node never honours more than was intended, nor more than the provider
\* will give once everything is settled.
INV_NoWidening ==
  \A who \in Readers : Honours(who) => IntendedAllows(who) /\ SettledAllows(who)

\* The last committed change on a tuple, or "none".
LastOp(t) ==
  LET S == {s \in Slots : Committed(s) /\ chg[s].subj = t}  \* the committed changes on it
  IN IF S = {} THEN "none" ELSE chg[Last(S)].op             \* the latest one's kind
\* The node honours this reader, and would not without this tuple.
Through(who, t) ==
  Honours(who) /\ ~HonoursUsing(who, chainTuples \ {t}, OwedGranted \ {t})
\* Once a delete of a tuple committed and nothing added it again, no access
\* the node honours rests on that tuple.
INV_RevokedAtCommit ==
  \A t \in Subjects : LastOp(t) = "del" => \A who \in Readers : ~Through(who, t)

\* No change has been sent while the document's registration was owed: one
\* that was sent finds the document registered.
INV_RegistrationFirst ==
  (\E s \in Slots : st[s] \in {"inflight", "settled", "refused"})
    => (~owedReg /\ chainOwner # "none")

\* A change takes effect on the provider at most once.
INV_AtMostOneEffect == \A s \in Slots : effects[s] <= 1

\* Nothing is owed for a transaction that rolled back: no registration
\* without a committed document, no record of a rolled-back change.
INV_RollbackLeavesNothing ==
  /\ owedReg => doc = "exists"                            \* owed only for a real document
  /\ \A s \in rolled : st[s] = "void"                     \* a rolled-back change is gone

\* While the registration is owed, the document is read only by its creator
\* and by identities the creator granted by name.
INV_OwedDocumentNarrow ==
  owedReg => \A who \in Readers :
    Honours(who) => \/ who = "c"                          \* the creator,
                    \/ \E s \in Slots :                   \* or someone the creator
                         /\ Committed(s) /\ chg[s].op = "add"  \* granted,
                         /\ chg[s].actor = "c" /\ chg[s].subj = who  \* by name

\* Every delete the node stores was staged by an actor whose authority was
\* checked: nobody denies a reader on an unchecked word.
INV_DenialsChecked ==
  \A s \in Slots : Kept(s) /\ chg[s].op = "del" => chk[s]

\* Every change parked as refused was checked at staging: an unchecked one
\* the provider refuses is removed, so it holds no tuple.
INV_RefusedChecked ==
  \A s \in Slots : st[s] = "refused" => chk[s]

\* A parked change never holds a later one unless its actor was checked at
\* staging: an actor nobody vouched for cannot keep the owner's change from
\* the provider.
INV_UncheckedYields ==
  \A r, s \in Slots : Holds(r, s) => chk[r]

\* The registration reaches the provider only on its creator's word: what
\* its record holds, the delegation recorded with a change the creator still
\* owes, or the credential a request carries for the creator. Never on a
\* token the process holds for the creator.
INV_PresentedAuthority == regVia # "ambient"

\* What failed says so. A registration whose submission failed keeps the
\* reason in its record for as long as it is owed. A change whose record was
\* deleted without the provider holding it (refused though nobody checked
\* it, passed by a later change, abandoned) leaves a note until its actor
\* was told: no caller reads "nothing is owed" as "the provider holds it".
INV_FailureReported ==
  /\ (owedReg /\ regFail) => regNote
  /\ \A s \in Slots :
       (st[s] \in {"dropped", "abandoned"} /\ s \notin acked) => s \in tomb

\* A change is parked as refused only when the provider refuses it on its
\* own: its actor does not own the document. A change that was refused only
\* because it shared a request with another one is never parked.
INV_RefusedOnItsOwn ==
  \A s \in Slots : st[s] = "refused" => chg[s].actor # chainOwner

\* Nothing of an abandoned change reaches the provider afterwards.
INV_AbandonIsFinal == late = {}

\* A change whose record was deleted as settled took effect on the provider:
\* nobody's outcome but its own request's decides it.
INV_SettledTookEffect == \A s \in Slots : st[s] = "settled" => effects[s] >= 1

\* A change a client staged acts as the DID whose verified credential it
\* presented: nobody stages a change, or a denial, on another's word.
INV_ActsAsItsCredential ==
  \A s \in Slots : Kept(s) /\ cred[s] # "-" => chg[s].actor = cred[s]

\* Every change a client staged carries an authority the node can send it
\* with: no change is owed that the node can never send.
\* A change signed with the node's key names an actor whose key the node
\* holds, and one signed with a delegation was staged by a client that
\* presented a delegation, not an identity token.
INV_SendableAuthority ==
  \A s \in Slots : Kept(s) /\ cred[s] # "-" =>
    IF chg[s].auth = "node" THEN chg[s].actor \in HeldKeys  \* the node holds the actor's key
    ELSE s \notin bare                                       \* or the client gave a delegation

\* A journaled request takes effect only under the delegation it carries,
\* while that one is valid: a fresh delegation handed in later renews no
\* request already signed.
INV_LandsUnderItsCredential == stale = {}

\* A parked change of the same tuple, staged earlier, holds this one.
Held(s) == \E r \in Slots : Holds(r, s)

\* Every committed change ends settled, or parked as refused or as awaiting
\* authority, or staged behind a parked change of its tuple, and stays so.
LIVE_Drains ==
  <>[](\A s \in Slots : Committed(s) =>
         \/ st[s] \in {"settled", "refused", "awaiting"}   \* delivered or parked,
         \/ st[s] = "staged" /\ Held(s))                   \* or held behind a parked one

====
