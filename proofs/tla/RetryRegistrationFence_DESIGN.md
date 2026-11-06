# Retry registration fence

Marker tokens are allocated before a retried registration transaction. Allocation order therefore need not match commit order. The model separates these actions and permits forget/re-add between them.

A registration must observe both the captured peer incarnation and a marker no newer than its allocated token. The positive configuration checks both conditions; the two negative configurations remove one condition each and violate `ACT_RegistrationCannotCrossFence`, an action property over the marker each commit writes.

The model also includes the caller in `broadcast.rs` (`admit_replicator_push`). A committed head lists the replicators and then registers one peer at a time. Each registration commits or is rejected for one of these reasons:
- the peer is gone;
- the peer was re-incarnated (forget/re-add, or `create_replicator` on a present peer);
- a newer marker covers the head;
- a storage fault.

`INV_FanOutPeerLocal` requires two things once the caller is done. Every listed peer has its own decision, and the queued jobs are exactly the registered peers. `Red_FanOutAbort` returns on the first rejection, as the code did before. A delete during the fan-out, or a re-incarnation of a present peer, then leaves later peers undecided and earlier peers registered with no job. Faults are off in that configuration, so a delete alone produces the counterexample. `LIVE_FanOutDecides` takes weak fairness on the caller's step as a whole, not per peer.

The bounds are three peers, one other writer's registration, and two incarnations per peer. Whether a rejected peer's head is redelivered (a re-add replays it; a re-incarnation without a replay does not) is outside this model. Transport, payload discovery, retry scheduling, and the Rust implementation are also outside this boundary model. Already-admitted external I/O is modeled separately from authorization of a future push.
