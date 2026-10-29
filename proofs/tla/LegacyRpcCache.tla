------------------------- MODULE LegacyRpcCache -------------------------
\* A peer can have a modern path and a legacy fallback. Repeated calls reuse
\* the fallback; publishing it cannot replace a modern path. The two slots
\* correspond to endpoint_rpc's connections and legacy_connections maps.
EXTENDS Naturals
\* Negative controls remove legacy caching or modern selection precedence.
CONSTANTS CacheLegacy, PreferModern
\* Handles are 1 (legacy) and 2 (modern); 0 is an empty slot.
VARIABLES modern, legacy, calls, dials
\* These are all observations of this protocol-selection model.
vars == <<modern, legacy, calls, dials>>
\* No connection is selected initially.
Init == /\ modern = 0
        \* The legacy fallback also starts empty.
        /\ legacy = 0
        \* No caller has requested a legacy connection yet.
        /\ calls = 0
        \* No network dial has been needed yet.
        /\ dials = 0
\* A modern connection can arrive before or after either legacy request.
PublishModern == /\ modern = 0
                 \* The authenticated modern path occupies only its own slot.
                 /\ modern' = 2
                 \* Publication cannot change legacy state or dial counts.
                 /\ UNCHANGED <<legacy, calls, dials>>
\* Exercise two outbound calls; a selected connection should make another dial unnecessary.
Call == /\ calls < 2
        \* Count one completed caller operation.
        /\ calls' = calls + 1
        \* Only an empty pair of slots needs a network connection.
        /\ dials' = IF modern = 0 /\ legacy = 0 THEN dials + 1 ELSE dials
        \* A newly dialed legacy connection must be retained for the next call.
        /\ legacy' = IF CacheLegacy /\ modern = 0 THEN 1 ELSE legacy
        \* Dialing the legacy peer never overwrites the modern slot.
        /\ UNCHANGED modern
\* A legacy connection may also be published while a modern one is selected.
PublishLegacy == /\ legacy = 0
                 \* Each protocol has its own slot.
                 /\ legacy' = 1
                 \* No caller or network dial is involved in publication.
                 /\ UNCHANGED <<modern, calls, dials>>
\* The logical shared path prefers a modern handle whenever one exists.
Selected == IF PreferModern /\ modern # 0 THEN modern
            ELSE IF legacy # 0 THEN legacy ELSE modern
\* Two outbound calls to a legacy-only peer dial only once.
LegacyReused == dials <= 1
\* A legacy publication cannot displace a selected modern connection.
ModernPreferred == modern = 2 => Selected = 2
\* Publication and calls can occur in any order.
Next == PublishModern \/ PublishLegacy \/ Call
\* Idle periods cannot invent a connection.
Spec == Init /\ [][Next]_vars
=========================================================================
