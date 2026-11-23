# `defra.acp`: Access control

- **Parents:** ["defra"]

Access-control behaviour beyond what Go's integration suite checks.

# `defra.acp.nac_restart_enforced`: Stored NAC stays enforced after a restart without the flag

- **Parents:** ["defra.acp"]
- **Scenarios:** ["acp::nac::nac_restart_enforced::a_node_restarted_without_the_nac_flag_still_refuses_an_anonymous_caller"]

A store NAC was enabled on comes back with NAC enforced when the node restarts without `--node-acp-enable`: the owner still sees status `enabled`, an anonymous caller adding a document fails with `not authorized to perform operation. Permission: get-collection`, and the owner's add succeeds. A store NAC was never configured on serves no NAC checks: its status is `not configured` and anonymous callers are not refused (`go.acp.nac.lifecycle.start`).

# `defra.acp.vera_uncovered_takeover_recovery`: An uncovered takeover recovers without a new worker

- **Parents:** ["defra.acp"]
- **Scenarios:** ["external::recovery::tests::a_pass_owes_what_the_deployment_lost_registration_first_and_then_ends", "external::recovery::tests::a_pass_checks_the_records_past_its_first_page", "external::recovery::tests::a_page_not_recorded_is_read_again_and_a_recorded_one_is_not", "external::recovery::tests::a_new_term_starts_the_pass_again", "external::recovery::tests::what_the_outbox_settles_is_recorded_and_owed_again_when_the_deployment_loses_it", "vera_rs::provider::recovery_tests::an_uncovered_journal_recovers_through_the_outbox_under_the_same_worker"]

When a Vera deployment takeover may have voided confirmed submissions older than the journal keeps, the node closes its journal as recoverable and refuses every access decision until a recovery pass finishes. The node keeps a record of every registration, unregistration, grant and revocation its provider confirmed, written in the commit that settles it. The pass walks that record in bounded pages, checks each page against the deployment under the certified term, and owes through the outbox what the deployment lost: a missing registration first, then the grants on it, and a delete for a grant the deployment still holds after the node revoked it. A page is recorded as delivered only after the outbox holds it; a restart resumes after the last recorded page, a new certified term starts the pass again, and the pass ends once the last record has been handed over. The node serves no access decision before then, and a journal closed for any other reason stays closed.

What the pass cannot restore is reported in the `acp::audit` log, never dropped: an object the deployment holds for another owner, a registration the node removed that the deployment holds again, a relationship whose registration was never recorded, a relationship the deployment does not let the pass read, and a registration or grant the provider never answered and the deployment does not hold. A delete the provider never answered is owed when the deployment still holds the grant. Changes other actors made directly on the deployment, and what the provider confirmed before the node kept this record, are not in it and are not restored.
