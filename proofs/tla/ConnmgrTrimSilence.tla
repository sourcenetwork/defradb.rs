---- MODULE ConnmgrTrimSilence ----
\* A node holds connections to a few peers. Its connection manager trims the
\* node back to the low-water mark when it has more than the high-water mark.
\* A connection can also close for other reasons: the peer restarts, the
\* network blips, or someone runs `client p2p disconnect`. Peers can connect
\* explicitly at any moment (`client p2p connect`, a replicator push, a Bitswap
\* fetch), and Kademlia, on the node or on the peer, can dial a peer it has met
\* before (its automatic bootstrap dials every known peer). A dial takes time,
\* so a second dial can arrive while a first one is still in flight.
\*
\* What must never happen:
\* - Kademlia redials a peer whose last connection closed less than a silence
\*   period ago, undoing a trim (#3357);
\* - the manager trims again before its silence period is over, so a node at
\*   high water keeps cutting and reconnecting;
\* - an explicit connect is refused because of the hold, which is Kademlia's
\*   alone;
\* - a Kademlia dial that finds a dial already in flight to the peer is
\*   refused as held, which tells Kademlia's query the peer failed while the
\*   in-flight dial may still connect it.
\*
\* The hold table's size cap is not modelled: these few peers never fill it.
\* proofs/lean/RedialHold.lean proves what the cap keeps and drops.
\*
\* Code: crates/database/p2p/src/host/p2p_host/connection_manager.rs
\* ActiveConnectionManager::prune_candidates and silenced;
\* crates/database/p2p/src/behaviour/dual_kademlia.rs DefraKademlia::poll,
\* handle_pending_outbound_connection, on_swarm_event and refuse_held_dial;
\* crates/database/p2p/src/behaviour/redial_hold.rs RedialHold::hold, is_held.
\* Integers for times that start below zero; FiniteSets for set sizes.
EXTENDS Integers, FiniteSets
\* Peers: the other nodes. Low and High: the watermarks. Silence: the least
\* time between trims, also how long a disconnected peer's redial is held.
\* MaxTime: how long the model runs. Mode: the code, or one planted bug.
CONSTANTS Peers, Low, High, Silence, MaxTime, Mode
\* "correct" is the code; "noSilence" trims whenever the node is over high
\* water; "noHold" lets Kademlia redial a peer whose connection just closed;
\* "holdAll" holds every dial to a held peer, explicit ones too;
\* "refuseFirst" refuses a held Kademlia dial before the swarm's peer check;
\* "trimOnly" holds only a trim's peers, not those of any other disconnect.
ASSUME Mode \in {"correct", "noSilence", "noHold", "holdAll", "refuseFirst", "trimOnly"}
\* The watermarks are ordered and the silence period is positive.
ASSUME Low <= High /\ Silence > 0
\* now: the clock. connected: peers with an open connection. dialing: peers
\* an explicit dial is in flight to. known: peers Kademlia has met, so may
\* dial. heldUntil: when each peer's redial hold ends, the code's state.
\* closedAt: when each peer's last connection last closed, the truth the
\* hold must follow, kept apart from it so a hold that is never taken shows
\* up. lastTrim: when the last
\* trim that closed something ran. badRedial: Kademlia redialed a held peer.
\* badTrim: a trim ran inside a silence period. badRefused: an explicit dial
\* was refused. badDenied: a Kademlia dial that met an in-flight dial was
\* refused as held.
VARIABLES now, connected, dialing, known, heldUntil, closedAt, lastTrim,
          badRedial, badTrim, badRefused, badDenied
\* All variables, for the "nothing else changes" part of the spec.
vars == <<now, connected, dialing, known, heldUntil, closedAt, lastTrim,
          badRedial, badTrim, badRefused, badDenied>>
\* The four "something went wrong" flags, which most steps leave alone.
flags == <<badRedial, badTrim, badRefused, badDenied>>
\* No peers yet, nothing held, and the last trim far enough in the past.
Init == /\ now = 0
        /\ connected = {}
        /\ dialing = {}
        /\ known = {}
\* A hold that ended at time 0 holds nothing.
        /\ heldUntil = [p \in Peers |-> 0]
\* No connection ever closed: long enough ago that nothing is recent.
        /\ closedAt = [p \in Peers |-> -Silence]
\* The manager has never trimmed: a trim may run at once.
        /\ lastTrim = -Silence
        /\ badRedial = FALSE
        /\ badTrim = FALSE
        /\ badRefused = FALSE
        /\ badDenied = FALSE
\* Time passes by one step, up to MaxTime.
Tick == /\ now < MaxTime
        /\ now' = now + 1
        /\ UNCHANGED <<connected, dialing, known, heldUntil, closedAt, lastTrim, flags>>
\* Is p's redial held right now (RedialHold::is_held)?
Held(p) == now < heldUntil[p]
\* Did p's last connection close less than a silence period ago?
Recent(p) == now - closedAt[p] < Silence
\* An explicit dial to a peer with no connection and no dial in flight, from
\* either end: a connect, a replicator push or a Bitswap fetch. The code never
\* holds it (refuse_held_dial lets through every dial Kademlia did not ask
\* for), so it goes in flight. The "holdAll" bug refuses it while the peer is
\* held, and remembers that it did.
ExplicitDial(p) == /\ p \notin connected \cup dialing
                   /\ IF Mode = "holdAll" /\ Held(p)
                      THEN /\ badRefused' = TRUE
                           /\ UNCHANGED <<dialing>>
                      ELSE /\ dialing' = dialing \cup {p}
                           /\ UNCHANGED <<badRefused>>
                   /\ UNCHANGED <<now, connected, known, heldUntil, closedAt,
                                  lastTrim, badRedial, badTrim, badDenied>>
\* An in-flight explicit dial connects; the node meets the peer.
DialDone(p) == /\ p \in dialing
               /\ dialing' = dialing \ {p}
               /\ connected' = connected \cup {p}
               /\ known' = known \cup {p}
               /\ UNCHANGED <<now, heldUntil, closedAt, lastTrim, flags>>
\* Kademlia, on the node or on the peer, dials a known peer with no
\* connection while an explicit dial to it is in flight. The swarm's peer
\* condition answers it `DialPeerConditionFalse` before any behaviour sees
\* it, so the query that wanted the peer keeps waiting (DefraKademlia::poll
\* remembers the dial; refuse_held_dial never runs for it). The "refuseFirst"
\* bug refuses it as held before that check, so the query counts the peer
\* failed although the in-flight dial may still connect it.
KadDialInFlight(p) == /\ p \in known
                      /\ p \in dialing
                      /\ badDenied' = (badDenied \/ (Mode = "refuseFirst" /\ Held(p)))
                      /\ UNCHANGED <<now, connected, dialing, known, heldUntil,
                                     closedAt, lastTrim, badRedial, badTrim, badRefused>>
\* Kademlia, on the node or on the peer, dials a known peer with no
\* connection and no dial in flight. DefraKademlia::refuse_held_dial refuses
\* it while the peer is held, on both ends, since each end held it when the
\* connection closed; the "noHold" bug skips that check.
KadDial(p) == /\ p \in known
              /\ p \notin connected \cup dialing
              /\ (Mode = "noHold" \/ ~Held(p))
              /\ connected' = connected \cup {p}
\* Remember if this dial undid a recent disconnect.
              /\ badRedial' = (badRedial \/ Recent(p))
              /\ UNCHANGED <<now, dialing, known, heldUntil, closedAt, lastTrim,
                             badTrim, badRefused, badDenied>>
\* A connection closes for a reason other than a trim: a restart, a network
\* blip, `client p2p disconnect`. Closing a peer's last connection holds
\* Kademlia's redials of it on both ends until now + Silence
\* (DefraKademlia::on_swarm_event): the far end of a trim sees exactly this,
\* a remote close, and cannot tell it from a trim. The "trimOnly" bug takes
\* no hold here.
Disconnect(p) == /\ p \in connected
                 /\ connected' = connected \ {p}
                 /\ heldUntil' = IF Mode = "trimOnly" THEN heldUntil
                                  ELSE [heldUntil EXCEPT ![p] = now + Silence]
\* The connection closed now, whatever the code holds.
                 /\ closedAt' = [closedAt EXCEPT ![p] = now]
                 /\ UNCHANGED <<now, dialing, known, lastTrim, flags>>
\* Is the manager inside its silence period (ActiveConnectionManager::silenced)?
Silenced == now - lastTrim < Silence
\* A trim: only over high water and, in the code, only outside the silence
\* period. It closes any set of peers that brings the node to low water (which
\* peers is not what these invariants are about, so every choice is checked),
\* which holds their redials for Silence, and starts a new silence period.
Trim == /\ Cardinality(connected) > High
        /\ (Mode = "noSilence" \/ ~Silenced)
        /\ \E pruned \in SUBSET connected :
              /\ Cardinality(pruned) = Cardinality(connected) - Low
              /\ connected' = connected \ pruned
\* Closing a peer's last connection holds Kademlia's redials of it, on both
\* ends, until now + Silence (DefraKademlia::on_swarm_event).
              /\ heldUntil' = [p \in Peers |->
                                 IF p \in pruned THEN now + Silence ELSE heldUntil[p]]
\* The pruned peers' connections closed now.
              /\ closedAt' = [p \in Peers |->
                                IF p \in pruned THEN now ELSE closedAt[p]]
        /\ lastTrim' = now
\* Remember if this trim came too soon after the last one.
        /\ badTrim' = (badTrim \/ Silenced)
        /\ UNCHANGED <<now, dialing, known, badRedial, badRefused, badDenied>>
\* Any of the steps above.
Next == \/ Tick
        \/ Trim
        \/ \E p \in Peers : \/ ExplicitDial(p)
                            \/ DialDone(p)
                            \/ KadDialInFlight(p)
                            \/ KadDial(p)
                            \/ Disconnect(p)
\* Start in Init, then take Next steps or stand still.
Spec == Init /\ [][Next]_vars
\* Every variable stays one of its allowed values.
TypeOK == /\ now \in 0..MaxTime
          /\ connected \subseteq Peers /\ dialing \subseteq Peers
          /\ connected \cap dialing = {}
          /\ known \subseteq Peers
          /\ heldUntil \in [Peers -> 0..(MaxTime + Silence)]
          /\ closedAt \in [Peers -> (-Silence)..MaxTime]
          /\ lastTrim \in (-Silence)..MaxTime
          /\ badRedial \in BOOLEAN /\ badTrim \in BOOLEAN
          /\ badRefused \in BOOLEAN /\ badDenied \in BOOLEAN
\* Neither end's Kademlia redials a peer inside the silence period after its
\* last connection closed, by a trim or otherwise.
INV_NoRedialOfPruned == ~badRedial
\* Two trims are always at least the silence period apart.
INV_TrimsSpaced == ~badTrim
\* An explicit connect is never refused because of the hold.
INV_ExplicitNeverRefused == ~badRefused
\* A Kademlia dial that meets a dial already in flight is never refused as
\* held, so the query waiting on that peer keeps waiting for it.
INV_InFlightDialNotDenied == ~badDenied
====
