------------------------ MODULE ChunkSettingsSync ------------------------
\* The story, in plain words.
\*
\* Who acts: a peer node that learns one `@chunk` collection from another
\* node's definition blocks, and a user on the peer who activates versions.
\* Version 1 is the collection's first version. Versions 2..N are patches,
\* each one built on the version just before it.
\*
\* What can happen: blocks arrive in any order. A patch waits until the
\* version it patches is stored. Each block carries `@chunk` settings, which
\* any peer can forge: the honest ones ("Good"), others ("Other"), or a
\* `maxChars` below the minimum ("TooSmall"). The settings here stand for
\* everything the code compares: the chunked fields with their `maxChars`,
\* `overlap` and splitter version, and the chunk parent. A block naming
\* another splitter version is "Other": the same text would be cut with
\* other rules. A first version on rules this node lacks is still "Good"; the
\* node refuses to cut its text, which this model does not need.
\* Each block also carries the collection's name: the one version 1 has
\* ("Article") or a new one ("Story"). Chunks find their parent by name, so
\* a rename of a `@chunk` collection counts as changed settings too.
\* A node may also hold a record from before the merge checked settings
\* (the "legacy" version), stored with other settings than version 1.
\*
\* What must never happen: a stored version with settings no schema could
\* declare, a merged patch whose settings differ from version 1, or an active
\* version whose settings differ from version 1. Chunks already cut with one
\* setting would then be read or rewritten with another. Nor may a merged or
\* active version carry another name than version 1: the chunks would lose
\* their parent, or a new collection with the old name would take them over.
\*
\* Rust anchors:
\* * `crates/db/src/merge/merge_handler/definition.rs`
\*   `process_collection_definition_delta`, with
\*   `crates/db/src/merge/merge_handler/definition_chunks.rs`
\*   `chunk_settings_error`: a first version follows
\*   `schema::chunk_settings_errors` (FirstCheck) and a patch must keep its
\*   previous version's settings, `schema::ensure_chunk_settings_kept`
\*   (MergeGuard). Both compare through `crates/schema/src/chunk_rules.rs`
\*   `chunk_settings_change`, whose `ChunkSettingsChange::Name` branch
\*   refuses a rename (NameCheck). A block that names a collection and has
\*   heads is a renaming patch, never a first version, so it meets the same
\*   guard. Modeled by `MergeFirst` and `MergePatch`.
\* * `crates/db/src/chunk/user_writes.rs` `ensure_keeps_first_chunk_settings`,
\*   called from `crates/db/src/collection/ops/version.rs`
\*   `set_active_collection_version_as` and
\*   `crates/db/src/txn/registry/collections.rs`
\*   `set_collection_active_in_txn_as` (ActivateGuard). Modeled by `Activate`.
\* * `crates/ffi/src/mobile_schema.rs` `plan_bootstrap` holds a mobile SDL to
\*   the stored first version with the same `ensure_chunk_settings_kept`.
EXTENDS Naturals

\* N: how many versions the collection has; the highest is a patch.
\* FirstCheck, MergeGuard, ActivateGuard: the three guards, TRUE in GREEN.
\* NameCheck: whether those guards also compare names, TRUE in GREEN.
\* HasLegacy: whether the node starts with a record stored before the fix.
CONSTANTS N, FirstCheck, MergeGuard, ActivateGuard, NameCheck, HasLegacy

\* The versions a peer can send: 1 is the first, the rest are patches.
Versions == 1..N
\* The legacy record gets the next number, so it is never a merged version.
Legacy == N + 1
\* Every version a node can hold, the legacy one included.
Held == 1..(N + 1)
\* The settings a block can carry.
Settings == {"Good", "Other", "TooSmall"}
\* The names a block can carry: the original one or a rename.
Names == {"Article", "Story"}
\* What a schema could declare: `maxChars` at least the minimum.
Valid(s) == s # "TooSmall"
\* No version is active yet.
NoVersion == 0

\* block: the settings each version's block carries, picked once by the sender.
\* name: the name each version's block carries, picked once by the sender.
\* stored: the settings of each version this node holds, or "none".
\* storedName: the name of each version this node holds, or "none".
\* rejected: the versions whose block this node refused.
\* active: the version the user activated, or NoVersion.
VARIABLES block, name, stored, storedName, rejected, active
\* All six variables together, for stuttering steps.
vars == <<block, name, stored, storedName, rejected, active>>

\* Whether held version `v` differs from held version `w` in what the guards
\* compare: the settings, and the name when NameCheck is on.
Differs(v, w) ==
    \* Other settings always count.
    \/ stored[v] # stored[w]
    \* Another name counts only with NameCheck. Its RED skips this.
    \/ (NameCheck /\ storedName[v] # storedName[w])

\* Nothing is stored and nothing is active, unless a legacy record exists.
Init ==
    \* The sender may forge any version's settings.
    /\ block \in [Versions -> Settings]
    \* The sender may also rename any version.
    /\ name \in [Versions -> Names]
    \* With a legacy record, version 1 was merged honestly before the fix,
    \* and the legacy patch was stored with other settings next to it.
    /\ IF HasLegacy
          THEN /\ Valid(block[1])
               /\ \E s \in Settings \ {block[1]} :
                    stored = [v \in Held |->
                                IF v = 1 THEN block[1]
                                ELSE IF v = Legacy THEN s ELSE "none"]
               \* The legacy record may also carry another name.
               /\ \E n \in Names :
                    storedName = [v \in Held |->
                                    IF v = 1 THEN name[1]
                                    ELSE IF v = Legacy THEN n ELSE "none"]
          \* Without one, the node starts empty.
          ELSE /\ stored = [v \in Held |-> "none"]
               /\ storedName = [v \in Held |-> "none"]
    \* No block has been refused yet.
    /\ rejected = {}
    \* No version is active yet.
    /\ active = NoVersion

\* The first version arrives. With FirstCheck, settings no schema could
\* declare are refused; otherwise it is stored as it came.
MergeFirst ==
    \* It has not been handled yet.
    /\ stored[1] = "none" /\ 1 \notin rejected
    \* The check the SDL also runs. The RED for FirstCheck skips it.
    /\ IF FirstCheck /\ ~Valid(block[1])
          \* Refused: quarantined, nothing stored.
          THEN /\ rejected' = rejected \cup {1}
               /\ UNCHANGED <<stored, storedName>>
          \* Accepted: the record holds the block's settings.
          ELSE /\ stored' = [stored EXCEPT ![1] = block[1]]
               /\ storedName' = [storedName EXCEPT ![1] = name[1]]
               /\ UNCHANGED rejected
    \* Nothing else changes.
    /\ UNCHANGED <<block, name, active>>

\* A patch arrives once the version it patches is stored. With MergeGuard,
\* settings other than that version's are refused, and with NameCheck so
\* is another name.
MergePatch(v) ==
    \* It is a patch that has not been handled yet.
    /\ v > 1 /\ stored[v] = "none" /\ v \notin rejected
    \* Its previous version is held; until then the merge is retried later.
    /\ stored[v - 1] # "none"
    \* The guard compares with the previous version. The RED skips it.
    /\ IF MergeGuard
          \* The same comparison as `Differs`, against the block not yet held.
          /\ (block[v] # stored[v - 1]
              \/ (NameCheck /\ name[v] # storedName[v - 1]))
          \* Refused: quarantined, nothing stored.
          THEN /\ rejected' = rejected \cup {v}
               /\ UNCHANGED <<stored, storedName>>
          \* Accepted: the record holds the block's settings and name.
          ELSE /\ stored' = [stored EXCEPT ![v] = block[v]]
               /\ storedName' = [storedName EXCEPT ![v] = name[v]]
               /\ UNCHANGED rejected
    \* Nothing else changes.
    /\ UNCHANGED <<block, name, active>>

\* The user activates a held version. With ActivateGuard, a version whose
\* settings, or with NameCheck whose name, differ from the collection's first
\* version is refused.
Activate(v) ==
    \* The version is held, as an activation needs its record.
    /\ stored[v] # "none"
    \* The guard compares with version 1. The RED skips it.
    /\ ActivateGuard => ~Differs(v, 1)
    \* That version becomes the active one.
    /\ active' = v
    \* Nothing else changes.
    /\ UNCHANGED <<block, name, stored, storedName, rejected>>

\* Any block may arrive next, or the user may activate any held version.
Next ==
    \/ MergeFirst
    \/ \E v \in Versions : MergePatch(v)
    \/ \E v \in Held : Activate(v)

\* The whole behaviour: start, then take steps or stutter.
Spec == Init /\ [][Next]_vars

\* Each variable keeps its shape.
TypeOK ==
    \* The sender's settings are fixed settings.
    /\ block \in [Versions -> Settings]
    \* The sender's names are fixed names.
    /\ name \in [Versions -> Names]
    \* A held version has settings; others hold "none".
    /\ stored \in [Held -> Settings \cup {"none"}]
    \* A held version has a name; others hold "none".
    /\ storedName \in [Held -> Names \cup {"none"}]
    \* Only the peer's versions can be refused.
    /\ rejected \subseteq Versions
    \* The active version is held, or there is none.
    /\ active \in Held \cup {NoVersion}

\* A first version this node stored has settings a schema could declare.
FirstValid == stored[1] # "none" => Valid(stored[1])

\* Every patch this node merged keeps the settings of the first version.
MergedMatchFirst ==
    \A v \in Versions : (v > 1 /\ stored[v] # "none") => stored[v] = stored[1]

\* The active version's settings are the collection's first version's.
ActiveMatchesFirst == active # NoVersion => stored[active] = stored[1]

\* Every patch this node merged keeps the first version's name, so its
\* chunks still find their parent.
MergedKeepName ==
    \A v \in Versions :
        (v > 1 /\ stored[v] # "none") => storedName[v] = storedName[1]

\* The active version has the first version's name.
ActiveKeepsName == active # NoVersion => storedName[active] = storedName[1]
=============================================================================
