import Lean

-- A caller announces registry access before checking whether shutdown has
-- started. The drain closes admission before reading the access count.
-- The numbers below are positions in the order of the atomic operations.
-- See begin_task_registry_access, drain_background_tasks, and
-- TaskRegistryAccess::drop in crates/database/p2p/src/sync/coordinator/mod.rs.

namespace CoordinatorRegistry

-- Every caller admitted before shutdown has already announced itself when
-- the drain reads the count.
theorem admitted_announces_before_drain (enter check close drain : Nat)
    (announced : enter < check) (admitted : check < close)
    (closedBeforeDrain : close < drain) : enter < drain := by
  -- Follow the operations in their order.
  omega

-- An admitted caller that has yet to leave keeps the count positive at the
-- drain's read. The other callers can contribute any number of entries.
theorem admitted_access_prevents_zero (otherAccesses : Nat) :
    otherAccesses + 1 ≠ 0 := by
  -- The active caller contributes one even when all other callers left.
  omega

-- If the drain sees active access, the exit that takes the count to zero
-- observes shutdown and can signal the event, even before the drain waits.
theorem last_exit_observes_shutdown (close drain exit flagRead : Nat)
    (closedBeforeDrain : close < drain) (stillActive : drain < exit)
    (readsAfterExit : exit < flagRead) : close < flagRead := by
  -- The last exit and its flag read both follow the drain's observation.
  omega

end CoordinatorRegistry
