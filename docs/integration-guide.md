# Navin Contract Integration Guide

Complete guide for integrating the Navin shipment tracking smart contract with your Express.js backend using the Stellar JavaScript/TypeScript SDK.

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Setup & Configuration](#setup--configuration)
3. [Contract Invocation](#contract-invocation)
4. [Event Listening](#event-listening)
5. [Transaction Verification](#transaction-verification)
6. [Complete Examples](#complete-examples)

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
