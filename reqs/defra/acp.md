# `defra.acp`: Access control

- **Parents:** ["defra"]

Access-control behaviour beyond what Go's integration suite checks.

# `defra.acp.nac_restart_enforced`: Stored NAC stays enforced after a restart without the flag

- **Parents:** ["defra.acp"]
- **Scenarios:** ["acp::nac::nac_restart_enforced::a_node_restarted_without_the_nac_flag_still_refuses_an_anonymous_caller"]

A store NAC was enabled on comes back with NAC enforced when the node restarts without `--node-acp-enable`: the owner still sees status `enabled`, an anonymous caller adding a document fails with `not authorized to perform operation. Permission: get-collection`, and the owner's add succeeds. A store NAC was never configured on serves no NAC checks: its status is `not configured` and anonymous callers are not refused (`go.acp.nac.lifecycle.start`).
