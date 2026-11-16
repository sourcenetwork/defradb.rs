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
\* What must never happen:
\*   - a write is recorded although the Node could not act for the User;
\*   - a refused write leaves a row or an owed record behind;
\*   - a request-bound delegation is used for any command but the one it names;
\*   - a request-bound delegation logs a caller in at the HTTP door;
\*   - a command is sent under a credential that has expired;
\*   - a grant reaches the provider before its document's registration.
\*
\* The provider's own checks (vera.rs `authorize_delegation`) are not modelled:
\* this is what the Node does before it sends anything.
\*
\* The model comes before its code. Rust owners as the code stands:
\* crates/defra-node/src/lib.rs execute_verified_graph_in_txn;
\* crates/query/src/runner/turn.rs lent;
\* crates/vera/src/cosmos/dac.rs owed_authority;
\* crates/vera/src/vera_rs/bearer.rs caller_of_delegation (the HTTP door);
\* crates/vera/src/cosmos/dac/owed.rs token, lapsed;
\* crates/acp/src/outbox/staging.rs TransactionOutbox::stage;
\* crates/acp/src/external/settle.rs register_first.
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
  http         \* the HTTP door: "idle", "in" or "out"

\* Every variable, so a step can say which ones it leaves alone.
vars == <<kind, holdsKey, writeForm, writeNames, grantForm, grantNames, bySigner,
          forNode, current, phase, rows, owed, sent, parked, misused, stale, http>>

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
MaySend(c, form, names) ==
  \/ holdsKey                                                   \* the Node signs itself
  \/ form = "delegation" /\ Good                                \* a general delegation
  \/ form = "request" /\ Good /\ (names = c \/ Bug = "anyCommand") \* bound to c

\* The same question answered from the facts, with no bug.
TrulyMaySend(c, form, names) ==
  \/ holdsKey
  \/ form = "delegation" /\ TrulyGood
  \/ form = "request" /\ TrulyGood /\ names = c

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

\* The embedder grants access in the same transaction, on the document the
\* create made. The grant is owed only when the Node may send it; a refused
\* grant leaves the transaction as it was.
StageGrant ==
  /\ phase = "staged"                                           \* inside the transaction
  /\ kind = "create"                                            \* on the new document
  /\ "grant" \notin owed                                        \* at most once
  /\ MaySend("grant", grantForm, grantNames)                    \* authority for the grant
  /\ owed' = owed \cup {"grant"}                                \* the grant is owed
  /\ UNCHANGED <<kind, holdsKey, writeForm, writeNames, grantForm, grantNames,
                 bySigner, forNode, current, phase, rows, sent, parked, misused,
                 stale, http>>

\* The Node's owner commits the transaction: what is owed is now due.
Commit ==
  /\ phase = "staged"                                           \* something was staged
  /\ phase' = "committed"                                       \* the rows are durable
  /\ UNCHANGED <<kind, holdsKey, writeForm, writeNames, grantForm, grantNames,
                 bySigner, forNode, current, rows, owed, sent, parked, misused,
                 stale, http>>

\* Time passes and the credentials expire. This can happen at any moment.
Expire ==
  /\ current                                                    \* not expired yet
  /\ current' = FALSE                                           \* now they are
  /\ UNCHANGED <<kind, holdsKey, writeForm, writeNames, grantForm, grantNames,
                 bySigner, forNode, phase, rows, owed, sent, parked, misused,
                 stale, http>>

\* Whether sending c now would use a request-bound token for another command.
Misuse(c, form, names) == ~holdsKey /\ form = "request" /\ names # c
\* Whether sending c now would use a credential that has expired.
Stale(form) == ~holdsKey /\ form # "none" /\ ~current

\* After the commit the Node sends the registration, with the credential the
\* write came with, checked again. One it may no longer send stays owed.
SendRegistration ==
  /\ phase = "committed"                                        \* after the commit
  /\ "register" \in owed                                        \* still owed
  /\ (MaySend("register", writeForm, writeNames) \/ Bug = "sentExpired") \* checked again
  /\ owed' = owed \ {"register"}                                \* owed no more
  /\ sent' = sent \cup {"register"}                             \* the provider has it
  /\ misused' = (misused \/ Misuse("register", writeForm, writeNames)) \* remember a misuse
  /\ stale' = (stale \/ Stale(writeForm))                       \* remember a stale send
  /\ UNCHANGED <<kind, holdsKey, writeForm, writeNames, grantForm, grantNames,
                 bySigner, forNode, current, phase, rows, parked, http>>

\* After the registration the Node sends the grant, its credential checked
\* again. The planted bug sends it while the registration is still owed.
SendGrant ==
  /\ phase = "committed"                                        \* after the commit
  /\ "grant" \in owed                                           \* still owed
  /\ ("register" \notin owed \/ Bug = "grantFirst")             \* registration first
  /\ (MaySend("grant", grantForm, grantNames) \/ Bug = "sentExpired") \* checked again
  /\ owed' = owed \ {"grant"}                                   \* owed no more
  /\ sent' = sent \cup {"grant"}                                \* the provider has it
  /\ misused' = (misused \/ Misuse("grant", grantForm, grantNames)) \* remember a misuse
  /\ stale' = (stale \/ Stale(grantForm))                       \* remember a stale send
  /\ UNCHANGED <<kind, holdsKey, writeForm, writeNames, grantForm, grantNames,
                 bySigner, forNode, current, phase, rows, parked, http>>

\* A grant whose credential no longer lets the Node send it is parked: it
\* waits for a new credential and is not sent.
ParkGrant ==
  /\ phase = "committed"                                        \* after the commit
  /\ "grant" \in owed                                           \* still owed
  /\ "register" \notin owed                                     \* its turn has come
  /\ ~MaySend("grant", grantForm, grantNames)                   \* no usable credential
  /\ owed' = owed \ {"grant"}                                   \* out of the queue
  /\ parked' = parked \cup {"grant"}                            \* waiting for authority
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

\* Any of the steps above may come next.
Next == Stage \/ StageGrant \/ Commit \/ Expire \/ SendRegistration \/ SendGrant
        \/ ParkGrant \/ Knock

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
INV_RegistrationFirst == ("grant" \in sent) => ("register" \notin owed)

\* A grant is owed only when the Node could really send it when it was staged.
INV_GrantAuthority ==
  [][("grant" \notin owed /\ "grant" \in owed') =>
       TrulyMaySend("grant", grantForm, grantNames)]_vars
=============================================================================
