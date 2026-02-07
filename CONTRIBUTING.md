# Contributing to Navin

Thank you for your interest in contributing to Navin! This guide will help you get started and ensure your contributions can be smoothly integrated.

## Table of Contents

- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Making Contributions](#making-contributions)
- [Pre-PR Checklist](#pre-pr-checklist)
- [CI/CD Requirements](#cicd-requirements)
- [Code Standards](#code-standards)
- [Testing Guidelines](#testing-guidelines)
- [Getting Help](#getting-help)

## Getting Started

### Prerequisites

Before contributing, ensure you have the following installed:

 **Rust** (latest stable version)


### Fork and Clone

1. Fork the repository on GitHub by clicking the "Fork" button

2. Clone your fork locally:
   ```bash
   git clone https://github.com/YOUR-USERNAME/navin-contracts.git
   cd navin-contracts
   ```

3. Add the upstream repository: (So you can pull and sync new updates) 
NB: This is optional as you can just sync fork through github website
   ```bash
   git remote add upstream https://github.com/Navin-xmr/navin-contracts.git
   ```

## Development Setup

### Verify Your Environment

Run these commands to ensure everything is set up correctly:

```bash
# Check Rust version
rustc --version

# Check cargo
cargo --version

```

## Making Contributions

### Step 1: Create a Branch

Always create a new branch for your work. Never commit directly to `main` or `develop`.

```bash
# Update your local main branch
git checkout main

```

If you added the upstream branch, pull the changes 

(optional - you can just use github to sync your fork)
```
git pull upstream main
```

## Create a new branch with a descriptive name
git checkout -b feature/your-feature-name
```
### Examples of good branch names:
 - feature/add-delivery-status
 - fix/timestamp-validation
 - docs/update-readme
 - refactor/optimize-storage
```

### Step 2: Make Your Changes

1. Write your code following our [Code Standards](#code-standards)
2. Add or update tests for your changes
3. Update documentation if needed
4. Keep commits focused and atomic

### Step 3: Test Your Changes Locally

Before pushing, **always run these commands locally** to ensure CI will pass:

```bash
# 1. Format your code
cargo fmt

# 2. Run clippy (linter) - must pass with no warnings
cargo clippy --all-targets --all-features -- -D warnings

# 3. Run all tests
cargo test

# 4. Build contracts (WASM target)
cargo build --target wasm32-unknown-unknown --release
```

**Important**: If any of these commands fail, fix the issues before pushing!

### Step 4: Commit Your Changes

```bash
# Stage your changes
git add .

# Commit with a descriptive message
git commit -m "type: brief description

Longer explanation if needed.

- Detail 1
- Detail 2"
```

**Commit Message Format:**
- `feat:` - New feature
- `fix:` - Bug fix
- `docs:` - Documentation changes
- `test:` - Adding or updating tests
- `refactor:` - Code refactoring
- `chore:` - Maintenance tasks

**Examples:**
```bash
git commit -m "feat: add delivery status tracking"
git commit -m "fix: resolve timestamp overflow in lock_assets"
git commit -m "docs: update installation instructions"
```

### Step 5: Push to Your Fork

```bash
git push origin feature/your-feature-name
```

### Step 6: Create a Pull Request

1. Go to your fork on GitHub
2. Click "Compare & pull request"
3. Fill out the PR template:
   - **Title**: Clear, concise description
   - **Description**: Explain what and why
   - **Testing**: Describe how you tested
   - **Checklist**: Complete all items

## Pre-PR Checklist

Before submitting your PR, ensure you've completed ALL of these steps:

- [ ] Code is formatted with `cargo fmt`
- [ ] No clippy warnings: `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] All tests pass: `cargo test`
- [ ] WASM builds successfully: `cargo build --target wasm32-unknown-unknown --release`
- [ ] New tests added for new functionality
- [ ] Documentation updated (if applicable)
- [ ] Commit messages follow the format guidelines
- [ ] Branch is up to date with `main` or `develop`

## CI/CD Requirements

Our CI pipeline runs the following checks. **All must pass** before your PR can be merged:

### 1. Code Formatting Check
```bash
# Command run by CI:
cargo fmt --check

# To fix formatting issues:
cargo fmt
```

### 2. Clippy Lints
```bash
# Command run by CI:
cargo clippy --all-targets --all-features -- -D warnings

# This fails on ANY warnings, so fix all clippy suggestions
```

### 3. Tests
```bash
# Command run by CI:
cargo test

# Make sure all tests pass locally
```

### 4. WASM Build
```bash
# Command run by CI:
cargo build --target wasm32-unknown-unknown --release

# Ensure contracts compile to WASM without errors
```

### 5. Security Audit
```bash
# Command run by CI:
cargo audit --ignore RUSTSEC-0000-0000

# Checks for known security vulnerabilities in dependencies
```

## Code Standards

### Rust Style Guide

- Follow the official [Rust Style Guide](https://doc.rust-lang.org/nightly/style-guide/)
- Use `cargo fmt` to automatically format code
- Use descriptive variable names relevant to the feature

### Contract-Specific Guidelines

1. **Error Handling**
   - Always use proper error types (don't use `panic!`)
   - Return `Result<T, Error>` for fallible operations
   
   ```rust
   // Good
   pub fn transfer(env: Env, from: Address, to: Address, amount: i128) -> Result<(), Error> {
       if amount <= 0 {
           return Err(VaultError::InvalidAmount.into());
       }
       Ok(())
   }
   
   // Bad
   pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
       assert!(amount > 0);  // Don't use assertions for validation
   }
   ```

2. **Authentication**
   - Always verify addresses with `require_auth()`
   
   ```rust
   pub fn withdraw(env: Env, from: Address, amount: i128) -> Result<(), Error> {
       from.require_auth();  // Always first!
       // ... rest of function
   }
   ```

3. **Storage**
   - Use type-safe storage keys
   - Document storage layout
   - Consider gas costs

4. **Documentation**
   - Add doc comments to all public functions
   - Explain complex logic
   - Document error conditions
   
   ```rust
   /// Deposits assets into the vault
   /// 
   /// # Arguments
   /// * `from` - The address depositing assets
   /// * `amount` - Amount to deposit (must be positive)
   /// 
   /// # Errors
   /// Returns `InvalidAmount` if amount <= 0
   pub fn deposit(env: Env, from: Address, amount: i128) -> Result<(), Error> {
       // implementation
   }
   ```

## Testing Guidelines

### Writing Tests

1. **Test Structure**
   ```rust
   #[test]
   fn test_descriptive_name() {
       // Setup
       let env = Env::default();
       let contract = setup_contract(&env);
       
       // Execute
       let result = contract.function_to_test();
       
       // Assert
       assert!(result.is_ok());
   }
   ```

2. **Test Coverage**
   - Test error conditions
   - Test edge cases (zero, negative, maximum values)
   - Test access control (for scenarios like admin or regular users)

3. **Test Organization**
   - Keep tests in `test.rs` or dedicated test modules
   - Group related tests together
   - Use descriptive test names

### Example Test

```rust
#[test]
fn test_withdraw_insufficient_funds() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SecureAssetVault);
    let client = SecureAssetVaultClient::new(&env, &contract_id);
    
    let user = Address::generate(&env);
    
    // Try to withdraw more than deposited
    let result = client.try_withdraw(&user, &user, &1000);
    
    // Should fail with InsufficientFunds
    assert!(result.is_err());
}
```

## Common Issues and Solutions

### Issue: Clippy warnings about needless borrow

```rust
// Wrong
Vec::new(&env)

// Correct
Vec::new(env)
```

### Issue: Assertion on constants

```rust
// Wrong
assert!(true, "Always passes");

// Correct - Remove unnecessary assertions
// Just call the function or use proper assertions
```

### Issue: CI fails but local tests pass

1. Ensure you've run ALL pre-PR checklist items
2. Pull latest changes from upstream
3. Clear cargo cache and rebuild:
   ```bash
   cargo clean
   cargo test
   ```

## Getting Help

- **Bugs**: Open an [Issue](https://github.com/Navin-xmr/navin-contracts/issues)
- **Security**: Email navinxmr@gmail.com
- **General**: Join our Telegram - [Telegram Group Chat](https://t.me/+3svwFsQME6k1YjI0)

## Review Process

1. **Automated Checks**: CI must pass (typically 5 minutes)
2. **Code Review**: Maintainer reviews code (1-3 days)
3. **Revisions**: Address feedback. fix conflicts and push updates 

### Disclaimer: 
Please do not just combine both conflicts when trying to resolve merge conflicts then just push. Such PRs will not be merged. Actually check the differences, add and fix issues, ensure your build and tests work then you can push.


## Quick Command Reference

```bash
# Complete pre-submission checklist
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --target wasm32-unknown-unknown --release

# Create and switch to a new branch
git checkout -b feature/your-feature

# Stage, commit, and push
git add .
git commit -m "feat: your change description"
git push origin feature/your-feature

# Update your branch with latest upstream changes (Optional - Can be done on Website)
git checkout main
git pull upstream main
git checkout feature/your-feature
git rebase main
```

---

Thank you for contributing to Navin!
Together, we're building a transparent and secure delivery tracking platform.
