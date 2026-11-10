---- MODULE SigningAdmission ----
\* Three clients try to use two signing slots. A connection may close or its
\* bearer may expire; cleanup returns its slot. No more than two run at once,
\* and an expired connection eventually releases its slot under fair cleanup.
\* Rust owners: crates/http/src/client_signing.rs and router/state.rs.
EXTENDS Naturals, FiniteSets
\* RED configurations remove admission or cleanup.
CONSTANT CheckCapacity, Cleanup
\* Three sessions contend for two slots.
Sessions == {1, 2, 3}
Capacity == 2
\* Active sessions hold slots; expired sessions are awaiting cleanup.
VARIABLES active, expired
vars == <<active, expired>>
\* No session holds a slot initially.
Init == /\ active = {} /\ expired = {}
\* A fresh connection may enter only while capacity remains.
Open(s) == /\ s \notin active /\ s \notin expired
           /\ (CheckCapacity => Cardinality(active) < Capacity)
           /\ active' = active \cup {s} /\ UNCHANGED expired
\* Closing or expiry ends the connection and demands cleanup.
Expire(s) == /\ s \in active /\ s \notin expired
             /\ expired' = expired \cup {s} /\ UNCHANGED active
\* Cleanup releases exactly that connection's slot.
Reap(s) == /\ Cleanup /\ s \in active /\ s \in expired
           /\ active' = active \ {s} /\ UNCHANGED expired
\* Any client may open, expire, or be cleaned independently.
Next == \E s \in Sessions : Open(s) \/ Expire(s) \/ Reap(s)
\* Weak fairness says an enabled cleanup eventually runs.
Spec == Init /\ [][Next]_vars /\ (\A s \in Sessions : WF_vars(Reap(s)))
\* Both sets name only known sessions.
TypeOK == /\ active \subseteq Sessions /\ expired \subseteq Sessions
\* At most the configured number of sessions hold slots.
INV_Capacity == Cardinality(active) <= Capacity
\* Every expired active session eventually releases its slot.
LIVE_Reaped == \A s \in Sessions : (s \in active /\ s \in expired) ~> (s \notin active)
====
