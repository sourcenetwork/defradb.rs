---------------------- MODULE SelectedCarAuthority ----------------------
\* An explicit send commits exact peer/root permission before its hint.
\* Cache loss, offline time and subscription removal cannot revoke it.
\* An operator can revoke that scope; only a new explicit grant restores it.
\* Rust: peerstore/selected_car.rs; push_docs.rs; event_handler/car.rs.
EXTENDS Naturals
\* Negative controls remove one safety requirement at a time.
CONSTANT Bug
\* Each number stands for a distinct peer/root pair.
Scopes == {1, 2}
\* Durable permission is separate from the disposable cache and subscription.
VARIABLES durable, cache, sent, revoked, subscribed, waited
\* The state tuple lets cache-only actions leave all authority unchanged.
vars == <<durable, cache, sent, revoked, subscribed, waited>>
\* No selected scope starts authorized.
Init == /\ durable = {} /\ cache = {} /\ sent = {} /\ revoked = {}
        /\ subscribed = TRUE /\ waited = FALSE
\* A grant is a committed native transaction, independent of subscriptions.
Grant(s) == /\ durable' = IF Bug = "durability" THEN durable ELSE durable \cup {s}
            /\ cache' = IF Bug = "durability" THEN cache \cup {s} ELSE cache
            /\ revoked' = revoked \ {s}
            /\ UNCHANGED <<sent, subscribed, waited>>
\* A head hint cannot precede its committed serving authority.
Send(s) == /\ (Bug = "send" \/ s \in durable \/ (Bug = "durability" /\ s \in cache))
           /\ sent' = sent \cup {s}
           /\ UNCHANGED <<durable, cache, revoked, subscribed, waited>>
\* Restart or pressure may lose caches, never durable permission.
LoseCache == /\ cache' = {} /\ UNCHANGED <<durable, sent, revoked, subscribed, waited>>
\* A receiver may remain offline beyond any old cache recovery deadline.
Wait == /\ ~waited /\ waited' = TRUE
        /\ durable' = IF Bug = "expiry" THEN {} ELSE durable
        /\ UNCHANGED <<cache, sent, revoked, subscribed>>
\* Removing a collection subscription leaves explicit grants alone.
Unsubscribe == /\ subscribed /\ subscribed' = FALSE
               /\ durable' = IF Bug = "subscription" THEN {} ELSE durable
               /\ UNCHANGED <<cache, sent, revoked, waited>>
\* Revocation deletes only the named scope in native commit order.
Revoke(s) == /\ revoked' = revoked \cup {s}
             /\ durable' = IF Bug = "revoke" THEN durable ELSE durable \ {s}
             /\ cache' = cache \ {s}
             /\ UNCHANGED <<sent, subscribed, waited>>
\* No other peer/root may borrow a persisted authorization.
Allows(s) == IF Bug = "scope" THEN durable # {} ELSE s \in durable
\* An acknowledged selected root remains recoverable unless explicitly revoked.
INV_Recovery == \A s \in sent \ revoked : Allows(s)
\* A different scope cannot inherit a grant.
INV_Scope == \A s \in Scopes \ durable : ~Allows(s)
\* A completed revocation removes its serving authority.
INV_Revoked == \A s \in revoked : ~Allows(s)
\* Native transactions linearize overlapping explicit grants and revocations.
Next == (\E s \in Scopes : Grant(s) \/ Send(s) \/ Revoke(s)) \/ LoseCache \/ Wait \/ Unsubscribe
=============================================================================
