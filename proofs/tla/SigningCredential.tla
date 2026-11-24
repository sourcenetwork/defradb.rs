---- MODULE SigningCredential ----
\* The edge verifies each socket's own credential. Identity tokens and
\* delegations share admission, and expiry stops new signing work. RED modes
\* restore the format-only admission bug or drop the expiration check.
EXTENDS Naturals
CONSTANT RejectDelegation, IgnoreExpiry
VARIABLES now, credential, opened, dispatched
vars == <<now, credential, opened, dispatched>>
Kinds == {"identity", "delegation"}
\* Each connection starts before its credential expires at second two.
Init == /\ now = 0 /\ credential = "none" /\ opened = FALSE /\ dispatched = {}
\* The same verified caller may use either credential format.
Connect(kind) == /\ credential = "none" /\ kind \in Kinds
                 /\ credential' = kind
                 /\ opened' = ~(RejectDelegation /\ kind = "delegation")
                 /\ UNCHANGED <<now, dispatched>>
\* Time may advance after a connection has opened.
Tick == /\ now < 3 /\ now' = now + 1
        /\ UNCHANGED <<credential, opened, dispatched>>
\* The socket records the time at which it starts a signing operation.
Dispatch == /\ opened /\ (IgnoreExpiry \/ now < 2)
            /\ dispatched' = dispatched \cup {now}
            /\ UNCHANGED <<now, credential, opened>>
Next == (\E kind \in Kinds: Connect(kind)) \/ Tick \/ Dispatch
Spec == Init /\ [][Next]_vars
\* A verified delegation opens a socket just as an identity token does.
INV_DelegationAdmitted == credential = "delegation" => opened
\* No operation starts at or after the credential's expiration.
INV_CurrentAtDispatch == \A time \in dispatched: time < 2
====
