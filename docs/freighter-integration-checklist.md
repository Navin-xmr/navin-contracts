# Freighter Integration Checklist

Practical checklist for frontend teams integrating [Freighter](https://www.freighter.app/) with Navin contract calls.

---

## 1. Pre-Call Checks

Before submitting any transaction:

- [ ] **Wallet connected**: `freighter.isConnected()` returns `true`
- [ ] **Correct network**: `freighter.getNetwork()` matches your target (`TESTNET` or `PUBLIC`)
- [ ] **Public key retrieved**: `freighter.getPublicKey()` returns the expected address
- [ ] **Contract ID confirmed**: hardcode or load from config — never derive at runtime from user input
- [ ] **Run simulation first**: call `simulateTransaction` via RPC before signing; reject if `error` is present in the response

```typescript
import { SorobanRpc } from '@stellar/stellar-sdk';

const server = new SorobanRpc.Server('https://soroban-testnet.stellar.org');
const sim = await server.simulateTransaction(tx);

if (SorobanRpc.Api.isSimulationError(sim)) {
  // surface sim.error to the user — do NOT proceed to sign
  throw new Error(`Simulation failed: ${sim.error}`);
}

// assemble the transaction with the simulation result
const assembled = SorobanRpc.assembleTransaction(tx, sim).build();
```

---

## 2. Signing & Auth

- [ ] Pass the **assembled** (fee-bumped + footprint-set) transaction to Freighter, not the raw one
- [ ] Use `freighter.signTransaction(xdr, { network, networkPassphrase })` — always pass `networkPassphrase`
- [ ] After signing, verify the returned XDR is non-empty before submitting
- [ ] For multi-sig proposals (`approve_proposal` / `execute_proposal`), each admin must sign independently; collect all signatures before submitting

```typescript
const signedXdr = await freighter.signTransaction(assembled.toXDR(), {
  network: 'TESTNET',
  networkPassphrase: Networks.TESTNET,
});

if (!signedXdr) throw new Error('User rejected or signing failed');
```

### Common Auth Failures

| Symptom | Likely Cause | Fix |
|---------|-------------|-----|
| `Error: User declined` | User dismissed Freighter popup | Prompt user to retry; do not auto-retry silently |
| `HostError: Error(Auth, InvalidAction)` | Wrong signer for the required `Address` | Ensure the connected wallet matches the `caller` argument passed to the contract |
| `HostError: Error(Auth, InvalidAction)` on multi-sig | Threshold not met | Collect all required admin signatures before executing |
| Simulation succeeds but submission fails auth | Ledger advanced between sim and submit | Re-simulate and re-sign |

---

## 3. Submission & Error Handling

- [ ] Submit via `server.sendTransaction(signedTx)` and check `status !== 'ERROR'`
- [ ] Poll `server.getTransaction(hash)` until status is `SUCCESS` or `FAILED` (not `NOT_FOUND`)
- [ ] On `FAILED`, decode the result XDR to extract the `NavinError` code

```typescript
const send = await server.sendTransaction(signedTx);
if (send.status === 'ERROR') throw new Error(send.errorResult?.toXDR());

// poll
let result;
do {
  await new Promise(r => setTimeout(r, 2000));
  result = await server.getTransaction(send.hash);
} while (result.status === SorobanRpc.Api.GetTransactionStatus.NOT_FOUND);

if (result.status === SorobanRpc.Api.GetTransactionStatus.FAILED) {
  // result.resultXdr contains the contract error code
  console.error('Transaction failed', result.resultXdr);
}
```

### Contract Error Code Reference

Map `NavinError` codes surfaced in failed transactions to user-facing messages:

| Code | Name | User Message |
|------|------|-------------|
| 3 | `Unauthorized` | You don't have permission to perform this action |
| 4 | `ShipmentNotFound` | Shipment not found — verify the ID or check if it has expired |
| 5 | `InvalidStatus` | This action is not allowed in the shipment's current state |
| 8 | `InsufficientFunds` | Insufficient token balance for escrow deposit |
| 9 | `ShipmentAlreadyCompleted` | Shipment is already completed |
| 21 | `RateLimitExceeded` | Too many updates — wait before retrying |
| 41 | `DuplicateAction` | This action was already processed |
| 42 | `ShipmentUnavailable` | Shipment is archived; no further changes allowed |
| 43 | `ContractPaused` | Contract is paused; try again later |
| 46 | `CircuitBreakerOpen` | Token transfers temporarily disabled; try again later |

For the full error catalog see [`contracts/shipment/src/errors.rs`](../contracts/shipment/src/errors.rs).

---

## 4. Post-Submission Verification

After a transaction lands, verify on-chain state before updating UI:

- [ ] **Confirm transaction status** via `getTransaction` — `SUCCESS` only
- [ ] **Filter events by contract ID**: discard any event whose `contractId` does not match your known contract address
- [ ] **Match event topic** against the expected action (e.g. `shipment_created`, `status_updated`)
- [ ] **Verify data hash**: recompute `SHA-256` of your off-chain payload and compare against the `data_hash` field in the event

See [`contracts/shipment/docs/FRONTEND_VERIFICATION.md`](../contracts/shipment/docs/FRONTEND_VERIFICATION.md) for the full event verification procedure and sample RPC traces.

---

## 5. Explorer Inspection

To manually inspect a transaction or contract state:

| Network | Explorer |
|---------|----------|
| Testnet | https://stellar.expert/explorer/testnet |
| Mainnet | https://stellar.expert/explorer/public |

Steps:
1. Paste the transaction hash into the explorer search bar
2. Confirm **Result: success** and the correct **Contract ID** in the operations list
3. Expand **Contract Events** to inspect emitted topics and data values
4. Use **Stellar Laboratory** (`https://laboratory.stellar.org`) to decode raw XDR if needed

---

## 6. Common Failure Modes

| Failure | Cause | Resolution |
|---------|-------|-----------|
| Freighter popup never appears | `signTransaction` called before wallet is connected | Always await `isConnected()` check first |
| `fee_bump` rejected | Base fee too low during congestion | Use `server.getFeeStats()` to pick a competitive fee |
| Transaction expires (`txTOO_LATE`) | Submission delayed past `timeBounds.maxTime` | Set `maxTime` to at least 30 s from now; re-sign if expired |
| Simulation returns `InsufficientFunds` | Token allowance not set | Call `token.approve(contract_id, amount)` before the escrow deposit |
| `ShipmentUnavailable` (42) | Shipment archived to temp storage | Shipment lifecycle is complete; inform user, do not retry |
| `ContractPaused` (43) | Admin paused the contract | Poll contract state and surface a maintenance message |
