---- MODULE MC_Nac_Red_StartReEnables ----
EXTENDS Nac
\* RED: starting the node with the enable option re-enables a stored
\* temporarily disabled NAC (BugStartReEnables): NacManager::initialize
\* calling re_enable() on DisabledTemporarily. The code already keeps a
\* disabled NAC off on start; this config guards against that coming back.
\* The bug it plants: the owner disables NAC, the node restarts with the
\* option, and NAC comes back on with nobody authorizing the re-enable.
\* Expected:
\*   INV_ReEnableNeedsPersistedAdmin   (re-enabled with no authorized actor)

\* The bootstrap owner.
mcOwner == "owner"
\* One legitimate admin.
mcAdmins0 == {"a1"}
\* One actor who is never legitimately an admin.
mcNonAdmins == {"adv"}
\* At most two admin-set changes, to keep the state space small.
mcMaxMutations == 2
====
