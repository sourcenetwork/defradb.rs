---- MODULE ConnmgrTrimSilence ----
\* A node holds connections to a few peers. Its connection manager trims the
\* node back to the low-water mark when it has more than the high-water mark.
\* Peers can connect explicitly at any moment, and Kademlia, on the node or on
\* the peer, can dial a peer it has met before (its automatic bootstrap dials
\* every known peer). What must never happen: Kademlia redials a peer the
\* manager just pruned, undoing the trim, or the manager trims again before
\* its silence period is over, so a node at high water keeps cutting and
\* reconnecting (#3357).
\*
\* Code: crates/database/p2p/src/host/p2p_host/connection_manager.rs
\* ActiveConnectionManager::prune_candidates and silenced;
\* crates/database/p2p/src/behaviour/dual_kademlia.rs DefraKademlia::poll,
\* handle_pending_outbound_connection, on_swarm_event and refuse_held_dial;
\* crates/database/p2p/src/behaviour/redial_hold.rs RedialHold::hold, is_held.
\* Integers for times that start below zero; FiniteSets for set sizes.
EXTENDS Integers, FiniteSets
\* Peers: the other nodes. Low and High: the watermarks. Silence: the least
\* time between trims, also how long a pruned peer's redial is held.
\* MaxTime: how long the model runs. Mode: the code, or one planted bug.
CONSTANTS Peers, Low, High, Silence, MaxTime, Mode
\* "correct" is the code; "noSilence" trims whenever the node is over high
\* water; "noHold" lets Kademlia redial a peer the trim just pruned.
ASSUME Mode \in {"correct", "noSilence", "noHold"}
\* The watermarks are ordered and the silence period is positive.
ASSUME Low <= High /\ Silence > 0
\* now: the clock. connected: peers with an open connection. known: peers
\* Kademlia has met, so may dial. heldUntil: when each peer's redial hold ends.
\* lastTrim: when the last trim that closed something ran. badRedial: Kademlia
\* redialed a held peer. badTrim: a trim ran inside a silence period.
VARIABLES now, connected, known, heldUntil, lastTrim, badRedial, badTrim
\* All variables, for the "nothing else changes" part of the spec.
vars == <<now, connected, known, heldUntil, lastTrim, badRedial, badTrim>>
\* No peers yet, nothing held, and the last trim far enough in the past.
Init == /\ now = 0
        /\ connected = {}
        /\ known = {}
\* A hold that ended at time 0 holds nothing.
        /\ heldUntil = [p \in Peers |-> 0]
\* The manager has never trimmed: a trim may run at once.
        /\ lastTrim = -Silence
        /\ badRedial = FALSE
        /\ badTrim = FALSE
\* Time passes by one step, up to MaxTime.
Tick == /\ now < MaxTime
        /\ now' = now + 1
        /\ UNCHANGED <<connected, known, heldUntil, lastTrim, badRedial, badTrim>>
\* An explicit connect, from either end. Nobody holds it, so it may be any
\* peer, pruned or not; the node meets it.
DialIn(p) == /\ p \notin connected
             /\ connected' = connected \cup {p}
             /\ known' = known \cup {p}
             /\ UNCHANGED <<now, heldUntil, lastTrim, badRedial, badTrim>>
\* Is p's redial held right now (RedialHold::is_held)?
Held(p) == now < heldUntil[p]
\* Kademlia, on the node or on the peer, dials a known, disconnected peer.
\* DefraKademlia::refuse_held_dial refuses the dial while the peer is held,
\* once the swarm takes it (a dial already in progress is not a new one), on
\* both ends, since each end held it when the connection closed; the "noHold"
\* bug skips that check.
Redial(p) == /\ p \in known
             /\ p \notin connected
             /\ (Mode = "noHold" \/ ~Held(p))
             /\ connected' = connected \cup {p}
\* Remember if this dial undid a trim.
             /\ badRedial' = (badRedial \/ Held(p))
             /\ UNCHANGED <<now, known, heldUntil, lastTrim, badTrim>>
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
        /\ lastTrim' = now
\* Remember if this trim came too soon after the last one.
        /\ badTrim' = (badTrim \/ Silenced)
        /\ UNCHANGED <<now, known, badRedial>>
\* Any of the steps above.
Next == \/ Tick
        \/ Trim
        \/ \E p \in Peers : DialIn(p) \/ Redial(p)
\* Start in Init, then take Next steps or stand still.
Spec == Init /\ [][Next]_vars
\* Every variable stays one of its allowed values.
TypeOK == /\ now \in 0..MaxTime
          /\ connected \subseteq Peers /\ known \subseteq Peers
          /\ heldUntil \in [Peers -> 0..(MaxTime + Silence)]
          /\ lastTrim \in (-Silence)..MaxTime
          /\ badRedial \in BOOLEAN /\ badTrim \in BOOLEAN
\* Neither end's Kademlia redials a peer inside the silence period of the trim
\* that pruned it.
INV_NoRedialOfPruned == ~badRedial
\* Two trims are always at least the silence period apart.
INV_TrimsSpaced == ~badTrim
====
