# Navin - Decentralized Delivery Tracking Platform

[![CI](https://github.com/Navin-xmr/navin-contracts/actions/workflows/test.yml/badge.svg)](https://github.com/Navin-xmr/navin-contracts/actions)

**Navin** is a decentralized delivery tracking platform built on the Stellar blockchain. It empowers corporations and logistics companies to track foodstuff and other deliverable items in real-time with complete transparency and security.

## Overview

In today's supply chain ecosystem, transparency and accountability are critical. Navin leverages Stellar's fast, low-cost blockchain infrastructure to provide:

- **Real-time Tracking**: Monitor deliveries from origin to destination
- **Transparent Operations**: All stakeholders can verify delivery status on-chain
- **Secure Data**: Cryptographically secured delivery records and proof of custody
- **Cost-Effective**: Built on Stellar's efficient blockchain infrastructure
- **Scalable**: Designed to handle high-volume delivery operations

## Use Cases

- Food delivery tracking and verification
- Pharmaceutical supply chain management
- Perishable goods monitoring
- Multi-party logistics coordination
- Proof of delivery and custody chain

## Project Structure

This repository contains Soroban smart contracts for the Navin platform:

```text
.
├── contracts
│   ├── nft                 # Shipment-asset NFT contract
│   │   ├── src
│   │   │   ├── lib.rs      # Minting, transfer, burn, and metadata entrypoints
│   │   │   ├── storage.rs  # Ownership and metadata URI storage
│   │   │   ├── errors.rs   # Contract error enum
│   │   │   └── event_topics.rs # Event topic constants
│   │   └── Cargo.toml
│   ├── shipment            # Core logistics and escrow contract
│   │   ├── src             # Entry points plus the test_*.rs regression suites
│   │   ├── tests           # Cross-contract integration tests
│   │   ├── docs            # Contract-specific reference documents
│   │   └── Cargo.toml
│   └── token               # Payment token contract
│       ├── src
│       │   ├── lib.rs      # Token entrypoints
│       │   ├── storage.rs  # Balance and allowance storage
│       │   ├── types.rs    # Token types
│       │   └── test.rs     # Token tests
│       └── Cargo.toml
├── docs                    # Architecture, ABI, event, and operations guides
├── scripts                 # Build, deploy, simulate, and release scripts
├── Cargo.toml              # Workspace configuration
├── Makefile                # Build and test commands
├── CONTRIBUTING.md         # Contribution guidelines
└── README.md
```

## Smart Contracts

The repository contains three smart contracts located under `contracts/`:

- **`contracts/shipment`**: Core logistics and escrow contract managing the shipment lifecycle, delivery tracking, escrow, and role-based access control. See the contract entry points in [`contracts/shipment/src/lib.rs`](contracts/shipment/src/lib.rs), the regression suites in [`contracts/shipment/src/`](contracts/shipment/src/), the cross-contract integration test in [`contracts/shipment/tests/test_nft_proof_of_delivery_integration.rs`](contracts/shipment/tests/test_nft_proof_of_delivery_integration.rs), and the contract reference documents in [`docs/`](docs/).
- **`contracts/token`**: Fungible payment token contract providing balance tracking, transfer allowances, and batch transfer events. See tests in [`contracts/token/src/test.rs`](contracts/token/src/test.rs) and event docs in [`docs/events.md`](docs/events.md).
- **`contracts/nft`**: Shipment-asset NFT contract implementing unique digital asset minting, ownership, transfer, approval, and burning logic. See the contract implementation in [`contracts/nft/src/lib.rs`](contracts/nft/src/lib.rs) and its test suites in [`contracts/nft/src/`](contracts/nft/src/).

## Quick Start

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (latest stable)
- [Stellar CLI](https://developers.stellar.org/docs/build/smart-contracts/getting-started/setup) (only needed for schema generation and local networks)
- Make (optional, for convenience commands)

### Installation

```bash
# Fork the repository
# Then clone your fork into your local environment
git clone https://github.com/yourusername/navin-contracts.git
cd navin-contracts

# Add wasm32 target
rustup target add wasm32-unknown-unknown
```

### Build

```bash
# Build every contract for wasm32-unknown-unknown in release mode
make build
```

### Test

```bash
# Run all tests
make test

# Or directly with cargo
cargo test
```

### Format & Lint

```bash
# Format code
make fmt

# Check formatting (CI)
make fmt-check

# Run clippy lints
make lint

# Format check + clippy in one go (CI)
make check
```

## Development

For detailed contribution guidelines, please see [CONTRIBUTING.md](CONTRIBUTING.md).

### Running Locally

1. Build the contracts:

   ```bash
   make build
   ```

2. Run tests:

   ```bash
   make test
   ```

3. Run the full check suite (format check, clippy, tests, and build):

   ```bash
   make all
   ```

### Additional Make Targets

| Target | Purpose | Requirements |
|---|---|---|
| `make help` | List every available target | — |
| `make build` | Build all contracts for wasm32 | `wasm32-unknown-unknown` target |
| `make test` | Run the workspace test suite | — |
| `make coverage` | Line-coverage report in `target/llvm-cov/` | `cargo install cargo-llvm-cov` |
| `make fmt` / `make fmt-check` | Format code / verify formatting | — |
| `make lint` | Run clippy over all targets and features | — |
| `make check` | `fmt-check` + `lint` | — |
| `make all` | `check` + `test` + `build` | — |
| `make clean` | Remove build artifacts | — |
| `make generate-schema-shipment` | Regenerate `docs/contract-schema.shipment.json` | Stellar CLI |
| `make generate-schema-token` | Regenerate `docs/contract-schema.token.json` | Stellar CLI |
| `make generate-schema-nft` | Regenerate `docs/contract-schema.nft.json` | Stellar CLI |

The `docs/contract-schema.*.json` files are generated from the built WASM; do not edit them by hand. Regenerate them when a contract's public interface changes and commit the output only when it differs.

## Deployment

For deploying contracts to Stellar testnet, see the [Deployment Guide](docs/deployment.md).

Quick deployment:

```bash
# Build contracts
make build

# Deploy to testnet
./scripts/deploy-testnet.sh

# Initialize contracts
./scripts/init-testnet.sh
```

## Documentation

### Architecture and Contract Reference

- **[Integration Guide](docs/integration-guide.md)** - Contract ABI, event payload schemas, and the [architecture overview](docs/integration-guide.md#architecture-overview)
- **[Storage Layout](docs/storage.md)** - On-chain storage keys of the shipment contract
- **[Storage Key Registry](docs/storage-key-registry.md)** - Versioned registry of every storage key group
- **[Event Schema Reference](docs/events.md)** - Canonical event payloads and the `schema_version` policy
- **[Event Payload Schemas](docs/event_schemas.md)** - Conformance fixtures for the shipment event families
- **[Contract Package Versioning](docs/versioning.md)** - Why every contract crate stays at `version = "0.0.0"`, and which mechanisms to version instead
- **[Config Checksum](docs/config-checksum.md)** - Deterministic drift detection for the on-chain configuration

### Frontend Integration

- **[Freighter Integration Checklist](docs/freighter-integration-checklist.md)** - End-to-end checklist for frontend teams: preflight checks, signing, error handling, on-chain verification, and common failure modes
- **[Client Examples](docs/client-examples.md)** - Copy-paste Rust contract call shapes for common shipment operations
- **[Preflight Helpers](docs/preflight-helpers.md)** - Helpers for simulating state-changing calls before signing them
- **[Transaction Simulation Guide](docs/simulation.md)** - How to simulate contract calls with `scripts/simulate.sh`

### Operations

- **[Deployment Guide](docs/deployment.md)** - Testnet deployment and contract initialization
- **[Budget Report](docs/BUDGET_REPORT.md)** - Resource budget measurements for the contract entry points

## Technology Stack

- **Blockchain**: Stellar (Soroban smart contracts)
- **Language**: Rust
- **SDK**: Soroban SDK v22.0.0
- **Testing**: Soroban test utilities

## Contributing

We welcome contributions! Please see our [CONTRIBUTING.md](CONTRIBUTING.md) for details on:

- Setting up your development environment
- Code style and standards
- Testing requirements
- Submitting pull requests

## Security

Security is paramount for Navin. If you discover a security vulnerability, please email <navinxmr@gmail.com> instead of using the issue tracker. See also [SECURITY_GUIDE.md](SECURITY_GUIDE.md).

## Resources

- [Stellar Documentation](https://developers.stellar.org/)
- [Soroban Smart Contracts](https://soroban.stellar.org/)
- [Stellar CLI Reference](https://developers.stellar.org/docs/tools/developer-tools)

## Community

- [Telegram Group Chat](https://t.me/+3svwFsQME6k1YjI0)

---

**Built with ❤️ on Stellar**
