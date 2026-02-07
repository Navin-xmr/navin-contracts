# Quick Start Guide for Contributors

This guide will help you get started contributing to Navin in minutes.

## Prerequisites Check

Run this command to check if you have everything installed:

```bash
make check-setup
```


## Fork and Clone

1. Fork the repository on GitHub (click the "Fork" button)

2. Clone your fork:
   ```bash
   git clone https://github.com/YOUR_USERNAME/navin-contracts.git
   cd navin-contracts
   ```

3. Add upstream remote:
   ```bash
   git remote add upstream https://github.com/Navin-xmr/navin-contracts.git
   ```

## Making Your First Contribution

### 1. Create a Branch

```bash
git checkout -b feature/your-feature-name
```

### 2. Make Your Changes

Edit the files you need to change.

### 3. Before Committing - Run These Commands

**This is the most important part!** Run these commands to ensure your PR will pass CI:

```bash
# Format your code
make fmt

# Run all pre-commit checks (formatting, linting, tests, build)
make pre-commit
```

If `make pre-commit` passes, your PR will likely pass CI! ✅

### 4. Commit Your Changes

```bash
git add .
git commit -m "feat: add your feature description"
```

Good commit message examples:
- `feat(tracking): add delivery status endpoint`
- `fix(storage): resolve balance overflow bug`
- `docs: update installation instructions`
- `test: add tests for asset locking`

### 5. Push to Your Fork

```bash
git push origin feature/your-feature-name
```

### 6. Create Pull Request

1. Go to your fork on GitHub
2. Click "Compare & pull request"
3. Fill in the PR template
4. Submit!

## Essential Commands

### Daily Development

```bash
# Build contracts
make build

# Run tests
make test

# Format code
make fmt

# Check everything (quick)
make check
```

### Before Creating a PR

```bash
# Run ALL checks (this is what CI runs)
make pre-commit
```

This single command runs:
1. ✅ Format check (`cargo fmt --check`)
2. ✅ Lint check (`cargo clippy`)
3. ✅ All tests (`cargo test`)
4. ✅ WASM build (`stellar contract build`)

### If CI Fails

If your PR fails CI, run these commands locally:

```bash
# Check formatting
make fmt-check

# If formatting fails, fix it:
make fmt

# Check lints
make lint

# If lint fails, try auto-fix:
make lint-fix

# Run tests
make test

# Build contracts
make build
```

Then commit and push the fixes:

```bash
git add .
git commit -m "fix: address CI feedback"
git push
```

## Common Issues and Solutions

### Issue: "Format check failed"

**Solution:**
```bash
make fmt
git add .
git commit -m "style: format code"
git push
```

### Issue: "Clippy lints failed"

**Solution:**
```bash
make lint-fix  # Auto-fix what's possible
make lint      # See remaining issues
# Fix remaining issues manually
git add .
git commit -m "fix: resolve clippy warnings"
git push
```

### Issue: "Tests failed"

**Solution:**
```bash
cargo test -- --nocapture  # See detailed output
# Fix the failing tests
git add .
git commit -m "test: fix failing tests"
git push
```

### Issue: "Build failed"

**Solution:**
```bash
cargo build --target wasm32-unknown-unknown --release
# Fix compilation errors
git add .
git commit -m "fix: resolve build errors"
git push
```

## Testing Your Changes

### Run all tests:
```bash
make test
```

### Run specific test:
```bash
cargo test test_name
```

### Run tests with output:
```bash
make test-verbose
```

## Getting Help

- Read the full [CONTRIBUTING.md](../CONTRIBUTING.md)

## Summary Checklist

Before submitting a PR, make sure:

- [ ] Code is formatted (`make fmt`)
- [ ] All pre-commit checks pass (`make pre-commit`)
- [ ] Tests are added for new features
- [ ] Documentation is updated if needed
- [ ] Commit messages follow convention
- [ ] PR template is filled out

**Key Command to Remember:**

```bash
make pre-commit
```

This runs everything CI will check. If it passes locally, your PR will likely pass CI!

---

Happy contributing!
