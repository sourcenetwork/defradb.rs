-------------------------- MODULE GenesisReadAccess --------------------------
\* The story. A remote author signs the genesis of a document and replicates it
\* to this node. The host may have asked, for the document's collection, that
\* such a genesis also grants read access: to everyone (a public discovery
\* collection), or to this node when the genesis names this node in an
\* immutable field (an addressed instruction). The first time this node
\* registers the document it stores the author as owner and, when the host
\* asked for it, the initial reader. Afterwards the owner may revoke that
\* reader, and the same genesis may arrive again at any time.
\*
\* What must never happen: an unsigned genesis grants anything; a grant goes
\* to a node the genesis did not address; a collection the host did not opt in
\* grants anything; a genesis that arrives again brings back a reader the
\* owner revoked.
\*
\* The code: genesis_reader and register_signed_genesis in
\* crates/database/db/src/merge/merge_handler/genesis_access.rs, called from
\* process_linked_field_blocks (composite_fields.rs) and the registration in
\* composite_persist.rs.
\* Only the standard operators on numbers are used.
EXTENDS Naturals

\* Bug names the one guard a RED configuration breaks; "none" breaks nothing.
CONSTANT Bug

\* signed: the genesis carries a valid signature of its author.
\* address: whom the genesis names in its addressed field.
\* config: what the host asked for this collection.
\* registered: this node holds an owner for the document.
\* reader: this node holds the initial reader relationship.
\* revoked: the owner has revoked that reader at least once.
\* arrivals: how often the genesis arrived, bounded to keep the model small.
VARIABLES signed, address, config, registered, reader, revoked, arrivals

\* Every variable, for stuttering steps.
vars == <<signed, address, config, registered, reader, revoked, arrivals>>

\* Any genesis, any address, any host configuration; nothing is stored yet.
Init == /\ signed \in BOOLEAN
        /\ address \in {"this", "other", "absent"}
        /\ config \in {"public", "addressed", "off"}
        /\ registered = FALSE /\ reader = FALSE /\ revoked = FALSE
        /\ arrivals = 0

\* The genesis names this node in the field the host chose.
\* The "address" bug grants to any address, the "config" bug ignores an
\* opted-out collection that names this node.
Addressed == \/ address = "this"
             \/ Bug = "address"

\* What the genesis asks for, before the first-registration guard:
\* a valid signature (the "unsigned" bug drops it), and the host's choice.
Grantable == /\ (signed \/ Bug = "unsigned")
             /\ \/ config = "public"
                \/ config = "addressed" /\ Addressed
                \/ Bug = "config" /\ config = "off" /\ address = "this"

\* The guard: grant only while the document is not registered yet, read
\* before registering. The "replay" bug grants on every arrival.
First == ~registered \/ Bug = "replay"

\* The genesis arrives (again). A signed genesis registers its author; the
\* initial reader is granted only by the first registration.
Arrive == /\ arrivals < 3
          /\ arrivals' = arrivals + 1
          /\ registered' = (registered \/ signed)
          /\ reader' = (reader \/ (First /\ Grantable))
          /\ UNCHANGED <<signed, address, config, revoked>>

\* The owner revokes the initial reader.
Revoke == /\ reader
          /\ reader' = FALSE /\ revoked' = TRUE
          /\ UNCHANGED <<signed, address, config, registered, arrivals>>

\* Arrivals and revocations interleave in any order.
Next == Arrive \/ Revoke

\* Steps may also wait.
Spec == Init /\ [][Next]_vars

\* Only a validly signed genesis grants read access.
INV_SignedOnly == reader => signed

\* A grant needs the host's opt-in, and an addressed grant names this node.
INV_OptedIn == reader => (config = "public" \/ (config = "addressed" /\ address = "this"))

\* A reader the owner revoked never comes back by a genesis arriving again.
INV_RevocationStands == revoked => ~reader
=============================================================================
