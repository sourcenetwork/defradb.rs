---- MODULE KeyBatchNotification ----
\* The KMS stores verified keys from one reply. A later put may block while
\* the request is cancelled. With a listener installed, the reply guard
\* must notify every earlier key.
\* Models RemoteKeys::fetch_batch and ArrivedKeys::drop in
\* crates/database/kms/src/remote.rs.
EXTENDS Naturals
\* Enable notification on cancellation; FALSE plants the old batch-loop bug.
CONSTANT NotifyOnCancel
\* Two keys suffice: the first is stored while the second put is pending.
Keys == {1, 2}
\* The stored keys, keys delivered to the listener, and reply lifecycle.
VARIABLES stored, notified, phase
\* Group state for temporal stuttering.
vars == <<stored, notified, phase>>
\* The state holds only modeled keys and a valid lifecycle phase.
TypeOK == /\ stored \subseteq Keys
          /\ notified \subseteq Keys
          /\ phase \in {"active", "done"}
\* A new reply has stored and notified nothing.
Init == /\ stored = {}
        /\ notified = {}
        /\ phase = "active"
\* A successful put adds one previously unstored key.
Store(k) == /\ phase = "active" /\ k \in Keys \ stored
            /\ stored' = stored \cup {k}
            /\ UNCHANGED <<notified, phase>>
\* Normal completion notifies all keys stored from the reply.
Finish == /\ phase = "active"
          /\ phase' = "done"
          /\ notified' = notified \cup stored
          /\ UNCHANGED stored
\* Cancellation must flush the stored prefix, even with a later put pending.
Cancel == /\ phase = "active"
          /\ phase' = "done"
          /\ notified' = IF NotifyOnCancel THEN notified \cup stored ELSE notified
          /\ UNCHANGED stored
\* A reply can store, finish or be cancelled.
Next == (\E k \in Keys : Store(k)) \/ Finish \/ Cancel
\* The reply starts empty and follows those transitions or stutters.
Spec == Init /\ [][Next]_vars
\* Once a reply ends, every key it stored has been delivered to the listener.
INV_StoredKeysNotified == phase = "done" => stored \subseteq notified
====
