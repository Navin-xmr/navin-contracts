# Navin Contract Integration Guide

Complete guide for integrating the Navin shipment tracking smart contract with your Express.js backend using the Stellar JavaScript/TypeScript SDK.

## Table of Contents
# Navin Contract Integration Guide

Complete guide for integrating the Navin shipment tracking smart contract with your Express.js backend using the Stellar JavaScript/TypeScript SDK.

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Contract Schema (ABI)](#contract-schema-abi)
3. [Event Payload Schemas](#event-payload-schemas)
4. [Setup & Configuration](#setup--configuration)
5. [Contract Invocation](#contract-invocation)
6. [Immutable Provenance Queries](#immutable-provenance-queries)
7. [Event Listening](#event-listening)
8. [Transaction Verification](#transaction-verification)
9. [Complete Examples](#complete-examples)

## Contract Schema (ABI)

The shipment contract publishes a machine-readable XDR interface schema at `docs/contract-schema.shipment.json`. This file is the authoritative source of truth for all function signatures, argument types, and return types exposed by the contract. Backend indexers and frontend clients should use it to deserialize call results and build typed invocations without manual reverse-engineering.

### Regenerating the Schema

Run the following command from the repository root after any contract change:

```bash
make generate-schema-shipment
```

This will:
1. Build the contracts (`stellar contract build`)
2. Run `stellar contract info interface --wasm target/wasm32-unknown-unknown/release/shipment.wasm --output json-formatted`
3. Write the output to `docs/contract-schema.shipment.json`

Commit the updated `docs/contract-schema.shipment.json` alongside your contract changes.

### Backend Usage

The schema JSON is an array of contract spec entries (functions, types, errors). Parse it to dynamically build decoders or validate call shapes:

```typescript
import schema from '../docs/contract-schema.shipment.json';
import { xdr, Contract } from '@stellar/stellar-sdk';

// Find a function entry
const fnEntry = schema.find(
  (e: any) => e.function_v0?.name === 'create_shipment'
);

// Use stellar-sdk's ContractSpec to decode return values
const spec = new Contract.Spec(
  schema.map((e: any) => xdr.ScSpecEntry.fromJSON(e))
);

const decoded = spec.funcReturnType('get_shipment');
```

Alternatively, use `stellar contract bindings typescript` to auto-generate a fully typed TypeScript client directly from the schema:

```bash
stellar contract bindings typescript \
  --wasm target/wasm32-unknown-unknown/release/shipment.wasm \
  --output-dir src/generated/shipment-client
```

### Frontend Usage

Import the generated TypeScript client or load the schema JSON at build time to drive type-safe contract calls:

```typescript
// With generated bindings
import { Client as ShipmentClient } from './generated/shipment-client';

const client = new ShipmentClient({
  contractId: SHIPMENT_CONTRACT_ID,
  networkPassphrase: Networks.TESTNET,
  rpcUrl: RPC_URL,
});

const shipment = await client.get_shipment({ shipment_id: BigInt(1) });
```

The schema also documents every `#[contracterror]` variant so frontend error-handling can display user-friendly messages for each code.

### Schema Drift Enforcement in CI

CI will fail if the committed `docs/contract-schema.shipment.json` diverges from the schema regenerated during the build. The `schema-drift` job in `.github/workflows/test.yml`:

1. Builds the contracts from source
2. Regenerates the schema using the same `stellar contract info interface` command
3. Diffs the result against the committed file
4. Fails the workflow if any difference is found

This ensures the schema file stays in sync with the contract code on every PR. If CI fails with a schema drift error, run `make generate-schema-shipment` locally, commit the updated JSON, and push again.

## Event Payload Schemas

Use `docs/event_schemas.md` as the central JSON-schema-like reference for emitted event payloads and field ordering.

Indexer conformance fixtures are validated by tests in:

- `contracts/shipment/src/test_event_fixtures.rs`
- `contracts/shipment/test_snapshots/test_event_fixtures/`

When parser logic changes, regenerate and re-validate against these fixture outputs before release.

## Architecture Overview

Navin uses a **Hash-and-Emit** architecture:

- **On-chain**: Contract stores only critical data (shipment IDs, addresses, status, escrow amounts) and emits events with data hashes
- **Off-chain**: Backend (MongoDB) stores full shipment details (GPS coordinates, sensor readings, photos, metadata)
- **Verification**: Data integrity is verified by comparing on-chain hashes with off-chain data hashes

```
┌─────────────────┐         ┌──────────────────┐         ┌─────────────────┐
│   Frontend      │────────▶│  Express Backend │────────▶│ Stellar Network │
│   (React)       │         │   (Indexer)      │         │  (Soroban)      │
└─────────────────┘         └──────────────────┘         └─────────────────┘
                                     │                            │
                                     │                            │
                                     ▼                            ▼
                            ┌──────────────────┐         ┌─────────────────┐
                            │    MongoDB       │         │  Event Stream   │
                            │  (Full Data)     │◀────────│  (Horizon)      │
                            └──────────────────┘         └─────────────────┘
```

## Setup & Configuration

### Installation

```bash
npm install @stellar/stellar-sdk
# or
yarn add @stellar/stellar-sdk
```

### Environment Configuration

Create a configuration file for network settings:

```typescript
// src/config/stellar.config.ts
```

export interface StellarConfig {
networkPassphrase: string;
rpcUrl: string;
horizonUrl: string;
contractId: string;
sourceSecretKey: string;
}

export const testnetConfig: StellarConfig = {
networkPassphrase: 'Test SDF Network ; September 2015',
rpcUrl: 'https://soroban-testnet.stellar.org:443',
horizonUrl: 'https://horizon-testnet.stellar.org',
contractId: process.env.SHIPMENT_CONTRACT_ID!,
sourceSecretKey: process.env.STELLAR_SECRET_KEY!,
};

export const mainnetConfig: StellarConfig = {
networkPassphrase: 'Public Global Stellar Network ; September 2015',
rpcUrl: 'https://soroban-rpc.stellar.org:443',
horizonUrl: 'https://horizon.stellar.org',
contractId: process.env.SHIPMENT_CONTRACT_ID!,
sourceSecretKey: process.env.STELLAR_SECRET_KEY!,
};

// Select config based on environment
export const config = process.env.NODE_ENV === 'production'
? mainnetConfig
: testnetConfig;

````

### Initialize Stellar SDK

```typescript
// src/services/stellar.service.ts
import {
  SorobanRpc,
  Keypair,
  Contract,
  TransactionBuilder,
  Networks,
  Operation,
  BASE_FEE,
  Address,
  xdr,
  scValToNative,
  nativeToScVal,
} from '@stellar/stellar-sdk';
import { config } from '../config/stellar.config';

export class StellarService {
  private server: SorobanRpc.Server;
  private sourceKeypair: Keypair;
  private contract: Contract;

  constructor() {
    this.server = new SorobanRpc.Server(config.rpcUrl);
    this.sourceKeypair = Keypair.fromSecret(config.sourceSecretKey);
    this.contract = new Contract(config.contractId);
  }

  async getAccount() {
    return await this.server.getAccount(this.sourceKeypair.publicKey());
  }
}
````

## Contract Invocation

### Example 1: Create a Shipment

```typescript
// src/services/shipment.service.ts
```

import { StellarService } from './stellar.service';
import { createHash } from 'crypto';

export class ShipmentService extends StellarService {

/\*\*

- Create a new shipment on-chain
  \*/
  async createShipment(shipmentData: {
  sender: string;
  receiver: string;
  carrier: string;
  offChainData: any;
  paymentMilestones: Array<{ checkpoint: string; percentage: number }>;
  deadline: Date;
  }) {
  try {
  // 1. Hash the off-chain data
  const dataHash = this.hashOffChainData(shipmentData.offChainData);

      // 2. Prepare contract arguments
      const milestones = shipmentData.paymentMilestones.map(m => [
        m.checkpoint,
        m.percentage
      ]);

      // 3. Build transaction
      const account = await this.getAccount();
      const transaction = new TransactionBuilder(account, {
        fee: BASE_FEE,
        networkPassphrase: config.networkPassphrase,
      })
        .addOperation(
          this.contract.call(
            'create_shipment',
            Address.fromString(shipmentData.sender),
            Address.fromString(shipmentData.receiver),
            Address.fromString(shipmentData.carrier),
            nativeToScVal(Buffer.from(dataHash, 'hex'), { type: 'bytes' }),
            nativeToScVal(milestones, { type: 'vec' }),
            nativeToScVal(Math.floor(shipmentData.deadline.getTime() / 1000), { type: 'u64' })
          )
        )
        .setTimeout(30)
        .build();

      // 4. Sign and submit
      transaction.sign(this.sourceKeypair);
      const response = await this.server.sendTransaction(transaction);

      // 5. Wait for confirmation
      if (response.status === 'PENDING') {
        const result = await this.server.getTransaction(response.hash);
        return {
          success: true,
          txHash: response.hash,
          shipmentId: this.extractShipmentIdFromResult(result),
          dataHash
        };
      }

      throw new Error(`Transaction failed: ${response.status}`);

  } catch (error) {
  console.error('Failed to create shipment:', error);
  throw error;
  }
  }

/\*\*

- Update shipment status
  \*/
  async updateShipmentStatus(
  caller: string,
  shipmentId: number,
  newStatus: string,
  offChainData: any
  ) {
  const dataHash = this.hashOffChainData(offChainData);


    const account = await this.getAccount();
    const transaction = new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: config.networkPassphrase,
    })
      .addOperation(
        this.contract.call(
          'update_status',
          Address.fromString(caller),
          nativeToScVal(shipmentId, { type: 'u64' }),
          nativeToScVal(newStatus, { type: 'symbol' }),
          nativeToScVal(Buffer.from(dataHash, 'hex'), { type: 'bytes' })
        )
      )
      .setTimeout(30)
      .build();

    transaction.sign(this.sourceKeypair);
    const response = await this.server.sendTransaction(transaction);

    return {
      success: response.status === 'SUCCESS',
      txHash: response.hash,
      dataHash
    };

}

/\*\*

- Record milestone for shipment
  \*/
  async recordMilestone(
  carrier: string,
  shipmentId: number,
  checkpoint: string,
  offChainData: any
  ) {
  const dataHash = this.hashOffChainData(offChainData);


    const account = await this.getAccount();
    const transaction = new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: config.networkPassphrase,
    })
      .addOperation(
        this.contract.call(
          'record_milestone',
          Address.fromString(carrier),
          nativeToScVal(shipmentId, { type: 'u64' }),
          nativeToScVal(checkpoint, { type: 'symbol' }),
          nativeToScVal(Buffer.from(dataHash, 'hex'), { type: 'bytes' })
        )
      )
      .setTimeout(30)
      .build();

    transaction.sign(this.sourceKeypair);
    const response = await this.server.sendTransaction(transaction);

    return {
      success: response.status === 'SUCCESS',
      txHash: response.hash,
      dataHash
    };

}

private hashOffChainData(data: any): string {
const jsonString = JSON.stringify(data, Object.keys(data).sort());
return createHash('sha256').update(jsonString).digest('hex');
}

private extractShipmentIdFromResult(result: any): number {
// Extract shipment ID from transaction result
// Implementation depends on Stellar SDK response format
return scValToNative(result.returnValue);
}
}

````

## Immutable Provenance Queries

Use these read-only methods to render verification views without fetching the full shipment payload.

- `get_shipment_creator(shipment_id: u64) -> Address`: Returns the immutable creator (sender) address captured at shipment creation.
- `get_shipment_receiver(shipment_id: u64) -> Address`: Returns the immutable receiver address captured at shipment creation.

Both methods fail with `ShipmentNotFound` for unknown IDs, which helps frontend verification flows distinguish missing records from mismatched identities.

### Restore Triage Query

For pre-restore operations, call:

- `get_restore_diagnostics(shipment_id: u64) -> PersistentRestoreDiagnostics`

Interpretation guidance:

- `ActivePersistent`: no restore path needed for the primary shipment payload.
- `ArchivedExpected`: shipment has been archived and may require restore flow depending on operator policy.
- `Missing`: shipment ID is absent from both persistent and archived paths.
- `InconsistentDualPresence`: both paths are populated; investigate before any restore mutation.

This method is read-only and does not mutate contract state.

```typescript
// src/services/shipment-query.service.ts
import {
  Contract,
  TransactionBuilder,
  BASE_FEE,
  Address,
  nativeToScVal,
} from '@stellar/stellar-sdk';
import { StellarService } from './stellar.service';
import { config } from '../config/stellar.config';

export class ShipmentQueryService extends StellarService {
  async getShipmentCreator(shipmentId: number): Promise<string> {
    const contract = new Contract(config.contractId);
    const account = await this.getAccount();
    const tx = new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: config.networkPassphrase,
    })
      .addOperation(
        contract.call(
          'get_shipment_creator',
          nativeToScVal(shipmentId, { type: 'u64' })
        )
      )
      .setTimeout(30)
      .build();

    const sim = await this.server.simulateTransaction(tx);
    return Address.fromScVal(sim.result!.retval).toString();
  }

  async getShipmentReceiver(shipmentId: number): Promise<string> {
    const contract = new Contract(config.contractId);
    const account = await this.getAccount();
    const tx = new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: config.networkPassphrase,
    })
      .addOperation(
        contract.call(
          'get_shipment_receiver',
          nativeToScVal(shipmentId, { type: 'u64' })
        )
      )
      .setTimeout(30)
      .build();

    const sim = await this.server.simulateTransaction(tx);
    return Address.fromScVal(sim.result!.retval).toString();
  }
}
```

## Event Listening

### Horizon Event Stream Listener

```typescript
// src/services/event-listener.service.ts
import { Server } from '@stellar/stellar-sdk/lib/horizon';
import { config } from '../config/stellar.config';

export class EventListenerService {
  private horizonServer: Server;

  constructor() {
    this.horizonServer = new Server(config.horizonUrl);
  }

  /**
   * Listen for contract events for a specific shipment
   */
  async listenForShipmentEvents(shipmentId: number, callback: (event: any) => void) {
    const eventStream = this.horizonServer
      .effects()
      .forAccount(config.contractId)
      .cursor('now')
      .stream({
        onmessage: (effect) => {
          if (this.isShipmentEvent(effect, shipmentId)) {
            callback(this.parseContractEvent(effect));
          }
        },
        onerror: (error) => {
          console.error('Event stream error:', error);
        }
      });

    return eventStream;
  }

  /**
   * Listen for all contract events
   */
  async listenForAllEvents(callback: (event: any) => void) {
    const eventStream = this.horizonServer
      .effects()
      .forAccount(config.contractId)
      .cursor('now')
      .stream({
        onmessage: (effect) => {
          if (effect.type === 'contract_credited' || effect.type === 'contract_debited') {
            const event = this.parseContractEvent(effect);
            if (event) {
              callback(event);
            }
          }
        },
        onerror: (error) => {
          console.error('Event stream error:', error);
        }
      });

    return eventStream;
  }

  private isShipmentEvent(effect: any, shipmentId: number): boolean {
    // Check if the event relates to the specific shipment
    const event = this.parseContractEvent(effect);
    return event && event.shipmentId === shipmentId;
  }

  private parseContractEvent(effect: any): any {
    try {
      // Parse the contract event from Horizon effect
      // This is a simplified example - actual parsing depends on event structure
      const eventData = effect.data;

      return {
        type: eventData.topic?.[0],
        shipmentId: eventData.data?.[0],
        timestamp: effect.created_at,
        txHash: effect.transaction_hash,
        ...eventData.data
      };
    } catch (error) {
      console.error('Failed to parse contract event:', error);
      return null;
    }
  }
}
````

### Event Processing Service

```typescript
// src/services/event-processor.service.ts
import { EventListenerService } from "./event-listener.service";
import { ShipmentModel } from "../models/shipment.model";

export class EventProcessorService {
  private eventListener: EventListenerService;

  constructor() {
    this.eventListener = new EventListenerService();
  }

  async startProcessing() {
    await this.eventListener.listenForAllEvents(async (event) => {
      try {
        await this.processEvent(event);
      } catch (error) {
        console.error("Failed to process event:", error);
      }
    });
  }

  private async processEvent(event: any) {
    switch (event.type) {
      case "shipment_created":
        await this.handleShipmentCreated(event);
        break;
      case "status_updated":
        await this.handleStatusUpdated(event);
        break;
      case "milestone_recorded":
        await this.handleMilestoneRecorded(event);
        break;
      case "escrow_deposited":
        await this.handleEscrowDeposited(event);
        break;
      case "escrow_released":
        await this.handleEscrowReleased(event);
        break;
      default:
        console.log("Unknown event type:", event.type);
    }
  }

  private async handleShipmentCreated(event: any) {
    // Update MongoDB with new shipment
    await ShipmentModel.create({
      shipmentId: event.shipmentId,
      sender: event.sender,
      receiver: event.receiver,
      dataHash: event.dataHash,
      status: "Created",
      createdAt: new Date(event.timestamp),
      txHash: event.txHash,
    });

    console.log(`Shipment ${event.shipmentId} created`);
  }

  private async handleStatusUpdated(event: any) {
    await ShipmentModel.findOneAndUpdate(
      { shipmentId: event.shipmentId },
      {
        status: event.newStatus,
        dataHash: event.dataHash,
        updatedAt: new Date(event.timestamp),
        lastTxHash: event.txHash,
      },
    );

    console.log(
      `Shipment ${event.shipmentId} status updated to ${event.newStatus}`,
    );
  }

  private async handleMilestoneRecorded(event: any) {
    // Add milestone to shipment record
    await ShipmentModel.findOneAndUpdate(
      { shipmentId: event.shipmentId },
      {
        $push: {
          milestones: {
            checkpoint: event.checkpoint,
            dataHash: event.dataHash,
            timestamp: new Date(event.timestamp),
            reporter: event.reporter,
            txHash: event.txHash,
          },
        },
      },
    );

    console.log(
      `Milestone ${event.checkpoint} recorded for shipment ${event.shipmentId}`,
    );
  }

  private async handleEscrowDeposited(event: any) {
    await ShipmentModel.findOneAndUpdate(
      { shipmentId: event.shipmentId },
      {
        escrowAmount: event.amount,
        escrowTxHash: event.txHash,
      },
    );
  }

  private async handleEscrowReleased(event: any) {
    await ShipmentModel.findOneAndUpdate(
      { shipmentId: event.shipmentId },
      {
        $inc: { escrowAmount: -event.amount },
        releaseTxHash: event.txHash,
      },
    );
  }
}
```

## Transaction Verification

### Verify Transaction Hash and Data Integrity

```typescript
// src/services/verification.service.ts
import { StellarService } from "./stellar.service";
import { ShipmentModel } from "../models/shipment.model";
import { createHash } from "crypto";

export class VerificationService extends StellarService {
  /**
   * Verify transaction hash exists on-chain and compare data hash
   */
  async verifyTransaction(
    txHash: string,
    expectedDataHash: string,
  ): Promise<{
    valid: boolean;
    onChain: boolean;
    dataMatch: boolean;
    details?: any;
  }> {
    try {
      // 1. Get transaction from Stellar network
      const transaction = await this.server.getTransaction(txHash);

      if (!transaction) {
        return {
          valid: false,
          onChain: false,
          dataMatch: false,
        };
      }

      // 2. Extract data hash from transaction
      const onChainDataHash = this.extractDataHashFromTransaction(transaction);

      // 3. Compare hashes
      const dataMatch = onChainDataHash === expectedDataHash;

      return {
        valid: transaction.successful && dataMatch,
        onChain: true,
        dataMatch,
        details: {
          ledger: transaction.ledger,
          createdAt: transaction.created_at,
          fee: transaction.fee_charged,
          onChainDataHash,
          expectedDataHash,
        },
      };
    } catch (error) {
      console.error("Transaction verification failed:", error);
      return {
        valid: false,
        onChain: false,
        dataMatch: false,
      };
    }
  }

  /**
   * Verify shipment data integrity between MongoDB and blockchain
   */
  async verifyShipmentIntegrity(shipmentId: number): Promise<{
    valid: boolean;
    issues: string[];
  }> {
    const issues: string[] = [];

    try {
      // 1. Get shipment from MongoDB
      const dbShipment = await ShipmentModel.findOne({ shipmentId });
      if (!dbShipment) {
        issues.push("Shipment not found in database");
        return { valid: false, issues };
      }

      // 2. Get shipment from blockchain
      const onChainShipment = await this.getShipmentFromChain(shipmentId);
      if (!onChainShipment) {
        issues.push("Shipment not found on blockchain");
        return { valid: false, issues };
      }

      // 3. Compare critical fields
      if (dbShipment.sender !== onChainShipment.sender) {
        issues.push("Sender mismatch");
      }

      if (dbShipment.receiver !== onChainShipment.receiver) {
        issues.push("Receiver mismatch");
      }

      if (dbShipment.status !== onChainShipment.status) {
        issues.push("Status mismatch");
      }

      // 4. Verify data hash
      const computedHash = this.hashOffChainData(dbShipment.fullData);
      if (computedHash !== onChainShipment.dataHash) {
        issues.push("Data hash mismatch - data may be corrupted");
      }

      // 5. Verify transaction hashes
      if (dbShipment.txHash) {
        const txVerification = await this.verifyTransaction(
          dbShipment.txHash,
          dbShipment.dataHash,
        );
        if (!txVerification.valid) {
          issues.push("Creation transaction verification failed");
        }
      }

      return {
        valid: issues.length === 0,
        issues,
      };
    } catch (error) {
      console.error("Integrity verification failed:", error);
      return {
        valid: false,
        issues: ["Verification process failed"],
      };
    }
  }

  private async getShipmentFromChain(shipmentId: number) {
    try {
      const account = await this.getAccount();
      const transaction = new TransactionBuilder(account, {
        fee: BASE_FEE,
        networkPassphrase: config.networkPassphrase,
      })
        .addOperation(
          this.contract.call(
            "get_shipment",
            nativeToScVal(shipmentId, { type: "u64" }),
          ),
        )
        .setTimeout(30)
        .build();

      transaction.sign(this.sourceKeypair);
      const response = await this.server.sendTransaction(transaction);

      if (response.status === "SUCCESS") {
        return scValToNative(response.returnValue);
      }

      return null;
    } catch (error) {
      console.error("Failed to get shipment from chain:", error);
      return null;
    }
  }

  private extractDataHashFromTransaction(transaction: any): string {
    // Extract data hash from transaction operations
    // Implementation depends on transaction structure
    try {
      const operation = transaction.operations[0];
      // Parse the operation to extract data hash
      return operation.parameters?.data_hash || "";
    } catch (error) {
      console.error("Failed to extract data hash:", error);
      return "";
    }
  }

  private hashOffChainData(data: any): string {
    const jsonString = JSON.stringify(data, Object.keys(data).sort());
    return createHash("sha256").update(jsonString).digest("hex");
  }
}
```

## Complete Examples

### Express.js Route Implementation

```typescript
// src/routes/shipments.ts
import { Router } from "express";
import { ShipmentService } from "../services/shipment.service";
import { VerificationService } from "../services/verification.service";

const router = Router();
const shipmentService = new ShipmentService();
const verificationService = new VerificationService();

// Create shipment
router.post("/shipments", async (req, res) => {
  try {
    const {
      sender,
      receiver,
      carrier,
      shipmentData,
      paymentMilestones,
      deadline,
    } = req.body;

    const result = await shipmentService.createShipment({
      sender,
      receiver,
      carrier,
      offChainData: shipmentData,
      paymentMilestones,
      deadline: new Date(deadline),
    });

    res.json({
      success: true,
      shipmentId: result.shipmentId,
      txHash: result.txHash,
      dataHash: result.dataHash,
    });
  } catch (error) {
    res.status(500).json({
      success: false,
      error: error.message,
    });
  }
});

// Update shipment status
router.put("/shipments/:id/status", async (req, res) => {
  try {
    const { id } = req.params;
    const { caller, newStatus, updateData } = req.body;

    const result = await shipmentService.updateShipmentStatus(
      caller,
      parseInt(id),
      newStatus,
      updateData,
    );

    res.json(result);
  } catch (error) {
    res.status(500).json({
      success: false,
      error: error.message,
    });
  }
});

// Verify transaction
router.get("/verify/:txHash", async (req, res) => {
  try {
    const { txHash } = req.params;
    const { expectedDataHash } = req.query;

    const verification = await verificationService.verifyTransaction(
      txHash,
      expectedDataHash as string,
    );

    res.json(verification);
  } catch (error) {
    res.status(500).json({
      success: false,
      error: error.message,
    });
  }
});

export default router;
```

### MongoDB Schema

```typescript
// src/models/shipment.model.ts
import mongoose from "mongoose";

const milestoneSchema = new mongoose.Schema({
  checkpoint: String,
  dataHash: String,
  timestamp: Date,
  reporter: String,
  txHash: String,
  gpsCoordinates: {
    latitude: Number,
    longitude: Number,
  },
  sensorData: {
    temperature: Number,
    humidity: Number,
    pressure: Number,
  },
});

const shipmentSchema = new mongoose.Schema({
  shipmentId: { type: Number, unique: true, required: true },
  sender: { type: String, required: true },
  receiver: { type: String, required: true },
  carrier: { type: String, required: true },
  status: { type: String, required: true },
  dataHash: { type: String, required: true },
  txHash: String,
  createdAt: Date,
  updatedAt: Date,
  deadline: Date,
  escrowAmount: Number,
  milestones: [milestoneSchema],
  fullData: {
    description: String,
    weight: Number,
    dimensions: {
      length: Number,
      width: Number,
      height: Number,
    },
    specialInstructions: String,
    photos: [String],
    documents: [String],
  },
});

export const ShipmentModel = mongoose.model("Shipment", shipmentSchema);
```

### Environment Variables

```bash
# .env
NODE_ENV=development
STELLAR_SECRET_KEY=SXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
SHIPMENT_CONTRACT_ID=CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
TOKEN_CONTRACT_ID=CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
MONGODB_URI=mongodb://localhost:27017/navin
```

## Best Practices

1. **Error Handling**: Always wrap Stellar operations in try-catch blocks
2. **Rate Limiting**: Implement rate limiting for contract calls to avoid hitting network limits
3. **Data Validation**: Validate all input data before creating hashes or submitting transactions
4. **Event Deduplication**: Handle duplicate events that may occur during network issues
5. **Transaction Fees**: Monitor and adjust transaction fees based on network conditions
6. **Security**: Never expose private keys in client-side code or logs


## Error Code Mapping

Every contract invocation that fails returns a Soroban `ContractError` whose numeric code maps to a `NavinError` variant. The authoritative mapping — including user-facing category and retry guidance — lives in
[`contracts/shipment/src/error_map.rs`](../contracts/shipment/src/error_map.rs).

### Categories

| Category | Meaning |
|---|---|
| `InvalidInput` | Caller supplied bad data; fix the request before resubmitting. |
| `Unauthorized` | Missing role or signature; check auth setup. |
| `NotFound` | Referenced resource does not exist. |
| `InvalidState` | Operation not allowed in the current shipment state. |
| `LimitExceeded` | A resource cap or rate limit was hit. |
| `Transient` | Infrastructure or arithmetic failure; may resolve on retry. |
| `Configuration` | Contract initialisation or config problem. |

### Retry Guidance

| Guidance | Action |
|---|---|
| `NoRetry` | Do not retry; the request must be corrected first. |
| `RetryAfterDelay` | Retry after a short back-off (transient / rate-limit). |
| `RetryAfterStateChange` | Retry only after the relevant on-chain state changes. |

### Quick Reference Table

| Code | Variant | Category | Retry |
|---|---|---|---|
| 1 | `AlreadyInitialized` | Configuration | NoRetry |
| 2 | `NotInitialized` | Configuration | NoRetry |
| 3 | `Unauthorized` | Unauthorized | NoRetry |
| 4 | `ShipmentNotFound` | NotFound | NoRetry |
| 5 | `InvalidStatus` | InvalidState | RetryAfterStateChange |
| 6 | `InvalidHash` | InvalidInput | NoRetry |
| 7 | `EscrowLocked` | InvalidState | RetryAfterStateChange |
| 8 | `InsufficientFunds` | InvalidInput | NoRetry |
| 9 | `ShipmentAlreadyCompleted` | InvalidState | NoRetry |
| 10 | `InvalidTimestamp` | InvalidInput | NoRetry |
| 11 | `CounterOverflow` | Transient | NoRetry |
| 14 | `InvalidAmount` | InvalidInput | NoRetry |
| 15 | `EscrowAlreadyDeposited` | InvalidState | NoRetry |
| 16 | `BatchTooLarge` | LimitExceeded | NoRetry |
| 17 | `InvalidShipmentInput` | InvalidInput | NoRetry |
| 18 | `MilestoneSumInvalid` | InvalidInput | NoRetry |
| 19 | `MilestoneAlreadyPaid` | InvalidState | NoRetry |
| 20 | `MetadataLimitExceeded` | LimitExceeded | NoRetry |
| 21 | `RateLimitExceeded` | LimitExceeded | RetryAfterDelay |
| 22 | `ProposalNotFound` | NotFound | NoRetry |
| 23 | `ProposalAlreadyExecuted` | InvalidState | NoRetry |
| 24 | `ProposalExpired` | InvalidState | NoRetry |
| 25 | `AlreadyApproved` | InvalidState | NoRetry |
| 26 | `InsufficientApprovals` | InvalidState | RetryAfterStateChange |
| 27 | `NotAnAdmin` | Unauthorized | NoRetry |
| 28 | `InvalidMultiSigConfig` | InvalidInput | NoRetry |
| 29 | `NotExpired` | InvalidState | RetryAfterStateChange |
| 30 | `ShipmentLimitReached` | LimitExceeded | RetryAfterStateChange |
| 31 | `InvalidConfig` | InvalidInput | NoRetry |
| 32 | `CannotSelfRevoke` | InvalidInput | NoRetry |
| 33 | `CarrierSuspended` | Unauthorized | RetryAfterStateChange |
| 34 | `ForceCancelReasonHashMissing` | InvalidInput | NoRetry |
| 35 | `ArithmeticError` | Transient | NoRetry |
| 36 | `DisputeReasonHashMissing` | InvalidInput | NoRetry |
| 37 | `CompanySuspended` | Unauthorized | RetryAfterStateChange |
| 38 | `ShipmentFinalized` | InvalidState | NoRetry |
| 39 | `TokenTransferFailed` | Transient | RetryAfterDelay |
| 40 | `TokenMintFailed` | Transient | RetryAfterDelay |
| 41 | `DuplicateAction` | InvalidInput | NoRetry |
| 42 | `ShipmentUnavailable` | InvalidState | RetryAfterStateChange |
| 43 | `ContractPaused` | InvalidState | RetryAfterStateChange |
| 44 | `StatusHashNotFound` | NotFound | NoRetry |
| 45 | `DataHashMismatch` | InvalidInput | NoRetry |
| 46 | `CircuitBreakerOpen` | Transient | RetryAfterDelay |
| 47 | `InvalidMigrationEdge` | InvalidInput | NoRetry |
| 48 | `MilestoneLimitExceeded` | LimitExceeded | NoRetry |
| 49 | `NoteLimitExceeded` | LimitExceeded | NoRetry |
| 50 | `EvidenceLimitExceeded` | LimitExceeded | NoRetry |
| 51 | `BreachLimitExceeded` | LimitExceeded | NoRetry |
| 52 | `InvalidTokenDecimals` | InvalidInput | NoRetry |

### TypeScript Usage

```typescript
const ERROR_MAP: Record<number, { category: string; retry: string; message: string }> = {
  1:  { category: "Configuration",  retry: "NoRetry",               message: "Contract is already initialised; call init only once." },
  2:  { category: "Configuration",  retry: "NoRetry",               message: "Contract has not been initialised; call init first." },
  3:  { category: "Unauthorized",   retry: "NoRetry",               message: "Caller does not hold the required role or signature." },
  4:  { category: "NotFound",       retry: "NoRetry",               message: "Shipment ID does not exist." },
  5:  { category: "InvalidState",   retry: "RetryAfterStateChange", message: "State transition not allowed from current status." },
  6:  { category: "InvalidInput",   retry: "NoRetry",               message: "Provided data hash does not match the stored value." },
  7:  { category: "InvalidState",   retry: "RetryAfterStateChange", message: "Escrow is locked; wait for terminal state." },
  8:  { category: "InvalidInput",   retry: "NoRetry",               message: "Caller balance too low for escrow deposit." },
  9:  { category: "InvalidState",   retry: "NoRetry",               message: "Shipment already in a terminal state." },
  21: { category: "LimitExceeded",  retry: "RetryAfterDelay",       message: "Rate limit hit; retry after the interval elapses." },
  39: { category: "Transient",      retry: "RetryAfterDelay",       message: "Token transfer failed; retry after verifying token state." },
  43: { category: "InvalidState",   retry: "RetryAfterStateChange", message: "Contract is paused; wait for operator to resume." },
  46: { category: "Transient",      retry: "RetryAfterDelay",       message: "Circuit breaker open; token transfers temporarily disabled." },
  // … see error_map.rs for the full list
};

function handleContractError(code: number): void {
  const info = ERROR_MAP[code];
  if (!info) {
    console.error(`Unknown contract error code: ${code}`);
    return;
  }
  console.error(`[${info.category}] ${info.message}`);
  if (info.retry === "RetryAfterDelay") {
    scheduleRetry();
  } else if (info.retry === "RetryAfterStateChange") {
    waitForStateChange();
  }
  // NoRetry → surface error to the user
}
```

## Testing

```typescript
// src/tests/shipment.test.ts
import { ShipmentService } from "../services/shipment.service";
import { VerificationService } from "../services/verification.service";

describe("Shipment Integration", () => {
  let shipmentService: ShipmentService;
  let verificationService: VerificationService;

  beforeEach(() => {
    shipmentService = new ShipmentService();
    verificationService = new VerificationService();
  });

  it("should create shipment and verify transaction", async () => {
    const shipmentData = {
      sender: "GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX",
      receiver: "GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX",
      carrier: "GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX",
      offChainData: {
        description: "Test shipment",
        weight: 10.5,
        specialInstructions: "Handle with care",
      },
      paymentMilestones: [
        { checkpoint: "pickup", percentage: 30 },
        { checkpoint: "delivery", percentage: 70 },
      ],
      deadline: new Date(Date.now() + 7 * 24 * 60 * 60 * 1000), // 7 days
    };

    const result = await shipmentService.createShipment(shipmentData);
    expect(result.success).toBe(true);
    expect(result.shipmentId).toBeGreaterThan(0);
    expect(result.txHash).toBeDefined();

    // Verify the transaction
    const verification = await verificationService.verifyTransaction(
      result.txHash,
      result.dataHash,
    );
    expect(verification.valid).toBe(true);
    expect(verification.onChain).toBe(true);
    expect(verification.dataMatch).toBe(true);
  });
});
```

This integration guide provides complete TypeScript examples for interacting with the Navin shipment contract, including contract invocation, event listening, and transaction verification patterns that your Express backend can use.

1. [Architecture Overview](#architecture-overview)
2. [Setup & Configuration](#setup--configuration)
3. [Contract Invocation](#contract-invocation)
4. [Immutable Provenance Queries](#immutable-provenance-queries)
5. [Event Listening](#event-listening)
6. [Transaction Verification](#transaction-verification)
7. [Complete Examples](#complete-examples)

## Architecture Overview

Navin uses a **Hash-and-Emit** architecture:

- **On-chain**: Contract stores only critical data (shipment IDs, addresses, status, escrow amounts) and emits events with data hashes
- **Off-chain**: Backend (MongoDB) stores full shipment details (GPS coordinates, sensor readings, photos, metadata)
- **Verification**: Data integrity is verified by comparing on-chain hashes with off-chain data hashes

```
┌─────────────────┐         ┌──────────────────┐         ┌─────────────────┐
│   Frontend      │────────▶│  Express Backend │────────▶│ Stellar Network │
│   (React)       │         │   (Indexer)      │         │  (Soroban)      │
└─────────────────┘         └──────────────────┘         └─────────────────┘
                                     │                            │
                                     │                            │
                                     ▼                            ▼
                            ┌──────────────────┐         ┌─────────────────┐
                            │    MongoDB       │         │  Event Stream   │
                            │  (Full Data)     │◀────────│  (Horizon)      │
                            └──────────────────┘         └─────────────────┘
```

## Setup & Configuration

### Installation

```bash
npm install @stellar/stellar-sdk
# or
yarn add @stellar/stellar-sdk
```

### Environment Configuration

Create a configuration file for network settings:

```typescript
// src/config/stellar.config.ts
```

export interface StellarConfig {
networkPassphrase: string;
rpcUrl: string;
horizonUrl: string;
contractId: string;
sourceSecretKey: string;
}

export const testnetConfig: StellarConfig = {
networkPassphrase: 'Test SDF Network ; September 2015',
rpcUrl: 'https://soroban-testnet.stellar.org:443',
horizonUrl: 'https://horizon-testnet.stellar.org',
contractId: process.env.SHIPMENT_CONTRACT_ID!,
sourceSecretKey: process.env.STELLAR_SECRET_KEY!,
};

export const mainnetConfig: StellarConfig = {
networkPassphrase: 'Public Global Stellar Network ; September 2015',
rpcUrl: 'https://soroban-rpc.stellar.org:443',
horizonUrl: 'https://horizon.stellar.org',
contractId: process.env.SHIPMENT_CONTRACT_ID!,
sourceSecretKey: process.env.STELLAR_SECRET_KEY!,
};

// Select config based on environment
export const config = process.env.NODE_ENV === 'production'
? mainnetConfig
: testnetConfig;

````

### Initialize Stellar SDK

```typescript
// src/services/stellar.service.ts
import {
  SorobanRpc,
  Keypair,
  Contract,
  TransactionBuilder,
  Networks,
  Operation,
  BASE_FEE,
  Address,
  xdr,
  scValToNative,
  nativeToScVal,
} from '@stellar/stellar-sdk';
import { config } from '../config/stellar.config';

export class StellarService {
  private server: SorobanRpc.Server;
  private sourceKeypair: Keypair;
  private contract: Contract;

  constructor() {
    this.server = new SorobanRpc.Server(config.rpcUrl);
    this.sourceKeypair = Keypair.fromSecret(config.sourceSecretKey);
    this.contract = new Contract(config.contractId);
  }

  async getAccount() {
    return await this.server.getAccount(this.sourceKeypair.publicKey());
  }
}
````

## Contract Invocation

### Example 1: Create a Shipment

```typescript
// src/services/shipment.service.ts
```

import { StellarService } from './stellar.service';
import { createHash } from 'crypto';

export class ShipmentService extends StellarService {

/\*\*

- Create a new shipment on-chain
  \*/
  async createShipment(shipmentData: {
  sender: string;
  receiver: string;
  carrier: string;
  offChainData: any;
  paymentMilestones: Array<{ checkpoint: string; percentage: number }>;
  deadline: Date;
  }) {
  try {
  // 1. Hash the off-chain data
  const dataHash = this.hashOffChainData(shipmentData.offChainData);

      // 2. Prepare contract arguments
      const milestones = shipmentData.paymentMilestones.map(m => [
        m.checkpoint,
        m.percentage
      ]);

      // 3. Build transaction
      const account = await this.getAccount();
      const transaction = new TransactionBuilder(account, {
        fee: BASE_FEE,
        networkPassphrase: config.networkPassphrase,
      })
        .addOperation(
          this.contract.call(
            'create_shipment',
            Address.fromString(shipmentData.sender),
            Address.fromString(shipmentData.receiver),
            Address.fromString(shipmentData.carrier),
            nativeToScVal(Buffer.from(dataHash, 'hex'), { type: 'bytes' }),
            nativeToScVal(milestones, { type: 'vec' }),
            nativeToScVal(Math.floor(shipmentData.deadline.getTime() / 1000), { type: 'u64' })
          )
        )
        .setTimeout(30)
        .build();

      // 4. Sign and submit
      transaction.sign(this.sourceKeypair);
      const response = await this.server.sendTransaction(transaction);

      // 5. Wait for confirmation
      if (response.status === 'PENDING') {
        const result = await this.server.getTransaction(response.hash);
        return {
          success: true,
          txHash: response.hash,
          shipmentId: this.extractShipmentIdFromResult(result),
          dataHash
        };
      }

      throw new Error(`Transaction failed: ${response.status}`);

  } catch (error) {
  console.error('Failed to create shipment:', error);
  throw error;
  }
  }

/\*\*

- Update shipment status
  \*/
  async updateShipmentStatus(
  caller: string,
  shipmentId: number,
  newStatus: string,
  offChainData: any
  ) {
  const dataHash = this.hashOffChainData(offChainData);


    const account = await this.getAccount();
    const transaction = new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: config.networkPassphrase,
    })
      .addOperation(
        this.contract.call(
          'update_status',
          Address.fromString(caller),
          nativeToScVal(shipmentId, { type: 'u64' }),
          nativeToScVal(newStatus, { type: 'symbol' }),
          nativeToScVal(Buffer.from(dataHash, 'hex'), { type: 'bytes' })
        )
      )
      .setTimeout(30)
      .build();

    transaction.sign(this.sourceKeypair);
    const response = await this.server.sendTransaction(transaction);

    return {
      success: response.status === 'SUCCESS',
      txHash: response.hash,
      dataHash
    };

}

/\*\*

- Record milestone for shipment
  \*/
  async recordMilestone(
  carrier: string,
  shipmentId: number,
  checkpoint: string,
  offChainData: any
  ) {
  const dataHash = this.hashOffChainData(offChainData);


    const account = await this.getAccount();
    const transaction = new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: config.networkPassphrase,
    })
      .addOperation(
        this.contract.call(
          'record_milestone',
          Address.fromString(carrier),
          nativeToScVal(shipmentId, { type: 'u64' }),
          nativeToScVal(checkpoint, { type: 'symbol' }),
          nativeToScVal(Buffer.from(dataHash, 'hex'), { type: 'bytes' })
        )
      )
      .setTimeout(30)
      .build();

    transaction.sign(this.sourceKeypair);
    const response = await this.server.sendTransaction(transaction);

    return {
      success: response.status === 'SUCCESS',
      txHash: response.hash,
      dataHash
    };

}

private hashOffChainData(data: any): string {
const jsonString = JSON.stringify(data, Object.keys(data).sort());
return createHash('sha256').update(jsonString).digest('hex');
}

private extractShipmentIdFromResult(result: any): number {
// Extract shipment ID from transaction result
// Implementation depends on Stellar SDK response format
return scValToNative(result.returnValue);
}
}

````

## Immutable Provenance Queries

Use these read-only methods to render verification views without fetching the full shipment payload.

- `get_shipment_creator(shipment_id: u64) -> Address`: Returns the immutable creator (sender) address captured at shipment creation.
- `get_shipment_receiver(shipment_id: u64) -> Address`: Returns the immutable receiver address captured at shipment creation.

Both methods fail with `ShipmentNotFound` for unknown IDs, which helps frontend verification flows distinguish missing records from mismatched identities.

### Restore Triage Query

For pre-restore operations, call:

- `get_restore_diagnostics(shipment_id: u64) -> PersistentRestoreDiagnostics`

Interpretation guidance:

- `ActivePersistent`: no restore path needed for the primary shipment payload.
- `ArchivedExpected`: shipment has been archived and may require restore flow depending on operator policy.
- `Missing`: shipment ID is absent from both persistent and archived paths.
- `InconsistentDualPresence`: both paths are populated; investigate before any restore mutation.

This method is read-only and does not mutate contract state.

```typescript
// src/services/shipment-query.service.ts
import {
  Contract,
  TransactionBuilder,
  BASE_FEE,
  Address,
  nativeToScVal,
} from '@stellar/stellar-sdk';
import { StellarService } from './stellar.service';
import { config } from '../config/stellar.config';

export class ShipmentQueryService extends StellarService {
  async getShipmentCreator(shipmentId: number): Promise<string> {
    const contract = new Contract(config.contractId);
    const account = await this.getAccount();
    const tx = new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: config.networkPassphrase,
    })
      .addOperation(
        contract.call(
          'get_shipment_creator',
          nativeToScVal(shipmentId, { type: 'u64' })
        )
      )
      .setTimeout(30)
      .build();

    const sim = await this.server.simulateTransaction(tx);
    return Address.fromScVal(sim.result!.retval).toString();
  }

  async getShipmentReceiver(shipmentId: number): Promise<string> {
    const contract = new Contract(config.contractId);
    const account = await this.getAccount();
    const tx = new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: config.networkPassphrase,
    })
      .addOperation(
        contract.call(
          'get_shipment_receiver',
          nativeToScVal(shipmentId, { type: 'u64' })
        )
      )
      .setTimeout(30)
      .build();

    const sim = await this.server.simulateTransaction(tx);
    return Address.fromScVal(sim.result!.retval).toString();
  }
}
```

## Event Listening

### Horizon Event Stream Listener

```typescript
// src/services/event-listener.service.ts
import { Server } from '@stellar/stellar-sdk/lib/horizon';
import { config } from '../config/stellar.config';

export class EventListenerService {
  private horizonServer: Server;

  constructor() {
    this.horizonServer = new Server(config.horizonUrl);
  }

  /**
   * Listen for contract events for a specific shipment
   */
  async listenForShipmentEvents(shipmentId: number, callback: (event: any) => void) {
    const eventStream = this.horizonServer
      .effects()
      .forAccount(config.contractId)
      .cursor('now')
      .stream({
        onmessage: (effect) => {
          if (this.isShipmentEvent(effect, shipmentId)) {
            callback(this.parseContractEvent(effect));
          }
        },
        onerror: (error) => {
          console.error('Event stream error:', error);
        }
      });

    return eventStream;
  }

  /**
   * Listen for all contract events
   */
  async listenForAllEvents(callback: (event: any) => void) {
    const eventStream = this.horizonServer
      .effects()
      .forAccount(config.contractId)
      .cursor('now')
      .stream({
        onmessage: (effect) => {
          if (effect.type === 'contract_credited' || effect.type === 'contract_debited') {
            const event = this.parseContractEvent(effect);
            if (event) {
              callback(event);
            }
          }
        },
        onerror: (error) => {
          console.error('Event stream error:', error);
        }
      });

    return eventStream;
  }

  private isShipmentEvent(effect: any, shipmentId: number): boolean {
    // Check if the event relates to the specific shipment
    const event = this.parseContractEvent(effect);
    return event && event.shipmentId === shipmentId;
  }

  private parseContractEvent(effect: any): any {
    try {
      // Parse the contract event from Horizon effect
      // This is a simplified example - actual parsing depends on event structure
      const eventData = effect.data;

      return {
        type: eventData.topic?.[0],
        shipmentId: eventData.data?.[0],
        timestamp: effect.created_at,
        txHash: effect.transaction_hash,
        ...eventData.data
      };
    } catch (error) {
      console.error('Failed to parse contract event:', error);
      return null;
    }
  }
}
````

### Event Processing Service

```typescript
// src/services/event-processor.service.ts
import { EventListenerService } from "./event-listener.service";
import { ShipmentModel } from "../models/shipment.model";

export class EventProcessorService {
  private eventListener: EventListenerService;

  constructor() {
    this.eventListener = new EventListenerService();
  }

  async startProcessing() {
    await this.eventListener.listenForAllEvents(async (event) => {
      try {
        await this.processEvent(event);
      } catch (error) {
        console.error("Failed to process event:", error);
      }
    });
  }

  private async processEvent(event: any) {
    switch (event.type) {
      case "shipment_created":
        await this.handleShipmentCreated(event);
        break;
      case "status_updated":
        await this.handleStatusUpdated(event);
        break;
      case "milestone_recorded":
        await this.handleMilestoneRecorded(event);
        break;
      case "escrow_deposited":
        await this.handleEscrowDeposited(event);
        break;
      case "escrow_released":
        await this.handleEscrowReleased(event);
        break;
      default:
        console.log("Unknown event type:", event.type);
    }
  }

  private async handleShipmentCreated(event: any) {
    // Update MongoDB with new shipment
    await ShipmentModel.create({
      shipmentId: event.shipmentId,
      sender: event.sender,
      receiver: event.receiver,
      dataHash: event.dataHash,
      status: "Created",
      createdAt: new Date(event.timestamp),
      txHash: event.txHash,
    });

    console.log(`Shipment ${event.shipmentId} created`);
  }

  private async handleStatusUpdated(event: any) {
    await ShipmentModel.findOneAndUpdate(
      { shipmentId: event.shipmentId },
      {
        status: event.newStatus,
        dataHash: event.dataHash,
        updatedAt: new Date(event.timestamp),
        lastTxHash: event.txHash,
      },
    );

    console.log(
      `Shipment ${event.shipmentId} status updated to ${event.newStatus}`,
    );
  }

  private async handleMilestoneRecorded(event: any) {
    // Add milestone to shipment record
    await ShipmentModel.findOneAndUpdate(
      { shipmentId: event.shipmentId },
      {
        $push: {
          milestones: {
            checkpoint: event.checkpoint,
            dataHash: event.dataHash,
            timestamp: new Date(event.timestamp),
            reporter: event.reporter,
            txHash: event.txHash,
          },
        },
      },
    );

    console.log(
      `Milestone ${event.checkpoint} recorded for shipment ${event.shipmentId}`,
    );
  }

  private async handleEscrowDeposited(event: any) {
    await ShipmentModel.findOneAndUpdate(
      { shipmentId: event.shipmentId },
      {
        escrowAmount: event.amount,
        escrowTxHash: event.txHash,
      },
    );
  }

  private async handleEscrowReleased(event: any) {
    await ShipmentModel.findOneAndUpdate(
      { shipmentId: event.shipmentId },
      {
        $inc: { escrowAmount: -event.amount },
        releaseTxHash: event.txHash,
      },
    );
  }
}
```

## Transaction Verification

### Verify Transaction Hash and Data Integrity

```typescript
// src/services/verification.service.ts
import { StellarService } from "./stellar.service";
import { ShipmentModel } from "../models/shipment.model";
import { createHash } from "crypto";

export class VerificationService extends StellarService {
  /**
   * Verify transaction hash exists on-chain and compare data hash
   */
  async verifyTransaction(
    txHash: string,
    expectedDataHash: string,
  ): Promise<{
    valid: boolean;
    onChain: boolean;
    dataMatch: boolean;
    details?: any;
  }> {
    try {
      // 1. Get transaction from Stellar network
      const transaction = await this.server.getTransaction(txHash);

      if (!transaction) {
        return {
          valid: false,
          onChain: false,
          dataMatch: false,
        };
      }

      // 2. Extract data hash from transaction
      const onChainDataHash = this.extractDataHashFromTransaction(transaction);

      // 3. Compare hashes
      const dataMatch = onChainDataHash === expectedDataHash;

      return {
        valid: transaction.successful && dataMatch,
        onChain: true,
        dataMatch,
        details: {
          ledger: transaction.ledger,
          createdAt: transaction.created_at,
          fee: transaction.fee_charged,
          onChainDataHash,
          expectedDataHash,
        },
      };
    } catch (error) {
      console.error("Transaction verification failed:", error);
      return {
        valid: false,
        onChain: false,
        dataMatch: false,
      };
    }
  }

  /**
   * Verify shipment data integrity between MongoDB and blockchain
   */
  async verifyShipmentIntegrity(shipmentId: number): Promise<{
    valid: boolean;
    issues: string[];
  }> {
    const issues: string[] = [];

    try {
      // 1. Get shipment from MongoDB
      const dbShipment = await ShipmentModel.findOne({ shipmentId });
      if (!dbShipment) {
        issues.push("Shipment not found in database");
        return { valid: false, issues };
      }

      // 2. Get shipment from blockchain
      const onChainShipment = await this.getShipmentFromChain(shipmentId);
      if (!onChainShipment) {
        issues.push("Shipment not found on blockchain");
        return { valid: false, issues };
      }

      // 3. Compare critical fields
      if (dbShipment.sender !== onChainShipment.sender) {
        issues.push("Sender mismatch");
      }

      if (dbShipment.receiver !== onChainShipment.receiver) {
        issues.push("Receiver mismatch");
      }

      if (dbShipment.status !== onChainShipment.status) {
        issues.push("Status mismatch");
      }

      // 4. Verify data hash
      const computedHash = this.hashOffChainData(dbShipment.fullData);
      if (computedHash !== onChainShipment.dataHash) {
        issues.push("Data hash mismatch - data may be corrupted");
      }

      // 5. Verify transaction hashes
      if (dbShipment.txHash) {
        const txVerification = await this.verifyTransaction(
          dbShipment.txHash,
          dbShipment.dataHash,
        );
        if (!txVerification.valid) {
          issues.push("Creation transaction verification failed");
        }
      }

      return {
        valid: issues.length === 0,
        issues,
      };
    } catch (error) {
      console.error("Integrity verification failed:", error);
      return {
        valid: false,
        issues: ["Verification process failed"],
      };
    }
  }

  private async getShipmentFromChain(shipmentId: number) {
    try {
      const account = await this.getAccount();
      const transaction = new TransactionBuilder(account, {
        fee: BASE_FEE,
        networkPassphrase: config.networkPassphrase,
      })
        .addOperation(
          this.contract.call(
            "get_shipment",
            nativeToScVal(shipmentId, { type: "u64" }),
          ),
        )
        .setTimeout(30)
        .build();

      transaction.sign(this.sourceKeypair);
      const response = await this.server.sendTransaction(transaction);

      if (response.status === "SUCCESS") {
        return scValToNative(response.returnValue);
      }

      return null;
    } catch (error) {
      console.error("Failed to get shipment from chain:", error);
      return null;
    }
  }

  private extractDataHashFromTransaction(transaction: any): string {
    // Extract data hash from transaction operations
    // Implementation depends on transaction structure
    try {
      const operation = transaction.operations[0];
      // Parse the operation to extract data hash
      return operation.parameters?.data_hash || "";
    } catch (error) {
      console.error("Failed to extract data hash:", error);
      return "";
    }
  }

  private hashOffChainData(data: any): string {
    const jsonString = JSON.stringify(data, Object.keys(data).sort());
    return createHash("sha256").update(jsonString).digest("hex");
  }
}
```

## Complete Examples

### Express.js Route Implementation

```typescript
// src/routes/shipments.ts
import { Router } from "express";
import { ShipmentService } from "../services/shipment.service";
import { VerificationService } from "../services/verification.service";

const router = Router();
const shipmentService = new ShipmentService();
const verificationService = new VerificationService();

// Create shipment
router.post("/shipments", async (req, res) => {
  try {
    const {
      sender,
      receiver,
      carrier,
      shipmentData,
      paymentMilestones,
      deadline,
    } = req.body;

    const result = await shipmentService.createShipment({
      sender,
      receiver,
      carrier,
      offChainData: shipmentData,
      paymentMilestones,
      deadline: new Date(deadline),
    });

    res.json({
      success: true,
      shipmentId: result.shipmentId,
      txHash: result.txHash,
      dataHash: result.dataHash,
    });
  } catch (error) {
    res.status(500).json({
      success: false,
      error: error.message,
    });
  }
});

// Update shipment status
router.put("/shipments/:id/status", async (req, res) => {
  try {
    const { id } = req.params;
    const { caller, newStatus, updateData } = req.body;

    const result = await shipmentService.updateShipmentStatus(
      caller,
      parseInt(id),
      newStatus,
      updateData,
    );

    res.json(result);
  } catch (error) {
    res.status(500).json({
      success: false,
      error: error.message,
    });
  }
});

// Verify transaction
router.get("/verify/:txHash", async (req, res) => {
  try {
    const { txHash } = req.params;
    const { expectedDataHash } = req.query;

    const verification = await verificationService.verifyTransaction(
      txHash,
      expectedDataHash as string,
    );

    res.json(verification);
  } catch (error) {
    res.status(500).json({
      success: false,
      error: error.message,
    });
  }
});

export default router;
```

### MongoDB Schema

```typescript
// src/models/shipment.model.ts
import mongoose from "mongoose";

const milestoneSchema = new mongoose.Schema({
  checkpoint: String,
  dataHash: String,
  timestamp: Date,
  reporter: String,
  txHash: String,
  gpsCoordinates: {
    latitude: Number,
    longitude: Number,
  },
  sensorData: {
    temperature: Number,
    humidity: Number,
    pressure: Number,
  },
});

const shipmentSchema = new mongoose.Schema({
  shipmentId: { type: Number, unique: true, required: true },
  sender: { type: String, required: true },
  receiver: { type: String, required: true },
  carrier: { type: String, required: true },
  status: { type: String, required: true },
  dataHash: { type: String, required: true },
  txHash: String,
  createdAt: Date,
  updatedAt: Date,
  deadline: Date,
  escrowAmount: Number,
  milestones: [milestoneSchema],
  fullData: {
    description: String,
    weight: Number,
    dimensions: {
      length: Number,
      width: Number,
      height: Number,
    },
    specialInstructions: String,
    photos: [String],
    documents: [String],
  },
});

export const ShipmentModel = mongoose.model("Shipment", shipmentSchema);
```

### Environment Variables

```bash
# .env
NODE_ENV=development
STELLAR_SECRET_KEY=SXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
SHIPMENT_CONTRACT_ID=CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
TOKEN_CONTRACT_ID=CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
MONGODB_URI=mongodb://localhost:27017/navin
```

## Best Practices

1. **Error Handling**: Always wrap Stellar operations in try-catch blocks
2. **Rate Limiting**: Implement rate limiting for contract calls to avoid hitting network limits
3. **Data Validation**: Validate all input data before creating hashes or submitting transactions
4. **Event Deduplication**: Handle duplicate events that may occur during network issues
5. **Transaction Fees**: Monitor and adjust transaction fees based on network conditions
6. **Security**: Never expose private keys in client-side code or logs

## Testing

```typescript
// src/tests/shipment.test.ts
import { ShipmentService } from "../services/shipment.service";
import { VerificationService } from "../services/verification.service";

describe("Shipment Integration", () => {
  let shipmentService: ShipmentService;
  let verificationService: VerificationService;

  beforeEach(() => {
    shipmentService = new ShipmentService();
    verificationService = new VerificationService();
  });

  it("should create shipment and verify transaction", async () => {
    const shipmentData = {
      sender: "GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX",
      receiver: "GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX",
      carrier: "GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX",
      offChainData: {
        description: "Test shipment",
        weight: 10.5,
        specialInstructions: "Handle with care",
      },
      paymentMilestones: [
        { checkpoint: "pickup", percentage: 30 },
        { checkpoint: "delivery", percentage: 70 },
      ],
      deadline: new Date(Date.now() + 7 * 24 * 60 * 60 * 1000), // 7 days
    };

    const result = await shipmentService.createShipment(shipmentData);
    expect(result.success).toBe(true);
    expect(result.shipmentId).toBeGreaterThan(0);
    expect(result.txHash).toBeDefined();

    // Verify the transaction
    const verification = await verificationService.verifyTransaction(
      result.txHash,
      result.dataHash,
    );
    expect(verification.valid).toBe(true);
    expect(verification.onChain).toBe(true);
    expect(verification.dataMatch).toBe(true);
  });
});
```

This integration guide provides complete TypeScript examples for interacting with the Navin shipment contract, including contract invocation, event listening, and transaction verification patterns that your Express backend can use.
