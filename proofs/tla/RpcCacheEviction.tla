------------------------ MODULE RpcCacheEviction ------------------------
\* Stale stream cleanup races disconnect after a replacement was cached.
EXTENDS Naturals
\* The negative control restores the remove-and-reinsert implementation.
CONSTANT AtomicEviction
\* Handle two is the replacement; zero is an empty cache slot.
VARIABLES cached, held, phase, disconnected, open
\* The complete state includes any replacement temporarily held by cleanup.
vars == <<cached, held, phase, disconnected, open>>
\* A replacement is already selected when old stream cleanup arrives.
Init == /\ cached = 2
        \* Cleanup has not taken a handle out of the map.
        /\ held = 0
        \* Cleanup has not started.
        /\ phase = "start"
        \* Disconnect has not run.
        /\ disconnected = FALSE
        \* The replacement transport is open.
        /\ open = TRUE
\* Cleanup belongs to old handle one, never to replacement handle two.
Evict == /\ phase = "start"
         \* Conditional removal leaves a mismatched replacement in the map.
         /\ cached' = IF AtomicEviction THEN cached ELSE 0
         \* The broken implementation hides that replacement until restoration.
         /\ held' = IF AtomicEviction THEN 0 ELSE cached
         \* Expose the interval in which disconnect could miss it.
         /\ phase' = "removed"
         \* Eviction itself neither disconnects nor closes a transport.
         /\ UNCHANGED <<disconnected, open>>
\* The broken implementation reinserts a mismatched handle after removal.
Restore == /\ phase = "removed"
           \* Only a handle removed by the broken cleanup can be restored.
           /\ cached' = IF held # 0 THEN held ELSE cached
           \* Cleanup no longer holds the handle privately.
           /\ held' = 0
           \* Cleanup is complete.
           /\ phase' = "done"
           \* Restoration cannot undo a disconnect or close a transport.
           /\ UNCHANGED <<disconnected, open>>
\* Disconnect can run before, during or after stale cleanup.
Disconnect == /\ ~disconnected
              \* Record the completed disconnect.
              /\ disconnected' = TRUE
              \* Only a visible cached handle can be closed.
              /\ open' = IF cached = 2 THEN FALSE ELSE open
              \* Disconnect removes the visible cache entry.
              /\ cached' = 0
              \* It cannot find a handle privately held by stale cleanup.
              /\ UNCHANGED <<held, phase>>
\* Enumerate all interleavings of cleanup and disconnect.
Next == Evict \/ Restore \/ Disconnect
\* Idle steps do not change cache ownership.
Spec == Init /\ [][Next]_vars
\* A completed disconnect leaves no open replacement, including hidden ones.
DisconnectClosesReplacement == disconnected => ~open
=========================================================================
