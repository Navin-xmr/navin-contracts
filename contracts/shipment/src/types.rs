use soroban_sdk::{contracttype, Address, BytesN, Map, Symbol, Vec};

pub const HASH_ALGO_SHA256: u32 = 1;
pub const DEFAULT_HASH_ALGO: u32 = HASH_ALGO_SHA256;

/// Expected number of decimal places for tokens used with this contract.
///
/// The contract assumes the Stellar/Soroban standard of 7 decimal places
/// (i.e., 1 token = 10_000_000 stroops). Tokens that return a different
/// value from their `decimals()` method will be rejected to prevent
/// mismatched amount interpretations during escrow operations.
pub const EXPECTED_TOKEN_DECIMALS: u32 = 7;

/// Storage keys for contract data.
///
/// # Examples
/// ```rust
/// use crate::types::DataKey;
/// let key = DataKey::Admin;
/// ```
#[contracttype(export = false)]
pub enum DataKey {
    /// The contract admin address.
    Admin,
    /// Contract version number, incremented on each upgrade.
    Version,
    /// Counter tracking total shipments created.
    ShipmentCount,
    /// Addresses with Company role.
    Company(Address),
    /// Addresses with Carrier role.
    Carrier(Address),
    /// Carrier suspension flag (carrier -> bool).
    CarrierSuspended(Address),
    /// Company suspension flag (company -> bool).
    CompanySuspended(Address),
    /// Individual shipment data keyed by ID.
    Shipment(u64),
    /// Carrier whitelist for a company — (company, carrier) -> bool.
    CarrierWhitelist(Address, Address),
    /// Role assigned to an address — (address, role) -> bool.
    UserRole(Address, Role),
    /// Role suspension status — (address, role) -> bool.
    RoleSuspended(Address, Role),
    /// Escrow balance for a shipment.
    Escrow(u64),
    /// Legacy single-role storage key for compatibility.
    Role(Address),
    /// Hash of proof-of-delivery data for a shipment.
    ConfirmationHash(u64),
    /// Token contract address for payments.
    TokenContract,
    /// NFT contract address for shipment tokenization (optional).
    NftContract,
    /// Timestamp of the last status update for a shipment (used for rate limiting).
    LastStatusUpdate(u64),
    /// Whether the pre-deadline warning has already been emitted for a shipment.
    DeadlineWarningEmitted(u64),
    /// Proposed new administrator address.
    ProposedAdmin,
    /// List of admin addresses for multi-sig.
    AdminList,
    /// Multi-sig threshold (number of approvals required).
    MultiSigThreshold,
    /// Counter for proposal IDs.
    ProposalCounter,
    /// Individual proposal data keyed by ID.
    Proposal(u64),
    /// Total escrow volume processed by the contract.
    TotalEscrowVolume,
    /// Total number of disputes raised.
    TotalDisputes,
    /// Count of shipments with a specific status.
    StatusCount(ShipmentStatus),
    /// Configurable limit on active shipments per company.
    ShipmentLimit,
    /// Per-company override for active shipment limit.
    CompanyShipmentLimit(Address),
    /// Counter for active shipments per company.
    ActiveShipmentCount(Address),
    /// Contract configuration parameters.
    ContractConfig,
    /// Event counter for a shipment (tracks number of events emitted).
    EventCount(u64),
    /// SHA-256 checksum of critical config fields for drift detection.
    ConfigChecksum,
    /// Counter for milestone events emitted for a shipment.
    MilestoneEventCount(u64),
    /// Temporary idempotency window key — present while the action hash is within its window.
    IdempotencyWindow(BytesN<32>),
    /// Contract pause state flag.
    IsPaused,
    /// Platform fee configuration.
    FeeConfig,
    /// Reserved. The platform treasury address lives in `FeeConfig::treasury`
    /// (the slot fee-payout code actually reads); this standalone key is kept
    /// only so the storage-key layout stays stable for already-deployed
    /// contracts and must not be written or read.
    Treasury,
    /// Rate limit quota tracker per actor (company/carrier).
    ActorQuota(Address),
    /// Circuit breaker state for token transfers.
    CircuitBreakerState,
    /// Admin-configured circuit breaker thresholds. Absent means the built-in
    /// default is in effect.
    CircuitBreakerConfig,
    /// Audit log entry keyed by entry ID.
    AuditEntry(u64),
    /// Total count of audit log entries.
    AuditEntryCount,
    /// Counter for condition breach events emitted for a shipment.
    BreachEventCount(u64),
    /// Contract-wide reentrancy lock flag for escrow-sensitive execution paths.
    ReentrancyLock,
    /// Settlement counter for generating unique settlement IDs.
    SettlementCounter,
    /// Settlement record keyed by settlement ID.
    Settlement(u64),
    /// Active settlement ID for a shipment (only one active settlement per shipment).
    ActiveSettlement(u64),
    /// Latest escrow freeze reason code keyed by shipment ID.
    EscrowFreezeReasonByShipment(u64),
    /// Per-company shipment creation quota tracker (for rate-limiting creation).
    CompanyCreationQuota(Address),
    /// Global shipment creation quota window configuration.
    CreationQuotaConfig,
    /// Deterministic action digest stored on proposal creation.
    ProposalDigest(u64),
    /// Per-shipment recovery action record (shipment_id, index) -> RecoveryRecord.
    /// Discriminant reserved for storage compatibility.
    RecoveryRecord(u64, u32),
    /// Total count of recovery action records for a shipment.
    /// Discriminant reserved for storage compatibility.
    RecoveryRecordCount(u64),
    /// Proposal salt used to prevent replay attacks — salt -> bool.
    ProposalSalt(BytesN<32>),
    /// Prerequisite shipment IDs for a dependent — dependent_id -> Vec<u64>.
    ShipmentDependents(u64),
    /// Counter for audit log entry IDs.
    AuditEntryCount,
    /// Individual audit log entry keyed by entry ID.
    AuditEntry(u64),
    /// Archived shipment data in temporary storage (for terminal state shipments).
    ArchivedShipment(u64),
}

/// Storage keys for dispute evidence.
///
/// Kept separate from [`DataKey`] deliberately: `DataKey` already carries the
/// maximum number of cases a single `#[contracttype]` enum may declare, so any
/// further per-shipment collection has to live in its own key type. Splitting
/// the dispute-evidence keys out also keeps the evidence subsystem
/// self-contained — it can be removed again without renumbering `DataKey`.
///
/// # Examples
/// ```rust
/// use crate::types::DisputeKey;
/// let key = DisputeKey::EvidenceCount(1);
/// ```
#[contracttype(export = false)]
pub enum DisputeKey {
    /// Number of evidence entries recorded for a shipment's dispute.
    EvidenceCount(u64),
    /// A single evidence hash — (shipment_id, zero-based index).
    Evidence(u64, u32),
}

/// Storage keys for settlement bookkeeping that do not fit in [`DataKey`].
///
/// `DataKey` is already at the `#[contracttype]` case limit, so additional
/// settlement keys live here.
#[contracttype(export = false)]
pub enum SettlementKey {
    /// Most recent settlement ID recorded for a shipment. Unlike
    /// `DataKey::ActiveSettlement`, it is not cleared when the settlement
    /// completes or fails.
    Latest(u64),
}

/// Storage key for a shipment's per-visit status data-hash log (issue #698).
///
/// `DataKey` is already at the `#[contracttype]` case limit, so this log lives
/// in its own key type — the same pattern as [`DisputeKey`] and
/// [`SettlementKey`].
///
/// One key per shipment holds the whole log ([`StatusHashLog`]) rather than one
/// key per visit: `update_status` runs on the hot path under a rate limit, and a
/// per-visit key scheme needs ~29 extra reads/writes/TTL bumps per call (ring
/// slot + sequence + per-status visit counter, then re-extending all of them).
/// A single bounded map keeps the cost at one read and one write.
///
/// # Examples
/// ```text
/// let key = StatusHashKey::Log(shipment_id);
/// ```
#[contracttype(export = false)]
pub enum StatusHashKey {
    /// Bounded status-hash log for one shipment.
    Log(u64),
}

/// Maximum number of status data-hash entries retained per shipment (issue #698).
///
/// The log is append-only but bounded: once this many visits have been recorded,
/// appending visit N evicts visit `N - MAX_STATUS_HASHES_PER_SHIPMENT`, i.e. the
/// oldest retained one. Bounding it is deliberate — an unbounded per-status map
/// is exactly the state growth removed upstream in #824, and every visit's hash
/// is already emitted permanently as a `status_updated` event for off-chain
/// indexing.
///
/// Sized to comfortably cover a full shipment lifecycle (typically well under
/// ten status updates) while keeping the per-shipment footprint small and
/// predictable.
pub const MAX_STATUS_HASHES_PER_SHIPMENT: u32 = 20;

/// A shipment's bounded status data-hash log (issue #698).
///
/// Stored as a single value under [`StatusHashKey::Log`] so that recording one
/// visit costs one read and one write.
///
/// `entries` is keyed by absolute visit sequence, which is monotonic across the
/// whole shipment, and holds at most [`MAX_STATUS_HASHES_PER_SHIPMENT`] items.
/// `visits` counts visits per status and yields each visit's `visit_index`, so
/// repeated visits to one status never overwrite one another.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct StatusHashLog {
    /// Sequence the next visit will receive. Monotonic, never reset.
    pub next_seq: u32,
    /// Retained visits by absolute sequence number, oldest first.
    pub entries: Map<u32, StatusHashRecord>,
    /// Visit count per status, used to assign `visit_index`.
    pub visits: Map<ShipmentStatus, u32>,
}

/// One recorded status-visit hash from a shipment's bounded status-hash log.
///
/// Returned by `get_status_hash` / `get_status_hash_history` so a caller can
/// tell *which* visit a hash belongs to, not just read the latest value.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct StatusHashRecord {
    /// Status the shipment held when this hash was recorded.
    pub status: ShipmentStatus,
    /// SHA-256 hash of the off-chain IoT payload supplied for this visit.
    pub data_hash: BytesN<32>,
    /// Zero-based index of this visit *among visits to `status`*. Distinct
    /// visits to the same status get distinct indices, so neither overwrites
    /// the other. Monotonic per status and never reused.
    pub visit_index: u32,
    /// Position of this entry in the shipment's log. Always
    /// strictly increasing; `seq % MAX_STATUS_HASHES_PER_SHIPMENT` is the ring
    /// slot the entry occupies.
    pub seq: u32,
    /// Ledger timestamp of the visit.
    pub timestamp: u64,
    /// Address that performed the status update.
    pub actor: Address,
}

/// Structured reason codes for escrow freeze events.
///
/// Attached to `escrow_frozen` events so that off-chain indexers and
/// auditing systems can distinguish *why* an escrow was frozen without
/// parsing free-form fields.
///
/// # Examples
/// ```rust
/// use crate::types::EscrowFreezeReason;
/// let reason = EscrowFreezeReason::DisputeRaised;
/// ```
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum EscrowFreezeReason {
    /// Escrow frozen because a dispute was raised on the shipment.
    DisputeRaised,
    /// Escrow frozen because the carrier account was suspended.
    CarrierSuspended,
    /// Escrow frozen because the company account was suspended.
    CompanySuspended,
    /// Escrow frozen because the contract was paused by an admin.
    ContractPaused,
    /// Escrow frozen due to the token-transfer circuit breaker opening.
    CircuitBreakerOpen,
}

/// Supported user roles.
///
/// # Examples
/// ```rust
/// use crate::types::Role;
/// let role = Role::Company;
/// ```
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum Role {
    /// A registered company that can create shipments.
    Company,
    /// A registered carrier that can transport shipments and report geofence events.
    Carrier,
    /// A guardian that can approve emergency operations.
    Guardian,
    /// An operator that can perform operational tasks.
    Operator,
    /// No role assigned.
    Unassigned,
}

/// Role change actions for RBAC audit trail.
///
/// # Examples
/// ```rust
/// use crate::types::RoleChangeAction;
/// let action = RoleChangeAction::Assigned;
/// ```
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum RoleChangeAction {
    /// Role was assigned to an address.
    Assigned,
    /// Role was revoked from an address.
    Revoked,
    /// Role was temporarily suspended.
    Suspended,
    /// Role was reactivated after suspension.
    Reactivated,
}

/// Shipment status lifecycle.
///
/// # Examples
/// ```rust
/// use crate::types::ShipmentStatus;
/// let status = ShipmentStatus::Created;
/// ```
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum ShipmentStatus {
    /// Shipment has been created but not yet picked up.
    Created,
    /// Shipment is in transit between checkpoints.
    InTransit,
    /// Shipment has arrived at an intermediate checkpoint.
    AtCheckpoint,
    /// Shipment has been partially delivered and partially settled.
    PartiallyDelivered,
    /// Shipment has been delivered to the receiver.
    Delivered,
    /// Shipment is under dispute.
    Disputed,
    /// Shipment has been cancelled.
    Cancelled,
    /// Shipment has been partially refunded after dispute or cancellation.
    PartiallyRefunded,
}

impl ShipmentStatus {
    /// Checks if a transition from the current status to a new status is valid.
    ///
    /// ### Status Transition Diagram
    /// ```text
    ///           +-----------+       +-----------+       +-----------+
    ///           |  Created  |------>| InTransit |<----->| AtCheckpt |
    ///           +-----------+       +-----------+       +-----------+
    ///                 |                   |                   |
    ///                 |           +-------+-------+-----------+
    ///                 |           |               |
    ///                 v           v               v
    ///           +-----------+-----------+   +-----------+
    ///           | Cancelled | Disputed  |<--| Delivered |
    ///           +-----------+-----------+   +-----------+
    ///                               |
    ///                               v
    ///                         (Terminal States)
    /// ```
    ///
    /// **Valid Transitions:**
    /// - `Created` -> `InTransit`, `Cancelled`
    /// - `InTransit` -> `AtCheckpoint`, `Delivered`, `Disputed`
    /// - `AtCheckpoint` -> `InTransit`, `Delivered`, `Disputed`
    /// - `Any` -> `Cancelled` (except `Delivered`)
    /// - `Any` -> `Disputed` (except `Cancelled`, `Delivered`)
    /// - `Disputed` -> `Cancelled`, `Delivered` (Special recovery cases if needed)
    ///
    /// # Arguments
    /// * `to` - The target status to transition to.
    ///
    /// # Returns
    /// * `bool` - `true` if the transition is valid, `false` otherwise.
    ///
    /// # Examples
    /// ```rust
    /// use crate::types::ShipmentStatus;
    /// let status = ShipmentStatus::Created;
    /// assert!(status.is_valid_transition(&ShipmentStatus::InTransit));
    /// ```
    pub fn is_valid_transition(&self, to: &ShipmentStatus) -> bool {
        match (self, to) {
            (Self::Created, Self::InTransit) => true,
            (Self::Created, Self::Cancelled) => true,
            (Self::Created, Self::Disputed) => true,
            (Self::InTransit, Self::AtCheckpoint) => true,
            (Self::InTransit, Self::PartiallyDelivered) => true,
            (Self::InTransit, Self::Delivered) => true,
            (Self::InTransit, Self::Disputed) => true,
            (Self::InTransit, Self::Cancelled) => true,
            // Issue #542 — a shipment must return to `InTransit` before
            // entering another checkpoint, so direct `AtCheckpoint ->
            // AtCheckpoint` hops are explicitly rejected here rather than
            // falling through to the catch-all `_ => false` arm. This
            // protects against off-by-one workflow code that would
            // otherwise silently record duplicate checkpoint entries.
            (Self::AtCheckpoint, Self::AtCheckpoint) => false,
            (Self::AtCheckpoint, Self::InTransit) => true,
            (Self::AtCheckpoint, Self::PartiallyDelivered) => true,
            (Self::AtCheckpoint, Self::Delivered) => true,
            (Self::PartiallyDelivered, Self::PartiallyDelivered) => true,
            (Self::PartiallyDelivered, Self::Delivered) => true,
            (Self::PartiallyDelivered, Self::Disputed) => true,
            (Self::PartiallyDelivered, Self::Cancelled) => true,
            (Self::AtCheckpoint, Self::Disputed) => true,
            (Self::AtCheckpoint, Self::Cancelled) => true,
            (Self::Disputed, Self::Cancelled) => true,
            (Self::Disputed, Self::Delivered) => true,
            // PartiallyRefunded is a terminal state - no transitions out
            (Self::PartiallyRefunded, _) => false,
            // Transitions TO PartiallyRefunded (from Disputed after partial refund resolution)
            (Self::Disputed, Self::PartiallyRefunded) => true,
            (_, Self::Cancelled) if self != &Self::Delivered && self != &Self::Cancelled => true,
            (_, Self::Disputed)
                if self != &Self::Cancelled
                    && self != &Self::Delivered
                    && self != &Self::Disputed =>
            {
                true
            }
            _ => false,
        }
    }
}

/// Core shipment data stored on-chain.
/// Raw payload is off-chain; only the hash is stored.
///
/// # Examples
/// ```rust
/// // Struct represents the full shipment payload tracked on-chain.
/// ```
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Shipment {
    /// Unique shipment identifier.
    pub id: u64,
    /// Address that created the shipment.
    pub sender: Address,
    /// Intended recipient of the shipment.
    pub receiver: Address,
    /// Carrier responsible for transport.
    pub carrier: Address,
    /// Current status in the shipment lifecycle.
    pub status: ShipmentStatus,
    /// SHA-256 hash of the off-chain shipment data.
    pub data_hash: BytesN<32>,
    /// Ledger timestamp when the shipment was created.
    pub created_at: u64,
    /// Ledger timestamp of the last status update.
    pub updated_at: u64,
    /// Amount held in escrow for this shipment.
    pub escrow_amount: i128,
    /// Total amount deposited in escrow.
    pub total_escrow: i128,
    /// Milestone-based payment schedule: (checkpoint name, percentage).
    pub payment_milestones: Vec<(Symbol, u32)>,
    /// List of symbols for milestones that have already been paid.
    pub paid_milestones: Vec<Symbol>,
    /// List of symbols for checkpoints that have been hit and recorded.
    pub milestones_completed: Vec<Symbol>,
    /// Timestamp after which the shipment is considered expired and can be auto-cancelled.
    pub deadline: u64,
    /// Counter to prevent replay of external actions and correlate off-chain integrations.
    pub integration_nonce: u32,
    /// Whether the shipment is finalized (terminal state reached and escrow cleared).
    pub finalized: bool,
}

/// A checkpoint milestone recorded during shipment transit.
/// Only the data hash is stored; full details live off-chain.
///
/// # Examples
/// ```rust
/// // Struct represents a milestone reached by a shipment.
/// ```
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Milestone {
    /// ID of the shipment this milestone belongs to.
    pub shipment_id: u64,
    /// Symbolic name of the checkpoint (e.g. "warehouse", "port").
    pub checkpoint: Symbol,
    /// SHA-256 hash of the off-chain milestone data.
    pub data_hash: BytesN<32>,
    /// Ledger timestamp when the milestone was recorded.
    pub timestamp: u64,
    /// Address that reported this milestone.
    pub reporter: Address,
}

/// Condition breach types reported by carriers for out-of-range sensor readings.
///
/// # Examples
/// ```rust
/// use crate::types::BreachType;
/// let breach = BreachType::TemperatureHigh;
/// ```
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum BreachType {
    /// Temperature exceeded the upper acceptable limit.
    TemperatureHigh,
    /// Temperature dropped below the lower acceptable limit.
    TemperatureLow,
    /// Humidity exceeded the upper acceptable limit.
    HumidityHigh,
    /// Physical impact detected (shock/drop event).
    Impact,
    /// Tamper detection triggered.
    TamperDetected,
}

/// Severity levels for condition breach events used for downstream analytics and alerting.
///
/// # Examples
/// ```rust
/// use crate::types::Severity;
/// let severity = Severity::High;
/// ```
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum Severity {
    /// Minor deviation with minimal impact on shipment integrity.
    Low,
    /// Moderate deviation requiring attention but not critical.
    Medium,
    /// Significant deviation that may compromise shipment quality.
    High,
    /// Critical deviation requiring immediate intervention.
    Critical,
}

/// Settlement state for tracking token transfer lifecycle.
///
/// Provides explicit in-flight state tracking for payment operations
/// to improve observability and failure handling.
///
/// # Examples
/// ```rust
/// use crate::types::SettlementState;
/// let state = SettlementState::Pending;
/// ```
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum SettlementState {
    /// No settlement operation in progress.
    None,
    /// Settlement operation initiated, awaiting token transfer.
    Pending,
    /// Token transfer completed successfully.
    Completed,
    /// Token transfer failed, requires investigation or retry.
    Failed,
}

/// Settlement operation type for tracking different payment flows.
///
/// # Examples
/// ```rust
/// use crate::types::SettlementOperation;
/// let op = SettlementOperation::Deposit;
/// ```
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum SettlementOperation {
    /// Escrow deposit from company to contract.
    Deposit,
    /// Escrow release from contract to carrier.
    Release,
    /// Escrow refund from contract to company.
    Refund,
    /// Milestone payment from contract to carrier.
    MilestonePayment,
}

/// Settlement record for tracking token transfer operations.
///
/// # Examples
/// ```rust
/// // Struct represents a settlement operation record.
/// ```
#[contracttype]
#[derive(Clone, Debug)]
pub struct SettlementRecord {
    /// Unique settlement identifier.
    pub settlement_id: u64,
    /// Associated shipment ID.
    pub shipment_id: u64,
    /// Type of settlement operation.
    pub operation: SettlementOperation,
    /// Current state of the settlement.
    pub state: SettlementState,
    /// Amount being transferred.
    pub amount: i128,
    /// Source address of the transfer.
    pub from: Address,
    /// Destination address of the transfer.
    pub to: Address,
    /// Ledger timestamp when settlement was initiated.
    pub initiated_at: u64,
    /// Ledger timestamp when settlement was completed or failed.
    pub completed_at: Option<u64>,
    /// Optional error message for failed settlements.
    pub error_code: Option<u32>,
}

/// Geofence event types for tracking shipment location events.
///
/// # Examples
/// ```rust
/// use crate::types::GeofenceEvent;
/// let event = GeofenceEvent::ZoneEntry;
/// ```
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum GeofenceEvent {
    /// Shipment entered a predefined geographical zone.
    ZoneEntry,
    /// Shipment exited a predefined geographical zone.
    ZoneExit,
    /// Shipment deviated from the expected route.
    RouteDeviation,
}

/// Input data for creating a shipment in a batch.
///
/// # Examples
/// ```rust
/// // Struct represents batch creation parameters for a shipment.
/// ```
#[contracttype]
#[derive(Clone, Debug)]
pub struct ShipmentInput {
    pub receiver: Address,
    pub carrier: Address,
    pub data_hash: BytesN<32>,
    pub payment_milestones: Vec<(Symbol, u32)>,
    pub deadline: u64,
}

/// On-chain introspection snapshot of the contract state.
///
/// # Examples
/// ```rust
/// // Struct holds metadata about the contract state itself.
/// ```
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ContractMetadata {
    /// Current contract version (starts at 1, incremented on each upgrade).
    pub version: u32,
    /// Address of the contract administrator.
    pub admin: Address,
    /// Total number of shipments created since initialization.
    pub shipment_count: u64,
    /// Whether the contract has been initialized.
    pub initialized: bool,
    /// Current hash algorithm version used for verification.
    pub hash_algo_version: u32,
}

/// Structured migration report emitted on upgrade completion.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct MigrationReport {
    /// Contract version before migration.
    pub current_version: u32,
    /// Contract version after migration.
    pub target_version: u32,
    /// Number of shipment entries affected or estimated.
    pub affected_shipments: u64,
}

/// Dispute resolution options for admin.
///
/// # Examples
/// ```rust
/// use crate::types::DisputeResolution;
/// let resolution = DisputeResolution::RefundToCompany;
/// ```
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum DisputeResolution {
    /// Release escrowed funds to the carrier.
    ReleaseToCarrier,
    /// Refund escrowed funds to the company.
    RefundToCompany,
    /// Split escrowed funds between carrier and company (partial refund).
    /// Results in `PartiallyRefunded` status. Arbiter provides the split
    /// via this flag — escrow is divided evenly between carrier and company.
    PartialRefund,
}

/// Admin action types for multi-signature proposals.
///
/// # Examples
/// ```rust
/// use crate::types::AdminAction;
/// let action = AdminAction::Upgrade(hash);
/// ```
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum AdminAction {
    /// Upgrade contract to new WASM hash.
    Upgrade(BytesN<32>),
    /// Transfer admin role to new address.
    TransferAdmin(Address),
    /// Force release escrow for a shipment to carrier.
    /// Includes a reason_hash for audit trail and accountability.
    ForceRelease(u64, BytesN<32>),
    /// Force refund escrow for a shipment to company.
    /// Includes a reason_hash for audit trail and accountability.
    ForceRefund(u64, BytesN<32>),
}

/// Multi-signature proposal for critical admin actions.
///
/// # Examples
/// ```rust
/// // Struct represents a pending multi-sig proposal.
/// ```
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Proposal {
    /// Unique proposal identifier.
    pub id: u64,
    /// Address that created the proposal.
    pub proposer: Address,
    /// The action to be executed.
    pub action: AdminAction,
    /// List of addresses that have approved this proposal.
    pub approvals: Vec<Address>,
    /// Ledger timestamp when the proposal was created.
    pub created_at: u64,
    /// Ledger timestamp when the proposal expires.
    pub expires_at: u64,
    /// Whether the proposal has been executed.
    pub executed: bool,
}

/// Notification types for backend indexing and push notifications.
///
/// # Examples
/// ```rust
/// use crate::types::NotificationType;
/// let notif = NotificationType::ShipmentCreated;
/// ```
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum NotificationType {
    /// Shipment was created.
    ShipmentCreated,
    /// Shipment status changed.
    StatusChanged,
    /// Delivery was confirmed.
    DeliveryConfirmed,
    /// Escrow was released.
    EscrowReleased,
    /// Dispute was raised.
    DisputeRaised,
    /// Dispute was resolved.
    DisputeResolved,
    /// Deadline is approaching.
    DeadlineApproaching,
    /// Carrier handoff occurred — ownership transferred to a new carrier.
    CarrierHandoff,
}

/// Aggregated on-chain analytics data.
///
/// # Examples
/// ```rust
/// // Struct represents basic analytics counters for the contract.
/// ```
#[contracttype]
#[derive(Clone, Debug)]
pub struct Analytics {
    /// Total number of shipments created.
    pub total_shipments: u64,
    /// Total volume of funds moved into escrow.
    pub total_escrow_volume: i128,
    /// Total number of disputes raised.
    pub total_disputes: u64,
    /// Number of shipments currently in 'Created' state.
    pub created_count: u64,
    /// Number of shipments currently in 'InTransit' state.
    pub in_transit_count: u64,
    /// Number of shipments currently in 'AtCheckpoint' state.
    pub at_checkpoint_count: u64,
    /// Number of shipments currently in 'PartiallyDelivered' state.
    pub partially_delivered_count: u64,
    /// Number of shipments currently in 'Delivered' state.
    pub delivered_count: u64,
    /// Number of shipments currently in 'Disputed' state.
    pub disputed_count: u64,
    /// Number of shipments currently in 'Cancelled' state.
    pub cancelled_count: u64,
    /// Number of shipments currently in 'PartiallyRefunded' state.
    pub partially_refunded_count: u64,
}

/// Compact summary of shipment counts aggregated by status.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ShipmentStatusSummary {
    /// Count of shipments in 'Created' state.
    pub created: u64,
    /// Count of shipments in 'InTransit' state.
    pub in_transit: u64,
    /// Count of shipments in 'AtCheckpoint' state.
    pub at_checkpoint: u64,
    /// Count of shipments in 'PartiallyDelivered' state.
    pub partially_delivered: u64,
    /// Count of shipments in 'Delivered' state.
    pub delivered: u64,
    /// Count of shipments in 'Disputed' state.
    pub disputed: u64,
    /// Count of shipments in 'Cancelled' state.
    pub cancelled: u64,
    /// Count of shipments in 'PartiallyRefunded' state.
    pub partially_refunded: u64,
}

/// Paginated result for company-carrier relationship queries (issue #295).
///
/// Returns a page of carrier addresses whitelisted by a company, with a
/// cursor for deterministic pagination.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct CarrierRelationshipPage {
    /// Carrier addresses in this page.
    pub carriers: Vec<Address>,
    /// Cursor to pass on the next call, or `None` if this is the last page.
    pub next_cursor: Option<u32>,
    /// Total number of entries scanned (not total whitelisted — use for diagnostics).
    pub total_scanned: u32,
}

/// Per-company creation quota tracker for the sliding-window rate limit (issue #296).
///
/// Stored under `DataKey::CompanyCreationQuota(company)` in instance storage.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct CreationQuotaTracker {
    /// Number of shipments created in the current window.
    pub count: u32,
    /// Ledger timestamp when the current window started.
    pub window_start: u64,
}

/// Deterministic action digest stored on proposal creation (issue #297).
///
/// Stored under `DataKey::ProposalDigest(proposal_id)` in persistent storage.
/// Enables off-chain signers to verify the exact payload before approving.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ProposalActionDigest {
    /// The proposal ID this digest belongs to.
    pub proposal_id: u64,
    /// SHA-256 hash of the canonical serialization of the proposal action.
    pub digest: BytesN<32>,
    /// Ledger timestamp when the digest was computed.
    pub computed_at: u64,
}

/// TTL health summary returned by `get_ttl_health_summary`.
///
/// Provides a snapshot of how many shipments are in persistent storage
/// versus archived or missing, along with the configured TTL parameters.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct TtlHealthSummary {
    /// Total number of shipments tracked by the contract counter.
    pub total_shipment_count: u64,
    /// Number of shipments sampled in this health check.
    pub sampled_count: u64,
    /// Number of sampled shipments found in persistent storage.
    pub persistent_count: u64,
    /// Number of sampled shipments not in persistent storage (archived or missing).
    pub missing_or_archived_count: u64,
    /// Percentage of sampled shipments in persistent storage (0–100).
    pub persistent_percentage: u32,
    /// Configured TTL threshold (ledgers remaining before extension fires).
    pub ttl_threshold: u32,
    /// Configured TTL extension (ledgers added when threshold is reached).
    pub ttl_extension: u32,
    /// Current ledger sequence number at query time.
    pub current_ledger: u32,
    /// Current ledger timestamp at query time.
    pub query_timestamp: u64,
}

/// Configuration for platform revenue collection.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct FeeConfig {
    /// The fee in basis points (1 bps = 0.01%). Capped at 1000 (10%).
    pub fee_bps: u32,
    /// The address where collected fees are sent.
    pub treasury: Address,
}

/// Sample fractional milestone percentages used in tests to verify
/// integer-division rounding behavior during milestone payouts.
/// Values (17, 33, 50) sum to exactly 100.
pub const FRACTIONAL_MILESTONE_PCTS: [u32; 3] = [17, 33, 50];
