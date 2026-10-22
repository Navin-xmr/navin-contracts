# Shipment Contract Call Argument Conventions

This document defines the canonical argument ordering expected by external
integrators and contract clients for `NavinShipment`.

## Ordering Rules

- Use the exact argument order from each `pub fn` in `src/lib.rs`.
- Treat ordering as part of the public interface contract.
- For authenticated entrypoints, signatures are bound to ordered argument tuples.
- For token interoperability, `transfer` is always invoked as
  `transfer(from, to, amount)`.

## Externally Consumed Contract Method Signatures

The external API surface is the `#[contractimpl]` block in `src/lib.rs`. The
current exported methods are:

### Metadata / Notes / Evidence

- `set_shipment_metadata(env, caller, shipment_id, metadata_hash)`
- `append_note_hash(env, caller, shipment_id, note_hash)`
- `add_dispute_evidence_hash(env, caller, shipment_id, evidence_hash)`
- `get_dispute_evidence_count(env, shipment_id)`
- `get_dispute_evidence_hash(env, shipment_id, index)`
- `get_integration_nonce(env, shipment_id)`
- `get_note_count(env, shipment_id)`
- `get_note_hash(env, shipment_id, index)`

### Initialization / Config / Limits

- `initialize(env, admin, token_contract)`
- `set_shipment_limit(env, admin, limit)`
- `get_shipment_limit(env)`
- `set_company_shipment_limit(env, admin, company, limit)`
- `get_effective_shipment_limit(env, company)`
- `get_active_shipment_count(env, company)`
- `get_admin(env)`
- `get_version(env)`
- `get_hash_algo_version(env)`
- `get_expected_token_decimals(env)`
- `get_contract_metadata(env)`
- `get_shipment_counter(env)`
- `get_analytics(env)`
- `get_config_checksum(env)`
- `compute_idempotency_key(env, caller, payload_hash, nonce)`
- `update_config(env, admin, config)`
- `get_contract_config(env)`

### Role / Whitelist / Access

- `add_carrier_to_whitelist(env, company, carrier)`
- `remove_carrier_from_whitelist(env, company, carrier)`
- `is_carrier_whitelisted(env, company, carrier)`
- `get_role(env, address)`
- `add_company(env, admin, company)`
- `add_carrier(env, admin, carrier)`
- `add_guardian(env, admin, guardian)`
- `add_operator(env, admin, operator)`
- `suspend_carrier(env, admin, carrier)`
- `reactivate_carrier(env, admin, carrier)`
- `is_carrier_suspended(env, carrier)`
- `revoke_role(env, admin, target)`
- `suspend_role(env, admin, target)`
- `reactivate_role(env, admin, target)`
- `suspend_company(env, admin, company)`
- `reactivate_company(env, admin, company)`

### Shipment Lifecycle / Escrow / Settlements

- `create_shipment(env, company, receiver, carrier, data_hash, payment_milestones, deadline)`
- `create_shipments_batch(env, company, shipments)`
- `get_shipment(env, shipment_id)`
- `get_shipment_creator(env, shipment_id)`
- `get_shipment_receiver(env, shipment_id)`
- `get_restore_diagnostics(env, shipment_id)`
- `deposit_escrow(env, sender, shipment_id, amount)`
- `update_status(env, caller, shipment_id, status, event_hash)`
- `get_escrow_balance(env, shipment_id)`
- `get_escrow_freeze_reason(env, shipment_id)`
- `get_settlement(env, settlement_id)`
- `get_active_settlement(env, shipment_id)`
- `get_settlement_count(env)`
- `get_shipment_count(env)`
- `search_shipments_by_status(env, status, start, limit)`
- `get_event_count(env, shipment_id)`
- `archive_shipment(env, admin, shipment_id)`
- `confirm_delivery(env, receiver, shipment_id, proof_hash)`
- `confirm_partial_delivery(env, receiver, shipment_id, amount, proof_hash)`
- `report_geofence_event(env, caller, shipment_id, geofence_hash, severity)`
- `update_eta(env, caller, shipment_id, eta, reason_hash)`
- `record_milestone(env, caller, shipment_id, milestone_id, milestone_hash)`
- `record_milestones_batch(env, caller, shipment_id, milestone_updates)`
- `extend_shipment_ttl(env, shipment_id)`
- `cancel_shipment(env, caller, shipment_id, reason_hash)`
- `force_cancel_shipment(env, admin, shipment_id, reason_hash)`
- `upgrade(env, admin, wasm_hash)`
- `dry_run_migration(env, target_version)`
- `release_escrow(env, caller, shipment_id)`
- `refund_escrow(env, caller, shipment_id)`
- `raise_dispute(env, caller, shipment_id, dispute_type, evidence_hash)`
- `resolve_dispute(env, admin, shipment_id, resolution, reason_hash)`
- `handoff_shipment(env, caller, shipment_id, new_carrier, handoff_hash)`
- `report_condition_breach(env, caller, shipment_id, breach_hash, severity)`
- `verify_delivery_proof(env, shipment_id, proof_hash)`
- `get_shipment_reference(env, shipment_id)`
- `check_deadline(env, shipment_id)`

### Governance / Multisig / Admin Transfer / Health

- `transfer_admin(env, admin, new_admin)`
- `accept_admin_transfer(env, new_admin)`
- `init_multisig(env, admin, signers, threshold)`
- `propose_action(env, proposer, action_type, params, expiry)`
- `approve_action(env, approver, proposal_id)`
- `execute_proposal(env, proposal_id)`
- `get_proposal(env, proposal_id)`
- `get_multisig_config(env)`
- `pause(env, admin)`
- `unpause(env, admin)`
- `is_paused(env)`
- `get_status_hash(env, status)`
- `verify_data_hash(env, data, expected_hash)`
- `check_contract_health(env)`
- `reset_circuit_breaker(env, admin)`
- `check_consistency_violations(env)`

## Regression Coverage

`src/test_signature_argument_ordering.rs` enforces:

- canonical auth payload tuple ordering for representative integrator calls;
- canonical token boundary ordering for `transfer(from, to, amount)` during
  deposit, release, and refund flows.
