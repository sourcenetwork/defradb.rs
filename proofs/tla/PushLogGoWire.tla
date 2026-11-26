---- MODULE PushLogGoWire ----
\* The story, in plain words.
\*
\* A Rust node pushes documents to two replicators: a Go node and a Rust
\* node. A replicator added with an identity gives the sender a capability,
\* and the sender may attach it to the push of a document that identity owns.
\* The capability is a field only Rust knows.
\*
\* Each push first offers Rust's request protocol. A Rust receiver accepts it;
\* a Go receiver does not, so the push falls back to Go's request protocol.
\* The sender then writes one of two signed forms of the request: the
\* extended form with the capability, or the Go-compatible form without it.
\*
\* Go checks a signature over the fields it knows. A request with the
\* capability therefore fails Go's check, and Go drops it without answering:
\* the sender times out and retries forever, and the document never arrives.
\* A Rust receiver checks the fields it got, so it answers either form.
\*
\* What must never happen:
\*   - Go is sent a request carrying the capability;
\*   - a push is dropped unanswered;
\*   - a Rust receiver gets an owned document's push without its capability.
\*
\* The Rust code this models:
\*   - the two signed forms        crates/database/p2p/src/two_stream/push_offer.rs PushLogOffer::sign
\*   - picking the form            crates/database/p2p/src/two_stream/push_offer.rs PushLogOffer::for_protocol
\*   - negotiating the protocol    crates/database/p2p/src/two_stream/handler/pushlog.rs TwoStreamHandler::start_request
\*   - attaching the capability    crates/database/p2p/src/explicit_replay.rs ExplicitReplayCapabilityCache::attach
\*   - Go's signature check        Go defradb internal/db/p2p/message/message.go verifyMessage

EXTENDS FiniteSets

CONSTANTS
  Docs,      \* the documents the sender pushes
  Owned,     \* the documents whose push the capability is attached to
  Receivers, \* the replicators: "go" and "rust"
  Bug        \* "None", "AttachEverywhere" or "StripEverywhere"

\* The two request protocols a push can travel on.
Protocols == {"go", "rust"}

VARIABLES
  inflight,    \* requests written to a stream and not yet handled
  answered,    \* <<doc, receiver>> pairs whose push the receiver answered
  dropped,     \* <<doc, receiver>> pairs whose push the receiver dropped
  capArrived   \* <<doc, receiver>> pairs that arrived carrying the capability

vars == <<inflight, answered, dropped, capArrived>>

\* A request on the wire: which document, to whom, on which protocol, and
\* whether it carries the capability.
Requests == [doc : Docs, to : Receivers, protocol : Protocols, cap : BOOLEAN]

\* Every variable holds what it should.
TypeOK ==
  /\ inflight \subseteq Requests
  /\ answered \subseteq Docs \X Receivers
  /\ dropped \subseteq Docs \X Receivers
  /\ capArrived \subseteq Docs \X Receivers

Init ==
  /\ inflight = {}
  /\ answered = {}
  /\ dropped = {}
  /\ capArrived = {}

\* The protocol a push ends up on: Rust's when the receiver accepts it, else
\* Go's, as the fallback on an unsupported protocol does.
Negotiated(r) == IF r = "rust" THEN "rust" ELSE "go"

\* Whether the request written carries the capability. Only an owned
\* document's push has one to carry. The fixed sender carries it on Rust's
\* protocol only; the old sender carried it everywhere; the over-eager fix
\* would strip it everywhere.
CarriesCap(d, protocol) ==
  /\ d \in Owned
  /\ CASE Bug = "AttachEverywhere" -> TRUE
       [] Bug = "StripEverywhere" -> FALSE
       [] OTHER -> protocol = "rust"

\* Whether a push of this document to this receiver was already made.
Pushed(d, r) ==
  \/ \E m \in inflight : m.doc = d /\ m.to = r
  \/ <<d, r>> \in answered
  \/ <<d, r>> \in dropped

\* The sender pushes a document to a receiver it has not pushed it to: it
\* negotiates the protocol, then writes the form that protocol takes.
Send(d, r) ==
  /\ ~Pushed(d, r)
  /\ LET p == Negotiated(r)
     IN inflight' = inflight \cup
          {[doc |-> d, to |-> r, protocol |-> p, cap |-> CarriesCap(d, p)]}
  /\ UNCHANGED <<answered, dropped, capArrived>>

\* Whether the receiver's signature check passes: Go re-encodes only its own
\* fields, so a request with the capability fails; Rust checks what it got.
Verifies(m) == m.to = "rust" \/ ~m.cap

\* A receiver handles one request: it answers a request that verifies and
\* drops one that does not, and a Rust receiver keeps the capability it got.
Receive(m) ==
  /\ m \in inflight
  /\ inflight' = inflight \ {m}
  /\ IF Verifies(m)
       THEN /\ answered' = answered \cup {<<m.doc, m.to>>}
            /\ dropped' = dropped
       ELSE /\ dropped' = dropped \cup {<<m.doc, m.to>>}
            /\ answered' = answered
  /\ capArrived' = IF m.cap /\ Verifies(m)
                     THEN capArrived \cup {<<m.doc, m.to>>}
                     ELSE capArrived

Next ==
  \/ \E d \in Docs, r \in Receivers : Send(d, r)
  \/ \E m \in inflight : Receive(m)

Spec == Init /\ [][Next]_vars

\* Go is never sent a request carrying the capability.
INV_GoGetsOnlyGoFields ==
  \A m \in inflight : m.to = "go" => ~m.cap

\* No push is ever dropped unanswered.
INV_NoPushDropped == dropped = {}

\* A Rust receiver that answered an owned document's push got its capability.
INV_RustKeepsCapability ==
  \A d \in Owned : <<d, "rust">> \in answered => <<d, "rust">> \in capArrived

====
