# Requirements Document: CI/CD Setup for Navin Contracts

## Introduction

The navin-contracts repository is a Rust-based Soroban smart contracts project for the Stellar blockchain. The repository currently has a disabled GitHub Actions workflow that references a frontend project. The team needs a proper CI/CD pipeline that validates contract correctness, enforces code quality standards, and ensures all checks pass for pull requests and main branch merges.

The CI/CD system should automate the verification process that developers currently run manually via `make check`, `make test`, and `make build`. This ensures consistency across all contributions and prevents broken code from entering the main branch.

## Glossary

- **CI_Pipeline**: The automated continuous integration system that validates code changes
- **Soroban_Contract**: A smart contract written for the Stellar blockchain using the Soroban SDK
- **WASM_Binary**: WebAssembly compiled output of the Soroban contracts
- **Clippy**: Rust's official linting tool for catching common mistakes
- **Cargo**: Rust's package manager and build system
- **Pull_Request**: A proposed code change submitted for review before merging
- **Main_Branch**: The primary development branch (typically "main" or "master")
- **Workflow_File**: A YAML configuration file defining GitHub Actions CI/CD steps
- **Build_Artifact**: The compiled WASM binaries produced during the build process
- **Test_Suite**: The collection of unit and integration tests in the contracts
- **Format_Check**: Verification that code follows Rust formatting standards (rustfmt)

## Requirements

### Requirement 1: Automated Code Quality Checks

**User Story:** As a maintainer, I want automated code quality checks to run on every pull request, so that code quality standards are consistently enforced.

#### Acceptance Criteria

1. WHEN a pull request is opened, THE CI_Pipeline SHALL run format checking using `cargo fmt --all -- --check`
2. WHEN a pull request is opened, THE CI_Pipeline SHALL run clippy linting using `cargo clippy --all-targets --all-features`
3. IF format checking fails, THEN THE CI_Pipeline SHALL fail with a descriptive error message
4. IF clippy linting fails, THEN THE CI_Pipeline SHALL fail with a descriptive error message
5. THE CI_Pipeline SHALL execute format checking and linting in parallel to minimize execution time

### Requirement 2: Automated Test Execution

**User Story:** As a developer, I want all tests to run automatically on pull requests, so that I can verify my changes don't break existing functionality.

#### Acceptance Criteria

1. WHEN a pull request is opened, THE CI_Pipeline SHALL run all tests using `cargo test`
2. WHEN tests are executed, THE CI_Pipeline SHALL report the number of tests passed and failed
3. IF any test fails, THEN THE CI_Pipeline SHALL fail and prevent merging
4. THE Test_Suite SHALL include tests from all workspace members (shipment and token contracts)
5. WHEN the Test_Suite completes successfully, THE CI_Pipeline SHALL proceed to build validation

### Requirement 3: Contract Build Validation

**User Story:** As a maintainer, I want to verify that contracts compile to valid WASM binaries, so that deployment-ready artifacts are always available.

#### Acceptance Criteria

1. WHEN tests pass, THE CI_Pipeline SHALL build contracts using `cargo build --target wasm32-unknown-unknown --release`
2. IF the build fails, THEN THE CI_Pipeline SHALL fail and report compilation errors
3. WHEN the build succeeds, THE CI_Pipeline SHALL produce WASM_Binary artifacts for both shipment and token contracts
4. THE CI_Pipeline SHALL verify that WASM_Binary artifacts exist at expected paths: `target/wasm32-unknown-unknown/release/shipment.wasm` and `target/wasm32-unknown-unknown/release/token.wasm`
5. WHEN WASM_Binary artifacts are produced, THE CI_Pipeline SHALL upload them as build artifacts with 7-day retention

### Requirement 4: Pull Request Trigger Configuration

**User Story:** As a contributor, I want CI checks to run automatically when I open or update a pull request, so that I receive immediate feedback on my changes.

#### Acceptance Criteria

1. WHEN a pull request is opened targeting Main_Branch, THE CI_Pipeline SHALL trigger automatically
2. WHEN a pull request is synchronized (new commits pushed), THE CI_Pipeline SHALL trigger automatically
3. THE CI_Pipeline SHALL execute all jobs (format check, lint, test, build) in the optimal order
4. WHEN all CI jobs pass, THE CI_Pipeline SHALL mark the pull request as passing checks
5. IF any CI job fails, THEN THE CI_Pipeline SHALL mark the pull request as failing checks and block merging

### Requirement 5: Main Branch Protection

**User Story:** As a maintainer, I want CI checks to run on the main branch after merges, so that the health of the main branch is continuously monitored.

#### Acceptance Criteria

1. WHEN code is pushed to Main_Branch, THE CI_Pipeline SHALL trigger automatically
2. WHEN code is merged to Main_Branch, THE CI_Pipeline SHALL execute all validation steps
3. IF Main_Branch CI checks fail, THEN THE CI_Pipeline SHALL notify the team through GitHub Actions status
4. THE CI_Pipeline SHALL execute the same validation steps on Main_Branch as on pull requests
5. WHEN Main_Branch builds succeed, THE CI_Pipeline SHALL update the repository status badge

### Requirement 6: Rust Toolchain Configuration

**User Story:** As a developer, I want the CI environment to match the project's Rust toolchain requirements, so that CI results are consistent with local development.

#### Acceptance Criteria

1. THE CI_Pipeline SHALL use the stable Rust toolchain
2. THE CI_Pipeline SHALL install the `wasm32-unknown-unknown` compilation target
3. THE CI_Pipeline SHALL cache Cargo dependencies to reduce build times
4. WHEN Cargo.lock changes, THE CI_Pipeline SHALL invalidate the cache and rebuild dependencies
5. THE CI_Pipeline SHALL use the latest Ubuntu LTS runner environment

### Requirement 7: Workflow File Management

**User Story:** As a maintainer, I want the outdated frontend workflow replaced with the contracts CI workflow, so that the repository has only relevant CI configuration.

#### Acceptance Criteria

1. THE new Workflow_File SHALL be created at `.github/workflows/ci.yml`
2. THE existing Workflow_File at `.github/workflows/test.yml` SHALL be removed
3. THE Workflow_File SHALL have a descriptive name: "Contracts CI"
4. THE Workflow_File SHALL include comments explaining each job's purpose
5. WHEN viewing the GitHub Actions tab, THE Workflow_File SHALL display as "Contracts CI"

### Requirement 8: Manual Trigger Support

**User Story:** As a maintainer, I want to manually trigger CI runs for testing or validation purposes, so that I can verify CI changes without creating pull requests.

#### Acceptance Criteria

1. WHERE manual execution is needed, THE CI_Pipeline SHALL support `workflow_dispatch` trigger
2. WHEN a user triggers the workflow manually, THE CI_Pipeline SHALL execute all validation steps
3. THE CI_Pipeline SHALL allow manual triggers from any branch
4. WHEN manually triggered, THE CI_Pipeline SHALL produce the same artifacts as automated runs
5. THE CI_Pipeline SHALL display manual trigger results in the GitHub Actions interface

### Requirement 9: CI Performance Optimization

**User Story:** As a contributor, I want CI checks to complete quickly, so that I can iterate rapidly on my changes.

#### Acceptance Criteria

1. THE CI_Pipeline SHALL execute independent jobs (format check, lint) in parallel
2. THE CI_Pipeline SHALL use GitHub Actions caching for Cargo registry and build artifacts
3. WHEN dependencies are cached, THE CI_Pipeline SHALL complete format and lint checks within 2 minutes
4. WHEN dependencies are cached, THE CI_Pipeline SHALL complete test execution within 5 minutes
5. WHEN dependencies are cached, THE CI_Pipeline SHALL complete WASM builds within 3 minutes

### Requirement 10: Clear Status Reporting

**User Story:** As a reviewer, I want clear status reporting from CI checks, so that I can quickly understand what failed and why.

#### Acceptance Criteria

1. WHEN a CI job fails, THE CI_Pipeline SHALL display the job name and failure reason in the GitHub UI
2. WHEN format checking fails, THE CI_Pipeline SHALL show which files need formatting
3. WHEN clippy fails, THE CI_Pipeline SHALL display the specific linting warnings or errors
4. WHEN tests fail, THE CI_Pipeline SHALL show which test cases failed and their output
5. WHEN build fails, THE CI_Pipeline SHALL display compilation errors with file locations and line numbers
