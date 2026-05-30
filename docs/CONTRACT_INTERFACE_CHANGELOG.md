# Contract Interface Changelog

This changelog tracks the evolution of the public contract interface, including additions, deprecations, and behavior changes for public APIs.

## Entry Template

---
**Release:** vX.Y.Z (YYYY-MM-DD)
**Summary:** _Short summary of the change_

### Added
- _List new public functions or endpoints added_

### Changed
- _List changes in behavior or signature of existing public functions_

### Deprecated
- _List public functions or endpoints deprecated in this release_

### Removed
- _List public functions or endpoints removed in this release_

### Notes
- _Any additional notes or migration instructions_
---

## Interface History

---
**Release:** v0.1.0 (2026-04-29)
**Summary:** Initial changelog creation and backfill of recent public API changes for shipment and token contracts.

### Added
- `shipment` contract: `initialize`, `set_shipment_metadata`, `append_note_hash`, `add_dispute_evidence_hash`, `get_dispute_evidence_count`, `get_dispute_evidence_hash`, `get_integration_nonce`, `get_note_count`, `get_note_hash`, `set_shipment_limit`, `get_shipment_limit`, `set_company_shipment_limit`, `get_effective_shipment_limit`, `get_active_shipment_count`, `get_admin`, `get_version`, `get_hash_algo_version`, `get_expected_token_decimals`, `get_contract_metadata`, `get_shipment_counter`, `get_analytics`, `get_status_summary`, `get_non_terminal_count`, `get_config_checksum`, `compute_idempotency_key`, `add_carrier_to_whitelist`, `remove_carrier_from_whitelist`, `is_carrier_whitelisted`, `get_role`, `add_company`, `add_carrier`, `add_guardian`, `add_operator`, `suspend_carrier`, `reactivate_carrier`, `is_carrier_suspended`, `revoke_role`, `suspend_role`, `reactivate_role`, `suspend_company`, `reactivate_company`, `create_shipment`, `create_shipments_batch`, `get_shipment`, `get_shipment_creator`, `get_shipment_receiver`, `get_shipment_created_at`, `get_shipment_sender`, `get_shipment_updated_at`, `get_shipment_carrier`, `get_restore_diagnostics` (see source for full list).
- `token` contract: `initialize`, `mint`, `burn`, `transfer`, `balance_of`, `total_supply`, `set_admin`, `get_admin`, `set_name`, `get_name`, `set_symbol`, `get_symbol` (see source for full list).

### Changed
- _No changes recorded yet._

### Deprecated
- _No deprecations recorded yet._

### Removed
- _No removals recorded yet._

### Notes
- This changelog will be updated with each release to track interface evolution. Contributors: please use the template above for future changes.
---

