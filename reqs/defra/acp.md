# `defra.acp`: Access control

- **Parents:** ["defra"]

Access-control behaviour beyond what Go's integration suite checks.

# `defra.acp.nac_restart_enforced`: Stored NAC stays enforced after a restart without the flag

- **Parents:** ["defra.acp"]
- **Scenarios:** ["acp::nac::nac_restart_enforced::a_node_restarted_without_the_nac_flag_still_refuses_an_anonymous_caller"]

A store NAC was enabled on comes back with NAC enforced when the node restarts without `--node-acp-enable`: the owner still sees status `enabled`, an anonymous caller adding a document fails with `not authorized to perform operation. Permission: get-collection`, and the owner's add succeeds. A store NAC was never configured on serves no NAC checks: its status is `not configured` and anonymous callers are not refused (`go.acp.nac.lifecycle.start`).

# `defra.acp.vera_takeover_report`: Vera takeover outcomes are published

- **Parents:** ["defra.acp"]
- **Scenarios:** ["term_takeover::a_provider_follows_a_confirmed_takeover_and_sends_again_what_it_voided"]

When a Vera provider follows a takeover that voided submissions it had confirmed, the call that checks the new term publishes one `acp-takeover-reconciled` event on the node's bus, after every voided submission was sent again. The event names the checked term and the cut of that term's certificate, and partitions the submissions into those applied again, those owed again and still pending, and those that could not be restored. Each entry names the DID its request acted as; a policy creation is named by its derived policy ID and its creator. Every unrestorable item carries the reason. A policy creation refused because the policy is there is applied again, not unrestorable. A later call that only delivers what the term left owed publishes nothing more. A check that fails closed publishes the event with `closed` set to the reason and what it had learnt until then, also when it sent nothing again; a node that fails closed later, because it cannot owe what the term left, publishes one more event for that term with `closed` set and the changes still owed. An event says `closed` only once the node has recorded the close: when that record cannot be written, the call that checked the term publishes the event of a checked term, and the next call closes again and publishes the closed event then. `closed` is null in the event of a term that was checked. A C API subscriber and the node's HTTP event stream receive the event as `acp-takeover-reconciled` with `term`, `cut_height`, `cut_block`, `closed`, `applied_again`, `owed_again` and `unrestorable`.
