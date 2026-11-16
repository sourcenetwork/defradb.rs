---- MODULE MC_Nac_Red_EnableReEnables ----
EXTENDS Nac
\* RED: enable() does not refuse a temporarily disabled NAC
\* (BugEnableReEnables). The bug it plants: while NAC is paused, any caller
\* calls enable() naming itself owner -- startup with the enable option, or
\* POST /api/v0/acp/node/enable -- and NAC switches back on with that caller
\* added as an owner. Nobody held the stored re-enable permission.
\* Two configs share this module, because TLC stops at the first broken
\* invariant:
\*   MC_Nac_Red_EnableReEnables.cfg  fails INV_ReEnableNeedsPersistedAdmin
\*                                   (a non-admin turned NAC back on)
\*   MC_Nac_Red_EnableAddsOwner.cfg  fails INV_NoNonAdminMutatesAdminSet
\*                                   (a non-admin made itself an owner)

\* The bootstrap owner.
mcOwner == "owner"
\* One legitimate admin.
mcAdmins0 == {"a1"}
\* One actor who is never legitimately an admin.
mcNonAdmins == {"adv"}
\* At most two admin-set changes, to keep the state space small.
mcMaxMutations == 2
====
