# `defra.acp`: Access control

- **Parents:** ["defra"]

Access-control behaviour beyond what Go's integration suite checks.

# `defra.acp.nac_restart_enforced`: Stored NAC stays enforced after a restart without the flag

- **Parents:** ["defra.acp"]
- **Scenarios:** ["acp::nac::nac_restart_enforced::a_node_restarted_without_the_nac_flag_still_refuses_an_anonymous_caller"]

A store NAC was enabled on comes back with NAC enforced when the node restarts without `--node-acp-enable`: the owner still sees status `enabled`, an anonymous caller adding a document fails with `not authorized to perform operation. Permission: get-collection`, and the owner's add succeeds. A store NAC was never configured on serves no NAC checks: its status is `not configured` and anonymous callers are not refused (`go.acp.nac.lifecycle.start`).

# `defra.acp.vera_uncovered_takeover_recovery`: An uncovered takeover recovers without a new worker

- **Parents:** ["defra.acp"]

When a Vera deployment takeover may have voided confirmed submissions older than the journal keeps, the node marks the journal recoverable and fails closed until a recovery pass finishes. The pass walks this node's own committed registration and relationship state in bounded pages, checks each page against the deployment under the certified term, and owes missing registrations and grants through the outbox. A page is recorded as delivered only after the outbox holds it; the pass resumes from that page after a restart, and a new certified term starts a new pass. The node serves no access decision before the whole pass has been handed over. What the source cannot prove or restore is reported, never dropped.
