-------------------------- MODULE VerifiedRemoteWrite --------------------------
\* The story, in plain words.
\*
\* A User signs one create or one update. An embedder checks that signature and
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
\* What must never happen:
\*   - a write is recorded although the Node could not act for the User;
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
\*
\* Rust owners: crates/defra-node/src/lib.rs execute_verified_graph_in_txn;
\* crates/defra-node/src/signer_credential.rs SignerCredential::carried_by;
\* crates/query/src/runner/remote_authority.rs remote_write_authority;
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
  unbacked     \* TRUE once a command was sent though nothing recorded for it allowed it

\* What is recorded after the commit about each command.
after == <<fresh, freshLive, failed, noted, unbacked>>

\* Every variable, so a step can say which ones it leaves alone.
vars == <<kind, holdsKey, writeForm, writeNames, grantForm, grantNames, bySigner,
          forNode, current, phase, rows, owed, sent, parked, misused, stale, http,
          after>>

\* No fresh credential was handed in for a command.
NoCred == [form |-> "none", names |-> "other", mine |-> FALSE]
\* The fresh credentials an embedder may hand in: either kind of delegation,
\* naming any command, signed by the User for this Node (mine) or not.
FreshCreds == [form : {"delegation", "request"}, names : Named, mine : BOOLEAN]

\* The facts about one run are chosen freely at the start; nothing is recorded.
Init ==
  /\ kind \in {"create", "update"}            \* the User signed either kind
  /\ holdsKey \in BOOLEAN                     \* the Node may or may not hold the key
  /\ writeForm \in Forms                      \* any form may come with the write
  /\ writeNames \in Named                     \* and may name any command
  /\ grantForm \in Forms                      \* any form may come with the grant
  /\ grantNames \in Named                     \* and may name any command
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

\* Whether the Node may act for the User at all, for a write that sends no
\* command. An update sends none, so a request-bound token has nothing to name.
MayAct(form) == holdsKey \/ (form = "delegation" /\ Good)

\* The same question answered from the facts, with no bug.
TrulyMayAct(form) == holdsKey \/ (form = "delegation" /\ TrulyGood)

\* Whether the write is admitted: a create needs authority for its
\* registration, an update authority to act for the User.
WriteAdmitted ==
  \/ Bug = "noAuthority"                                        \* bug: nothing is asked
  \/ kind = "create" /\ MaySend("register", writeForm, writeNames)
  \/ kind = "update" /\ MayAct(writeForm)

\* The same question answered from the facts, with no bug.
WriteTrulyAdmitted ==
  \/ kind = "create" /\ TrulyMaySend("register", writeForm, writeNames)
  \/ kind = "update" /\ TrulyMayAct(writeForm)

\* The embedder hands in the write. An admitted write puts its row and, for a
\* create, the owed registration into the transaction. A refused write records
\* nothing; the planted bug writes first and checks afterwards.
Stage ==
  /\ phase = "ready"                                            \* only once
  /\ IF WriteAdmitted
       THEN /\ phase' = "staged"                                \* the write is in
            /\ rows' = TRUE                                     \* its row is written
            /\ owed' = IF kind = "create" THEN {"register"} ELSE {} \* a create owes one command
       ELSE /\ phase' = "refused"                               \* the write is refused
            /\ rows' = (Bug = "recordedFirst")                  \* nothing is written
            /\ owed' = IF Bug = "recordedFirst" /\ kind = "create"
                         THEN {"register"} ELSE {}              \* nothing is owed
  /\ UNCHANGED <<kind, holdsKey, writeForm, writeNames, grantForm, grantNames,
                 bySigner, forNode, current, sent, parked, misused, stale, http>>
  /\ UNCHANGED after                                            \* nothing is recorded here

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

\* The Node's owner commits the transaction: what is owed is now due.
Commit ==
  /\ phase = "staged"                                           \* something was staged
  /\ phase' = "committed"                                       \* the rows are durable
  /\ UNCHANGED <<kind, holdsKey, writeForm, writeNames, grantForm, grantNames,
                 bySigner, forNode, current, rows, owed, sent, parked, misused,
                 stale, http>>
  /\ UNCHANGED after                                            \* nothing is recorded here

\* Time passes and the credentials expire. This can happen at any moment.
Expire ==
  /\ current                                                    \* not expired yet
  /\ current' = FALSE                                           \* now they are
  /\ UNCHANGED <<kind, holdsKey, writeForm, writeNames, grantForm, grantNames,
                 bySigner, forNode, phase, rows, owed, sent, parked, misused,
                 stale, http>>
  /\ UNCHANGED after                                            \* nothing is recorded here

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

\* A fresh credential expires too.
ExpireFresh(c) ==
  /\ freshLive[c]                                               \* not expired yet
  /\ freshLive' = [freshLive EXCEPT ![c] = FALSE]               \* now it is
  /\ UNCHANGED <<kind, holdsKey, writeForm, writeNames, grantForm, grantNames,
                 bySigner, forNode, current, phase, rows, owed, sent, parked,
                 misused, stale, http, fresh, failed, noted, unbacked>>

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

\* Any of the steps above may come next.
Next == Stage \/ StageGrant \/ Commit \/ Expire \/ SendRegistration \/ SendGrant
        \/ ParkRegistration \/ ParkGrant \/ Knock
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
=============================================================================
