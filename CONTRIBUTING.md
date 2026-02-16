# Contributing to options-data

Thank you for your interest in contributing! This document provides guidelines for contributing to the project.

## Getting Started

1. Fork the repository
2. Clone your fork locally
3. Create a branch for your change (`git checkout -b my-change`)
4. Make your changes
5. Run the checks (see below)
6. Commit and push your branch
7. Open a pull request

## Development Setup

You need Rust stable installed. Then:

```bash
cargo build --workspace
cargo test --workspace
```

To populate data, copy `.env.example` to `.env` and fill in your Alpaca API credentials.

## Before Submitting

All of these must pass:

```bash
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

## What to Contribute

- **New data providers** — implement the `OptionsBarProvider` trait in `crates/options-data-providers/`
- **Store improvements** — query optimizations, new filter methods in `crates/options-data-core/`
- **CLI features** — new commands or flags in `crates/options-data-cli/`
- **Bug fixes** — with a test that reproduces the issue

## Guidelines

- Every new feature or bug fix should include unit tests
- Follow existing code style (run `cargo fmt`)
- Keep PRs focused — one feature or fix per PR
- Update README.md if adding user-facing features
- Do not commit `.env` files or credentials

## Adding a New Provider

1. Create a new file in `crates/options-data-providers/src/`
2. Implement the `OptionsBarProvider` trait
3. Add JSON fixture tests for response parsing
4. Register the provider in the CLI's `create_provider` function
5. Document any required environment variables in `.env.example`

## Reporting Issues

Open an issue with:
- What you expected
- What happened
- Steps to reproduce
- Rust version (`rustc --version`)
