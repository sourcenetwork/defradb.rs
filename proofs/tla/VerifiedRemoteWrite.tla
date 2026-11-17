-------------------------- MODULE VerifiedRemoteWrite --------------------------
\* The story, in plain words.
\*
\* A User signs one create, update or delete. An embedder checks that signature and
\* hands the write to a Node, to run in a transaction the Node owns. The Node's
\* document ACP is a remote provider: it cannot join the transaction, so what
\* the write needs from it is written into the transaction as "owed" and sent
\* to the provider after the commit. A create owes one command, the
\* registration of its document. The embedder may also grant somebody access
\* in the same transaction, which owes a second command.
\*
\* The provider only takes a command from the Node when the Node may act for
\* the User. The Node may act for the User in three ways:
\*   - it holds the User's key;
\*   - the User signed a delegation: "this Node may send my policy commands";
\*   - the User signed a delegation bound to ONE request: "this Node may send
\*     exactly this command, once".
\* The embedder hands the Node the credential beside the write. The write and
\* the grant each carry their own.
\*
\* What can happen: the embedder presents no credential, the wrong one, one
\* signed by somebody else, one made for another Node, or one that names another
\* command. A credential can expire at any moment. The same token can be shown
\* to the Node's HTTP door as if it were a login.
\*
\* After the commit a submission can fail: the provider cannot be reached, or
\* the credential recorded for the command has run out. The commit stands. The
\* command stays owed, or is parked when no credential lets the Node send it,
\* and its record says why. The embedder then hands the Node a fresh credential
\* for that one command. The Node checks it as it checked the first one, with
\* the same rule, records it with the command and sends the command under it.
\*
\* An update or a delete (both called "update" below) sends the provider no
\* command and owes it nothing. The User's signature on its blocks already says
\* who wrote it. What decides it is the provider's own answer to "may this User
\* update (or delete) this document?", read and verified by the Node against
\* the provider's finalized state at one revision: the newest revision the
\* Node has verified. That proof is "fresh" while its revision is younger than
\* the freshness limit (30 seconds for vera.rs), and it says what the
\* permission was AT THAT REVISION, not what it is now. Evidence can grow
\* "stale" (older than the freshness limit) or be "unreachable" (the provider
\* does not answer). The owner can revoke the User's permission at any moment,
\* before or after the write is admitted. A revocation finalized after the
\* revision the Node last verified is not in its proof: until the Node verifies
\* a newer revision, or the proof ages past the limit, an update can still be
\* admitted on the proof that shows the permission. That window is the
\* freshness bound, and the model lets it happen. On this Node a registration of the document can still be owed
\* (the provider does not know the document yet: only its recorded creator may
\* touch it), and a revocation can still be owed (it denies the User whatever
\* the provider says). With an update the credential only says who is asking:
\*   - none: the signature on the blocks is enough, when the provider proves its
\*     decisions (vera.rs); a provider that does not (Cosmos) still needs the key;
\*   - a delegation: it must be good, as for any command;
\*   - a request-bound delegation: it names a command, an update sends none, so
\*     it is refused.
\* A revocation that lands after admission does not undo the write: the write
\* commits. That is the rule, not a race the model hides.
\*
\* What must never happen:
\*   - a write is recorded although the Node could not act for the User;
\*   - an update is admitted unless the User held the permission at a
\*     verified revision no older than the freshness limit, or it is admitted
\*     on stale or missing evidence, or past a revocation or a registration
\*     still owed on this Node;
\*   - an update is admitted with a request-bound delegation;
\*   - an update leaves something owed to the provider;
\*   - a refused write leaves a row or an owed record behind;
\*   - a request-bound delegation is used for any command but the one it names;
\*   - a request-bound delegation logs a caller in at the HTTP door;
\*   - a command is sent under a credential that has expired;
\*   - a grant reaches the provider before its document's registration;
\*   - a command is sent with anything but the credential recorded for it,
\*     such as a token the process happens to hold for the User;
\*   - a parked command is owed again on a fresh credential that would not
\*     have admitted it;
\*   - a submission fails after the commit and the record of what is owed
\*     does not say so.
\*
\* The provider's own checks (vera.rs `authorize_delegation`) are not modelled:
\* this is what the Node does before it sends anything. A provider that gives
\* the Node nothing to check a delegation against (it takes no delegation of
\* this kind) is the case where bySigner and forNode can never be found TRUE:
\* there a verified write is staged with a key the Node holds, or not at all.
\* Such a provider proves none of its decisions either (proven is FALSE), so an
\* update there still needs the key.
\*
\* Rust owners: crates/defra-node/src/lib.rs execute_verified_graph_in_txn;
\* crates/defra-node/src/signer_credential.rs SignerCredential::carried_by;
\* crates/query/src/runner/remote_authority.rs remote_write_authority,
\* edit_credential;
\* crates/query/src/runner/mutation.rs QueryRunner::authorize_mutation (the
\* permission, for every credential form);
\* crates/query/src/txn/context.rs check_doc_access_with_prepared_inputs;
\* crates/acp/src/external.rs ExternalDocumentAcp::check_doc_access (owed
\* registration and owed revocation);
\* crates/vera/src/cosmos/dac.rs VeraDocumentACP::check_doc_access,
\* proves_decisions;
\* crates/vera/src/vera_rs/provider.rs verify_access, decision_state;
\* crates/vera/src/cosmos/dac.rs authority;
\* crates/vera/src/cosmos/dac/request_bound.rs request_bound, admit_for;
\* crates/vera/src/vera_rs/bearer/request.rs caller_of_request_delegation;
\* crates/vera/src/vera_rs/bearer.rs caller_of_delegation (the HTTP door);
\* crates/vera/src/cosmos/dac/owed.rs token, lapsed;
\* crates/acp/src/outbox/staging.rs TransactionOutbox::stage;
\* crates/acp/src/external/settle.rs register_first;
\* crates/acp/src/pending.rs PendingRegistration (authority, state, last_error);
\* crates/acp/src/external/registration.rs settle_with, registrar,
\* note_failure;
\* crates/acp/src/external/owed.rs reauthorize, entries;
\* crates/db/src/write/autocommit/helpers.rs register_created_doc;
\* crates/defra-node/src/owed.rs Node::reauthorize_owed_acp_as, Node::owed_acp_as.
EXTENDS Naturals, FiniteSets

\* Each RED configuration plants exactly one bug by naming it here.
CONSTANT Bug

\* The two commands a verified transaction can owe the provider.
Commands == {"register", "grant"}
\* What a request-bound delegation can name: one of those two commands for this
\* document, or a command for some other document or relation.
Named == {"register", "grant", "other"}
\* The forms a credential comes in. "none" means the embedder handed nothing.
Forms == {"none", "delegation", "request"}

VARIABLES
  kind,        \* what the User signed: "create" or "update"
  holdsKey,    \* TRUE when the Node holds the User's key
  writeForm,   \* the form of the credential handed in with the write
  writeNames,  \* the command that credential names, when it is request-bound
  grantForm,   \* the form of the credential handed in with the grant
  grantNames,  \* the command that credential names, when it is request-bound
  bySigner,    \* TRUE when the credentials were signed by the User
  forNode,     \* TRUE when the credentials name this Node and its deployment
  current,     \* TRUE while the credentials have not expired
  phase,       \* "ready", "staged", "refused" or "committed"
  rows,        \* TRUE once the document row is written in the transaction
  owed,        \* the commands recorded as owed to the provider
  sent,        \* the commands the Node has sent to the provider
  parked,      \* the commands waiting for a new credential
  misused,     \* TRUE once a request-bound token was sent with another command
  stale,       \* TRUE once a command was sent under an expired credential
  http,        \* the HTTP door: "idle", "in" or "out"
  fresh,       \* per command: the credential handed in for it after it was parked
  freshLive,   \* per command: TRUE while that fresh credential has not expired
  failed,      \* the owed or parked commands whose last submission failed
  noted,       \* the commands whose record says why their last submission failed
  unbacked,    \* TRUE once a command was sent though nothing recorded for it allowed it
  proven,      \* TRUE when the provider proves its decisions (vera.rs), FALSE for Cosmos
  permitted,   \* TRUE while the provider's newest finalized state gives the User the permission
  seen,        \* the permission at the revision the Node last verified: what its proof shows
  evidence,    \* the age of that proof: "fresh", "stale", or "unreachable" (no answer)
  owedReg,     \* a registration of the document owed on this Node: "none", "signer", "other"
  owedRevoke   \* TRUE when a revocation owed on this Node names the User on the document

\* What is recorded after the commit about each command.
after == <<fresh, freshLive, failed, noted, unbacked>>

\* What decides an update: the provider, the evidence, and what this Node owes.
edit == <<proven, permitted, seen, evidence, owedReg, owedRevoke>>

\* Every variable, so a step can say which ones it leaves alone.
vars == <<kind, holdsKey, writeForm, writeNames, grantForm, grantNames, bySigner,
          forNode, current, phase, rows, owed, sent, parked, misused, stale, http,
          after, edit>>

\* No fresh credential was handed in for a command.
NoCred == [form |-> "none", names |-> "other", mine |-> FALSE]
\* The fresh credentials an embedder may hand in: either kind of delegation,
\* naming any command, signed by the User for this Node (mine) or not.
FreshCreds == [form : {"delegation", "request"}, names : Named, mine : BOOLEAN]

\* The facts about one run are chosen freely at the start; nothing is recorded.
\* An update stages no grant and a create reads no permission, so each kind
\* chooses only the facts that matter to it: this keeps the run small without
\* hiding a case.
Init ==
  /\ kind \in {"create", "update"}            \* the User signed either kind
  /\ holdsKey \in BOOLEAN                     \* the Node may or may not hold the key
  /\ writeForm \in Forms                      \* any form may come with the write
  /\ writeNames \in Named                     \* and may name any command
  /\ IF kind = "create"
       THEN /\ grantForm \in Forms             \* any form may come with the grant
            /\ grantNames \in Named            \* and may name any command
            /\ proven = TRUE                   \* a create is decided by its credential
            /\ permitted = FALSE               \* and reads no permission
            /\ seen = FALSE
            /\ evidence = "fresh"
            /\ owedReg = "none"
            /\ owedRevoke = FALSE
       ELSE /\ grantForm = "none"              \* an update stages no grant here
            /\ grantNames = "other"
            /\ proven \in BOOLEAN              \* vera.rs, or a provider that proves nothing
            /\ permitted \in BOOLEAN           \* the User may or may not hold the permission
            /\ seen \in BOOLEAN                \* the verified revision may predate a change
            /\ evidence \in {"fresh", "stale", "unreachable"} \* any evidence to start with
            /\ owedReg \in {"none", "signer", "other"} \* a registration may still be owed
            /\ owedRevoke \in BOOLEAN          \* a revocation may still be owed
  /\ bySigner \in BOOLEAN                     \* signed by the User, or by another
  /\ forNode \in BOOLEAN                      \* made for this Node, or for another
  /\ current = TRUE                           \* nothing has expired yet
  /\ phase = "ready"                          \* the write has not been handed in
  /\ rows = FALSE                             \* no row is written
  /\ owed = {}                                \* nothing is owed
  /\ sent = {}                                \* nothing was sent
  /\ parked = {}                              \* nothing waits for a credential
  /\ misused = FALSE                          \* no token was misused
  /\ stale = FALSE                            \* nothing was sent after expiry
  /\ http = "idle"                            \* nobody knocked at the HTTP door
  /\ fresh = [c \in Commands |-> NoCred]      \* no fresh credential was handed in
  /\ freshLive = [c \in Commands |-> FALSE]   \* so none is live
  /\ failed = {}                              \* no submission failed
  /\ noted = {}                               \* no record carries a reason
  /\ unbacked = FALSE                         \* nothing was sent without authority

\* A delegation is good when the User signed it, for this Node, and it has not
\* expired. Each planted bug drops one of the three checks.
Good ==
  /\ (bySigner \/ Bug = "anyIssuer")          \* signed by the User
  /\ (forNode \/ Bug = "anyNode")             \* for this Node and deployment
  /\ (current \/ Bug = "expiredAccepted")     \* not expired

\* The same three facts with no bug: what is true, not what the Node checked.
TrulyGood == bySigner /\ forNode /\ current

\* Whether the Node may send command c for the User with a credential of this
\* form naming this command. A key needs no credential. A general delegation
\* covers any policy command. A request-bound one covers only the command it
\* names; the planted bug forgets to compare.
\* `good` says whether the credential passed the three checks. This one rule
\* answers at admission, when the command is sent, and when a fresh credential
\* is handed in for a parked command.
MaySendWith(c, form, names, good) ==
  \/ holdsKey                                                   \* the Node signs itself
  \/ form = "delegation" /\ good                                \* a general delegation
  \/ form = "request" /\ good /\ (names = c \/ Bug = "anyCommand") \* bound to c
\* The rule for the credential that came with the write or the grant.
MaySend(c, form, names) == MaySendWith(c, form, names, Good)

\* The same question answered from the facts, with no bug.
TrulyMaySendWith(c, form, names, good) ==
  \/ holdsKey
  \/ form = "delegation" /\ good
  \/ form = "request" /\ good /\ names = c
TrulyMaySend(c, form, names) == TrulyMaySendWith(c, form, names, TrulyGood)

\* The credential recorded for command c now: the fresh one handed in for it
\* after it was parked, else the one its write or grant came with.
Renewed(c) == fresh[c] # NoCred
FormOf(c) == IF Renewed(c) THEN fresh[c].form
             ELSE IF c = "register" THEN writeForm ELSE grantForm
NamesOf(c) == IF Renewed(c) THEN fresh[c].names
              ELSE IF c = "register" THEN writeNames ELSE grantNames
\* Whether that credential is unexpired, in fact.
LiveOf(c) == IF Renewed(c) THEN freshLive[c] ELSE current
\* Whether it passes the three checks, as the Node checks them and in fact.
GoodOf(c) ==
  IF Renewed(c)
    THEN /\ (fresh[c].mine \/ Bug \in {"anyIssuer", "anyNode"})
         /\ (freshLive[c] \/ Bug = "expiredAccepted")
    ELSE Good
TrulyGoodOf(c) == IF Renewed(c) THEN fresh[c].mine /\ freshLive[c] ELSE TrulyGood
\* Whether the Node may send c now with what is recorded for it.
Sendable(c) == MaySendWith(c, FormOf(c), NamesOf(c), GoodOf(c))
TrulySendable(c) == TrulyMaySendWith(c, FormOf(c), NamesOf(c), TrulyGoodOf(c))

\* What the credential of an update must be. It only says who is asking:
\* nothing, when the provider proves its decisions (the blocks' signature says
\* who wrote them) or the Node holds the key; a delegation, when it is good (a
\* held key answers for it, as for any command). A request-bound delegation
\* names a command and an update sends none: refused, key or no key. The
\* planted bug takes it.
EditCredential(form) ==
  \/ form = "none" /\ (proven \/ holdsKey)                      \* the signature is enough
  \/ form = "delegation" /\ (holdsKey \/ Good)                  \* a delegation must be good
  \/ form = "request" /\ Bug = "requestEdit"                    \* (bug: a bound one is taken)

\* The same question answered from the facts, with no bug.
TrulyEditCredential(form) ==
  \/ form = "none" /\ (proven \/ holdsKey)
  \/ form = "delegation" /\ (holdsKey \/ TrulyGood)

\* What the provider answers the Node: the permission its proof shows, at the
\* revision it verified, while that revision is within the freshness limit.
\* That may be older than a revocation finalized since. Stale or missing
\* evidence answers nothing; the planted bug takes a proof past the limit.
Decision ==
  \/ evidence = "fresh" /\ seen                                 \* verified, within the limit
  \/ evidence = "stale" /\ seen /\ Bug = "staleEvidence"        \* (bug: past the limit)

\* The gate on this Node, as for a read: a registration still owed lets only
\* its creator in, a revocation still owed shuts the User out, and otherwise
\* the provider's decision stands. The planted bugs skip the whole check, or
\* forget what this Node owes.
Permit ==
  \/ Bug = "noPermission"                                       \* (bug: nothing is asked)
  \/ Bug # "owedIgnored" /\ owedReg = "signer"                  \* the owed creator
  \/ (owedReg = "none" \/ Bug = "owedIgnored")                 \* nothing owed on it
       /\ (~owedRevoke \/ Bug = "owedIgnored")                 \* no revocation owed
       /\ Decision                                            \* and the provider says yes

\* The same question answered from the facts, with no bug: the User held the
\* permission at a verified revision no older than the freshness limit.
TrulyPermit ==
  IF owedReg # "none"
    THEN owedReg = "signer"
    ELSE ~owedRevoke /\ evidence = "fresh" /\ seen

\* Whether the write is admitted: a create needs authority for its
\* registration; an update a credential that says who asks, and the
\* permission. One rule for every credential form.
WriteAdmitted ==
  \/ Bug = "noAuthority"                                        \* bug: nothing is asked
  \/ kind = "create" /\ MaySend("register", writeForm, writeNames)
  \/ kind = "update" /\ EditCredential(writeForm) /\ Permit

\* The same question answered from the facts, with no bug.
WriteTrulyAdmitted ==
  \/ kind = "create" /\ TrulyMaySend("register", writeForm, writeNames)
  \/ kind = "update" /\ TrulyEditCredential(writeForm) /\ TrulyPermit

\* The embedder hands in the write. An admitted write puts its row and, for a
\* create, the owed registration into the transaction. A refused write records
\* nothing; the planted bug writes first and checks afterwards.
Stage ==
  /\ phase = "ready"                                            \* only once
  /\ IF WriteAdmitted
       THEN /\ phase' = "staged"                                \* the write is in
            /\ rows' = TRUE                                     \* its row is written
            /\ owed' = IF kind = "create" \/ Bug = "editOwes"
                         THEN {"register"} ELSE {}              \* a create owes one command
       ELSE /\ phase' = "refused"                               \* the write is refused
            /\ rows' = (Bug = "recordedFirst")                  \* nothing is written
            /\ owed' = IF Bug = "recordedFirst" /\ kind = "create"
                         THEN {"register"} ELSE {}              \* nothing is owed
  /\ UNCHANGED <<kind, holdsKey, writeForm, writeNames, grantForm, grantNames,
                 bySigner, forNode, current, sent, parked, misused, stale, http>>
  /\ UNCHANGED after                                            \* nothing is recorded here
  /\ UNCHANGED edit                                             \* nor is the permission

\* The embedder grants access in the same transaction, on the document the
\* create made. The grant is owed only when the Node may send it; a refused
\* grant leaves the transaction as it was.
StageGrant ==
  /\ phase = "staged"                                           \* inside the transaction
  /\ kind = "create"                                            \* on the new document
  /\ "grant" \notin owed                                        \* at most once
  /\ (MaySend("grant", grantForm, grantNames) \/ Bug = "grantUnchecked") \* authority for the grant
  /\ owed' = owed \cup {"grant"}                                \* the grant is owed
  /\ UNCHANGED <<kind, holdsKey, writeForm, writeNames, grantForm, grantNames,
                 bySigner, forNode, current, phase, rows, sent, parked, misused,
                 stale, http>>
  /\ UNCHANGED after                                            \* nothing is recorded here
  /\ UNCHANGED edit                                             \* nor is the permission

\* The Node's owner commits the transaction: what is owed is now due.
Commit ==
  /\ phase = "staged"                                           \* something was staged
  /\ phase' = "committed"                                       \* the rows are durable
  /\ UNCHANGED <<kind, holdsKey, writeForm, writeNames, grantForm, grantNames,
                 bySigner, forNode, current, rows, owed, sent, parked, misused,
                 stale, http>>
  /\ UNCHANGED after                                            \* nothing is recorded here
  /\ UNCHANGED edit                                             \* nor is the permission

\* Time passes and the credentials expire. This can happen at any moment.
Expire ==
  /\ current                                                    \* not expired yet
  /\ current' = FALSE                                           \* now they are
  /\ UNCHANGED <<kind, holdsKey, writeForm, writeNames, grantForm, grantNames,
                 bySigner, forNode, phase, rows, owed, sent, parked, misused,
                 stale, http>>
  /\ UNCHANGED after                                            \* nothing is recorded here
  /\ UNCHANGED edit                                             \* nor is the permission

\* Whether sending c now would use a request-bound token for another command.
Misuse(c) == ~holdsKey /\ FormOf(c) = "request" /\ NamesOf(c) # c
\* Whether sending c now would use a credential that has expired.
Stale(c) == ~holdsKey /\ FormOf(c) # "none" /\ ~LiveOf(c)
\* What a sent command leaves of its record: nothing failed, nothing to say.
Settled(c) ==
  /\ failed' = failed \ {c}
  /\ noted' = noted \ {c}
  /\ unbacked' = (unbacked \/ ~TrulySendable(c))               \* remember an unbacked send
  /\ UNCHANGED <<fresh, freshLive>>
\* What a failed submission of c leaves: the failure, and its reason in the
\* record. The planted bug records no reason.
Failed(c) ==
  /\ failed' = failed \cup {c}
  /\ noted' = IF Bug = "silentFailure" THEN noted ELSE noted \cup {c}
  /\ UNCHANGED <<fresh, freshLive, unbacked>>
\* The registration is out of the way: sent, neither owed nor parked.
Registered == "register" \notin owed \cup parked

\* After the commit the Node sends the registration, with the credential
\* recorded for it, checked again: at the commit, at a later start, whenever
\* it is tried. The planted bugs skip the check, or send it with a token the
\* process happens to hold for the User.
SendRegistration ==
  /\ phase = "committed"                                        \* after the commit
  /\ "register" \in owed                                        \* still owed
  /\ (Sendable("register") \/ Bug \in {"sentExpired", "ambientRetry"}) \* checked again
  /\ owed' = owed \ {"register"}                                \* owed no more
  /\ sent' = sent \cup {"register"}                             \* the provider has it
  /\ misused' = (misused \/ Misuse("register"))                 \* remember a misuse
  /\ stale' = (stale \/ Stale("register"))                      \* remember a stale send
  /\ Settled("register")                                        \* nothing left to report
  /\ UNCHANGED <<kind, holdsKey, writeForm, writeNames, grantForm, grantNames,
                 bySigner, forNode, current, phase, rows, parked, http>>
  /\ UNCHANGED edit                                             \* the permission is not touched

\* A registration whose credential no longer lets the Node send it is parked:
\* it waits for a fresh credential, is not sent, and says why.
ParkRegistration ==
  /\ phase = "committed"                                        \* after the commit
  /\ "register" \in owed                                        \* still owed
  /\ ~Sendable("register")                                      \* no usable credential
  /\ Bug # "ambientRetry"                                       \* (the bug sends it instead)
  /\ owed' = owed \ {"register"}                                \* out of the queue
  /\ parked' = parked \cup {"register"}                         \* waiting for authority
  /\ Failed("register")                                         \* and saying so
  /\ UNCHANGED <<kind, holdsKey, writeForm, writeNames, grantForm, grantNames,
                 bySigner, forNode, current, phase, rows, sent, misused, stale, http>>
  /\ UNCHANGED edit                                             \* the permission is not touched

\* A submission fails though the Node may send the command: the provider
\* cannot be reached. The command stays owed and its record says why.
SendFails(c) ==
  /\ phase = "committed"                                        \* after the commit
  /\ c \in owed /\ c \notin failed                              \* owed, and this is news
  /\ (c = "grant" => Registered)                                \* its turn has come
  /\ Sendable(c)                                                \* the Node tried
  /\ Failed(c)                                                  \* and it failed
  /\ UNCHANGED <<kind, holdsKey, writeForm, writeNames, grantForm, grantNames,
                 bySigner, forNode, current, phase, rows, owed, sent, parked,
                 misused, stale, http>>
  /\ UNCHANGED edit                                             \* the permission is not touched

\* The embedder hands the Node a fresh credential for one parked command.
\* The Node admits it with the rule that admitted the write, asked about this
\* command, and records it: the command is owed again. A credential the rule
\* refuses changes nothing. The planted bug records whatever it is handed.
Reauthorize(c, cred) ==
  /\ phase = "committed"                                        \* after the commit
  /\ c \in parked                                               \* it waits for authority
  /\ \/ MaySendWith(c, cred.form, cred.names,
                     cred.mine \/ Bug \in {"anyIssuer", "anyNode"}) \* the same rule
     \/ Bug = "reauthUnchecked"                                 \* (bug: nothing is asked)
  /\ fresh' = [fresh EXCEPT ![c] = cred]                        \* recorded with the command
  /\ freshLive' = [freshLive EXCEPT ![c] = TRUE]                \* and not expired yet
  /\ parked' = parked \ {c}                                     \* no longer parked
  /\ owed' = owed \cup {c}                                      \* owed again
  /\ failed' = failed \ {c} /\ noted' = noted \ {c}             \* nothing to report
  /\ UNCHANGED <<kind, holdsKey, writeForm, writeNames, grantForm, grantNames,
                 bySigner, forNode, current, phase, rows, sent, misused, stale,
                 http, unbacked>>
  /\ UNCHANGED edit                                             \* the permission is not touched

\* A fresh credential expires too.
ExpireFresh(c) ==
  /\ freshLive[c]                                               \* not expired yet
  /\ freshLive' = [freshLive EXCEPT ![c] = FALSE]               \* now it is
  /\ UNCHANGED <<kind, holdsKey, writeForm, writeNames, grantForm, grantNames,
                 bySigner, forNode, current, phase, rows, owed, sent, parked,
                 misused, stale, http, fresh, failed, noted, unbacked>>
  /\ UNCHANGED edit                                             \* the permission is not touched

\* After the registration the Node sends the grant, its credential checked
\* again. The planted bug sends it while the registration is still owed.
SendGrant ==
  /\ phase = "committed"                                        \* after the commit
  /\ "grant" \in owed                                           \* still owed
  /\ (Registered \/ Bug = "grantFirst")                         \* registration first
  /\ (Sendable("grant") \/ Bug = "sentExpired")                 \* checked again
  /\ owed' = owed \ {"grant"}                                   \* owed no more
  /\ sent' = sent \cup {"grant"}                                \* the provider has it
  /\ misused' = (misused \/ Misuse("grant"))                    \* remember a misuse
  /\ stale' = (stale \/ Stale("grant"))                         \* remember a stale send
  /\ Settled("grant")                                           \* nothing left to report
  /\ UNCHANGED <<kind, holdsKey, writeForm, writeNames, grantForm, grantNames,
                 bySigner, forNode, current, phase, rows, parked, http>>
  /\ UNCHANGED edit                                             \* the permission is not touched

\* A grant whose credential no longer lets the Node send it is parked: it
\* waits for a new credential and is not sent. While the registration is owed
\* or parked the grant waits behind it, neither sent nor parked; once the
\* registration is with the provider the grant goes under its own credential.
ParkGrant ==
  /\ phase = "committed"                                        \* after the commit
  /\ "grant" \in owed                                           \* still owed
  /\ Registered                                                 \* its turn has come
  /\ ~Sendable("grant")                                         \* no usable credential
  /\ owed' = owed \ {"grant"}                                   \* out of the queue
  /\ parked' = parked \cup {"grant"}                            \* waiting for authority
  /\ Failed("grant")                                            \* and saying so
  /\ UNCHANGED <<kind, holdsKey, writeForm, writeNames, grantForm, grantNames,
                 bySigner, forNode, current, phase, rows, sent, misused, stale, http>>
  /\ UNCHANGED edit                                             \* the permission is not touched

\* Somebody shows the write's token at the HTTP door as a login. Only a
\* general delegation that is good logs its signer in. The planted bug lets a
\* request-bound one in too.
Knock ==
  /\ http = "idle"                                              \* once
  /\ http' = IF /\ Good
                /\ \/ writeForm = "delegation"
                   \/ (writeForm = "request" /\ Bug = "requestLogsIn")
               THEN "in" ELSE "out"                             \* let in, or turned away
  /\ UNCHANGED <<kind, holdsKey, writeForm, writeNames, grantForm, grantNames,
                 bySigner, forNode, current, phase, rows, owed, sent, parked,
                 misused, stale>>
  /\ UNCHANGED after                                            \* nothing is recorded here
  /\ UNCHANGED edit                                             \* nor is the permission

\* The owner revokes the User's permission and the provider finalizes it. This
\* can happen at any moment, also after the update was admitted: the update
\* then still commits. The Node's proof is of an earlier revision and still
\* shows the permission until the Node verifies a newer one (Observe) or the
\* proof ages past the limit (Age).
Revoke ==
  /\ permitted                                                  \* the User holds it now
  /\ permitted' = FALSE                                         \* and no longer does
  /\ UNCHANGED <<kind, holdsKey, writeForm, writeNames, grantForm, grantNames,
                 bySigner, forNode, current, phase, rows, owed, sent, parked,
                 misused, stale, http>>
  /\ UNCHANGED after                                            \* nothing is recorded here
  /\ UNCHANGED <<proven, seen, evidence, owedReg, owedRevoke>>

\* The Node verifies the provider's newest finalized revision: its proof is
\* fresh again and shows the permission as that revision has it.
Observe ==
  /\ evidence' = "fresh"                                        \* a new verified revision
  /\ seen' = permitted                                          \* showing the state as it is
  /\ UNCHANGED <<kind, holdsKey, writeForm, writeNames, grantForm, grantNames,
                 bySigner, forNode, current, phase, rows, owed, sent, parked,
                 misused, stale, http>>
  /\ UNCHANGED after                                            \* nothing is recorded here
  /\ UNCHANGED <<proven, permitted, owedReg, owedRevoke>>

\* The newest verified state grows older than the freshness limit: the
\* evidence goes stale. Or the provider stops answering.
Age ==
  /\ evidence = "fresh"                                         \* fresh until now
  /\ evidence' \in {"stale", "unreachable"}                     \* now stale, or gone
  /\ UNCHANGED <<kind, holdsKey, writeForm, writeNames, grantForm, grantNames,
                 bySigner, forNode, current, phase, rows, owed, sent, parked,
                 misused, stale, http>>
  /\ UNCHANGED after                                            \* nothing is recorded here
  /\ UNCHANGED <<proven, permitted, seen, owedReg, owedRevoke>>

\* Any of the steps above may come next.
Next == Stage \/ StageGrant \/ Commit \/ Expire \/ SendRegistration \/ SendGrant
        \/ ParkRegistration \/ ParkGrant \/ Knock \/ Revoke \/ Observe \/ Age
        \/ (\E c \in Commands : SendFails(c) \/ ExpireFresh(c))
        \/ (\E c \in Commands, cred \in FreshCreds : Reauthorize(c, cred))

\* The run: start in Init, then take steps or wait.
Spec == Init /\ [][Next]_vars

\* Every variable holds a value of its kind.
TypeOK ==
  /\ kind \in {"create", "update"}
  /\ holdsKey \in BOOLEAN /\ bySigner \in BOOLEAN /\ forNode \in BOOLEAN
  /\ current \in BOOLEAN /\ rows \in BOOLEAN
  /\ misused \in BOOLEAN /\ stale \in BOOLEAN
  /\ writeForm \in Forms /\ grantForm \in Forms
  /\ writeNames \in Named /\ grantNames \in Named
  /\ phase \in {"ready", "staged", "refused", "committed"}
  /\ owed \subseteq Commands /\ sent \subseteq Commands /\ parked \subseteq Commands
  /\ http \in {"idle", "in", "out"}
  /\ fresh \in [Commands -> FreshCreds \cup {NoCred}]
  /\ freshLive \in [Commands -> BOOLEAN]
  /\ failed \subseteq Commands /\ noted \subseteq Commands
  /\ unbacked \in BOOLEAN
  /\ proven \in BOOLEAN /\ permitted \in BOOLEAN /\ seen \in BOOLEAN
  /\ evidence \in {"fresh", "stale", "unreachable"}
  /\ owedReg \in {"none", "signer", "other"} /\ owedRevoke \in BOOLEAN

\* A write is staged only when the Node could really act for the User at that
\* moment. Checked on the step that stages it, since a credential may expire
\* later.
INV_AuthorityAtAdmission ==
  [][(phase = "ready" /\ phase' = "staged") => WriteTrulyAdmitted]_vars

\* A refused write leaves no row and nothing owed.
INV_RefusalRecordsNothing == phase = "refused" => (~rows /\ owed = {})

\* A request-bound delegation is never sent with a command it does not name.
INV_RequestOnlyItsRequest == ~misused

\* A request-bound delegation never logs a caller in at the HTTP door.
INV_RequestIsNoLogin == (http = "in") => writeForm # "request"

\* Nothing is sent under a credential that has expired.
INV_NothingSentStale == ~stale

\* A grant is never with the provider while its document's registration is
\* still owed.
INV_RegistrationFirst == ("grant" \in sent) => Registered

\* A command is sent only when what is recorded for it lets the Node send it:
\* never with a token the process holds for the User and nobody presented for
\* this command.
INV_SentUnderRecordedAuthority == ~unbacked

\* A parked command is owed again only on a fresh credential that really lets
\* the Node send that command: the rule of admission, asked again.
INV_ReauthorizedWithAuthority ==
  [][\A c \in Commands :
       (c \in parked /\ c \notin parked') => TrulySendable(c)']_vars

\* A command whose submission failed says so in its record for as long as it
\* is owed or parked.
INV_FailureReported == failed \subseteq noted

\* A grant is owed only when the Node could really send it when it was staged.
INV_GrantAuthority ==
  [][("grant" \notin owed \cup parked /\ "grant" \in owed') =>
       TrulyMaySend("grant", grantForm, grantNames)]_vars

\* An update or a delete is admitted only when the User held the permission
\* at a verified revision no older than the freshness limit, with no
\* revocation owed against the User on this Node; or, while the document's
\* registration is still owed here, only for its creator. Checked on the step
\* that admits it. A revocation finalized after that revision is not promised.
INV_EditOnVerifiedPermission ==
  [][(phase = "ready" /\ phase' = "staged" /\ kind = "update") => TrulyPermit]_vars

\* An update or a delete is never admitted with a request-bound delegation:
\* it names a command, and these writes send none.
INV_RequestBoundNoEdit ==
  [][(phase = "ready" /\ phase' = "staged" /\ kind = "update") =>
       writeForm # "request"]_vars

\* Not a promise: what the freshness window allows. An update is never admitted
\* while the User's permission is already revoked on the provider. The model
\* must break this (MC_VerifiedRemoteWrite_Red_WindowWitness), or it could not
\* show the admission the code makes on a proof that predates a revocation.
NotPastRevocation ==
  [][(phase = "ready" /\ phase' = "staged" /\ kind = "update" /\ owedReg = "none")
       => permitted]_vars

\* An update or a delete leaves nothing owed to the provider.
INV_EditOwesNothing == kind = "update" => owed = {}
=============================================================================
