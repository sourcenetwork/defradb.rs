------------------------- MODULE LocalGenesisOwnership -------------------------
\* The story, in plain words.
\*
\* A node can obtain a document in three ways. It can own the create, it can
\* receive the document's signed genesis from a peer while its ACP provider is
\* the node-local engine, or it can receive that genesis into a provider shared
\* by several nodes. Local ACP is node-local: receiving a genesis from a peer
\* never creates a Local ACP owner on this node. An owned local create registers
\* its creator in the create's transaction, and a shared provider records the
\* receiver's registration with the genesis. The shared record belongs to the
\* shared-provider path, not to the generic local merge.
\*
\* What must never happen:
\*   - a peer's genesis registers a Local ACP owner on this node;
\*   - an owned local create becomes visible before its creator is registered;
\*   - a shared-provider genesis becomes visible without its registration.
\*
\* Rust owners: crates/database/db/src/write/autocommit/helpers.rs
\* register_created_doc; crates/database/db/src/merge/merge_handler/
\* composite_persist.rs persist_merged_document and record_replica.
EXTENDS Naturals

\* Bug names the one guard a RED configuration breaks; "none" breaks nothing.
CONSTANT Bug

\* How this node obtained the document.
Origins == {"local_create", "local_incoming", "shared_incoming"}

VARIABLES
  origin,    \* how this node obtained the document
  registered, \* this node's ACP path holds the document owner
  visible,   \* the merged document or create result is observable
  public     \* Local ACP treats an unregistered document as public

vars == <<origin, registered, visible, public>>

\* Nothing is observed before the acquisition completes.
Init ==
  /\ origin \in Origins
  /\ registered = FALSE
  /\ visible = FALSE
  /\ public = FALSE

\* The ownership each acquisition path records when its completion is durable.
Registers ==
  CASE origin = "local_create" -> TRUE
    [] origin = "local_incoming" -> Bug = "incoming_local_registers"
    [] origin = "shared_incoming" -> Bug # "shared_incoming_unregistered"

\* Complete the acquisition. The local create registers before visibility; an
\* incoming Local ACP genesis stays public and unregistered; a shared-provider
\* genesis is visible only with its registration.
Complete ==
  /\ ~visible
  /\ visible' = TRUE
  /\ registered' =
       CASE Bug = "local_create_before_register" /\ origin = "local_create" -> FALSE
         [] OTHER -> Registers
  /\ public' = (origin = "local_incoming")
  /\ UNCHANGED origin

\* The acquisition completes once and then remains settled.
Next == Complete \/ (visible /\ UNCHANGED vars)

Spec == Init /\ [][Next]_vars

TypeOK ==
  /\ origin \in Origins
  /\ registered \in BOOLEAN
  /\ visible \in BOOLEAN
  /\ public \in BOOLEAN

\* A peer's genesis never registers a Local ACP owner on the receiver.
INV_LocalIncomingUnregistered ==
  (origin = "local_incoming" /\ visible) => ~registered

\* An owned local create is never observable before its creator is registered.
INV_LocalCreateAtomic ==
  (origin = "local_create" /\ visible) => registered

\* A shared-provider genesis is never observable without its registration.
INV_SharedIncomingRegistered ==
  (origin = "shared_incoming" /\ visible) => registered

\* Only the incoming Local ACP path is public because it has no local owner.
INV_PublicOnlyIncomingLocal ==
  public => origin = "local_incoming"
===============================================================================
