# Contributing to Water Credits Contracts

Thank you for your interest in contributing! This document covers the setup, workflow, and standards for contributing to this project.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Workflow](#workflow)
- [Coding Standards](#coding-standards)
- [Testing](#testing)
- [Pull Request Process](#pull-request-process)
- [Issue Reporting](#issue-reporting)
- [Security](#security)

## Code of Conduct

By participating, you agree to uphold our [Code of Conduct](CODE_OF_CONDUCT.md). Please report unacceptable behavior to the project maintainers.

## Getting Started

1. Fork the repository.
2. Clone your fork:
   ```bash
   git clone https://github.com/YOUR_USER/water-credits-contracts
   cd water-credits-contracts
   ```
3. Add the upstream remote:
   ```bash
   git remote add upstream https://github.com/your-org/water-credits-contracts
   ```

## Development Setup

### Prerequisites

- Rust 1.75+
- Soroban CLI 20.0.0
- WASM target: `rustup target add wasm32-unknown-unknown`
- Docker (for local Stellar devnet)

### Build & Test

```bash
# Build all contracts
make build

# Run all tests
make test

# Lint
make lint

# Format check
make fmt
```

## Workflow

1. **Sync your fork** — `git checkout main && git pull upstream main`
2. **Create a feature branch** — `git checkout -b feat/your-feature`
3. **Make changes** — Follow the coding standards below.
4. **Commit** — Use [Conventional Commits](https://www.conventionalcommits.org/):
   - `feat:` — new feature
   - `fix:` — bug fix
   - `docs:` — documentation
   - `refactor:` — code restructuring
   - `test:` — adding/updating tests
   - `chore:` — maintenance tasks
5. **Push** — `git push origin feat/your-feature`
6. **Open a Pull Request** — See [PR process](#pull-request-process).

## Coding Standards

- **Formatting**: Run `cargo fmt` before every commit.
- **Linting**: `cargo clippy --workspace -- -D warnings` must pass with zero warnings.
- **Tests**: All tests must pass. Aim for 95%+ coverage on new code.
- **Documentation**: All public functions must have doc comments (`///`).
- **No unsafe code**: `unsafe` blocks are prohibited.
- **Naming**: Follow Rust naming conventions — `snake_case` for functions/variables, `UpperCamelCase` for types.
- **Error handling**: Use `panic!` with clear messages for invariant violations; use `Result` for recoverable errors.

### Commit Convention

We use [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

[optional body]
```

Examples:
- `feat(oracle): add multi-oracle median aggregation`
- `fix(token): prevent zero-amount retirement`
- `docs: update deployment guide`

## Testing

### Running Tests

```bash
# All tests
cargo test --workspace

# With output
cargo test --workspace -- --nocapture

# Specific test
cargo test test_credit_lifecycle -- --nocapture

# Integration tests only
cargo test --test '*' -- --nocapture
```

### Test Checklist

Before submitting a PR, ensure:

- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy --workspace -- -D warnings` passes
- [ ] `cargo test --workspace` passes
- [ ] New tests cover the changes
- [ ] Manual testing on local devnet (if applicable)

## Pull Request Process

1. **Title** must follow Conventional Commits format.
2. **Description** should explain:
   - What the change does
   - Why it's needed
   - How it was tested
   - Any breaking changes
3. **Link** to any related issues.
4. **Assign** a reviewer from the maintainers team.
5. **Ensure CI passes** — the PR must build and all tests must pass.
6. **Squash merge** is preferred to keep history clean.

### PR Checklist

- [ ] Code follows coding standards
- [ ] Tests added/updated
- [ ] Documentation updated (if needed)
- [ ] CHANGELOG entry added (if applicable)
- [ ] No breaking changes without discussion

## Issue Reporting

See our issue templates:
- [Report a Bug](.github/ISSUE_TEMPLATE/bug_report.md)
- [Request a Feature](.github/ISSUE_TEMPLATE/feature_request.md)

## Security

For security vulnerabilities, please see [SECURITY.md](SECURITY.md). Do not report security issues in public GitHub issues.

---

Thank you for contributing to water quality and replenishment credits! 🌊
