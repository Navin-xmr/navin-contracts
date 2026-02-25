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
│   └── example-contract    # Core delivery tracking contract
│       ├── src
│       │   ├── lib.rs      # Main contract logic
│       │   ├── storage.rs  # Data persistence layer
│       │   ├── transactions.rs  # Transaction handling
│       │   ├── types.rs    # Type definitions
│       │   └── test.rs     # Contract tests
│       ├── Cargo.toml
│       └── Makefile
├── Cargo.toml              # Workspace configuration
├── Makefile                # Build and test commands
├── CONTRIBUTING.md         # Contribution guidelines
└── README.md
```

## Quick Start

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (latest stable)
- [Stellar CLI](https://developers.stellar.org/docs/build/smart-contracts/getting-started/setup)
- Make (optional, for convenience commands)

### Installation

```bash
# Fork  the repository
# Then clone your fork into your local environment
git clone https://github.com/yourusername/navin-contracts.git
cd navin-contracts

# Add wasm32 target
rustup target add wasm32-unknown-unknown
```

### Build

```bash
# Using Make
make build
```

OR

```
# Using cargo
cargo build --target wasm32-unknown-unknown --release
```

```
# Or directly with Stellar CLI
stellar contract build
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

```

```
# OR
cargo fmt
```

```
# Check formatting
make fmt-check

```

```
# OR
cargo check --all
```

```
# Run clippy lints
make lint
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

3. Deploy to local network:

   ```bash
   make deploy-local
   ```

## Deployment

For deploying contracts to Stellar testnet, see the [Deployment Guide](docs/deployment.md).

Quick deployment:

```bash
# Build contracts
./scripts/build.sh

# Deploy to testnet
./scripts/deploy-testnet.sh

# Initialize contracts
./scripts/init-testnet.sh
```

## Architecture

Navin's smart contracts handle:

- **Asset Management**: Secure storage and transfer of delivery tokens
- **Access Control**: Role-based permissions for different stakeholders
- **Transaction Logging**: Immutable audit trail of all operations
- **Asset Locking**: Time-based locks for escrow and guarantees

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

Security is paramount for Navin. If you discover a security vulnerability, please email <navinxmr@gmail.com> instead of using the issue tracker.

## Resources

- [Stellar Documentation](https://developers.stellar.org/)
- [Soroban Smart Contracts](https://soroban.stellar.org/)
- [Stellar CLI Reference](https://developers.stellar.org/docs/tools/developer-tools)

## Community

- [Twitter](https://twitter.com/navinxmr)
- [Telegram Group Chat](https://t.me/+3svwFsQME6k1YjI0)

---

**Built with ❤️ on Stellar**
