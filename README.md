# options-data

Parquet-backed store for 5-minute options bar data, designed for backtesting.

## Purpose and Principles

This repository exists to solve one problem well: **reliable, efficient access to intraday options bar data for backtesting and analysis.**

It is part of a modular ecosystem of independent repositories. Each repo has a focused responsibility and can be used on its own or composed with others. This repo is the options data layer — it owns the storage format, the retrieval API, and the tools to populate data. It does not contain trading logic, strategy evaluation, or real-time streaming.

### Core Principles

- **Data is gitignored.** Options data is ~1.5 GB for 2 years across all symbols. Unlike equity candles, it is too large to commit. The populate command fetches what's needed.

- **Single canonical format.** All data is stored as daily Parquet files with a fixed 13-column schema. One file per underlying per trading day. This convention makes the store predictable for consumers.

- **Idempotent population.** Running the populate command is always safe. It checks what exists, fetches what's missing, and never re-downloads data that's already present.

- **Multi-dimensional querying.** Options data has more dimensions than equities (underlying, expiration, strike, call/put). The `OptionsFilter` API supports querying along any combination of these dimensions.

- **Read-optimized for Rust.** The core library (`options-data-core`) is the primary public API. It is designed to be a dependency for Rust projects that need options bar data.

- **Precision over convenience.** Prices and strikes are stored as exact decimal strings, not floating-point. Timestamps are UTC microseconds. Contract types are decomposed from OCC symbols at ingestion time.

### What This Repo Is Not

- **Not for equity/ETF candle data.** Use [market-data](https://github.com/piekstra/market-data) for OHLCV candles.
- **Not for real-time streaming.** Use [market-data-live](https://github.com/piekstra/market-data-live) for live data feeds.
- **Not a trading engine.** No strategy logic, order management, or execution belongs here.
- **Not a pricing model.** This stores observed market data, not theoretical option values.
- **Not for greeks (yet).** Greeks come from snapshot endpoints, not bars. They can be added as a separate store/schema later.

### Related Repositories

| Repository | Purpose |
|------------|---------|
| [market-data](https://github.com/piekstra/market-data) | Historical + recent 5-min equity/ETF candles |
| [market-data-live](https://github.com/piekstra/market-data-live) | Real-time streaming market data via WebSocket |
| [options-data](https://github.com/piekstra/options-data) | Historical 5-min options bars (this repo) |

## Overview

This workspace provides:

- **`options-data-core`** — Core library with `OptionsBar` type, `OptionsBarStore` API, `OptionsFilter`, OCC symbol parser, and Parquet I/O. This is the primary dependency for consuming data.
- **`options-data-providers`** — Data fetching from Alpaca Options API.
- **`options-data-cli`** — CLI tool for populating and managing data.

## Data Layout

```
data/{UNDERLYING}/{YYYY}/{MM}/{YYYY-MM-DD}.parquet
```

One Parquet file per underlying per trading day. Each file contains 5-minute bars for all contracts with volume > 0 on that day, with columns:

| Column | Type | Description |
|--------|------|-------------|
| `timestamp` | Timestamp/UTC | Bar start time |
| `contract_symbol` | Utf8 | OCC symbol (e.g., `TQQQ250221C00090000`) |
| `underlying` | Utf8 | Underlying ticker |
| `expiration` | Date32 | Contract expiration |
| `strike` | Utf8 | Strike price (decimal string) |
| `contract_type` | Utf8 | `C` or `P` |
| `open`, `high`, `low`, `close` | Utf8 | Prices (decimal strings) |
| `volume` | Int64 | Bar volume |
| `trade_count` | Int64 | Number of trades |
| `vwap` | Utf8 | Volume-weighted average price |

Rows are sorted by `(contract_symbol, timestamp)` for compression efficiency.

## Quick Start

### Populate data

```bash
# Populate a single underlying (requires ALPACA_API_KEY_ID and ALPACA_API_SECRET_KEY env vars)
cargo run -p options-data-cli -- populate -s TQQQ --start 2025-01-01

# Populate all default underlyings
cargo run -p options-data-cli -- populate -s all --start 2025-01-01

# Force re-download existing data
cargo run -p options-data-cli -- populate -s TQQQ --start 2025-01-01 --end 2025-01-31 --force
```

The populate command is idempotent — it checks what data already exists and only fetches missing dates.

### Check status

```bash
cargo run -p options-data-cli -- status
cargo run -p options-data-cli -- status -s TQQQ
```

### Validate data

```bash
cargo run -p options-data-cli -- validate
```

### Use as a library

Add to your `Cargo.toml`:

```toml
options-data-core = { path = "../options-data/crates/options-data-core" }
```

```rust
use options_data_core::store::{OptionsBarStore, OptionsFilter};
use options_data_core::bar::ContractType;
use chrono::NaiveDate;
use rust_decimal_macros::dec;

let store = OptionsBarStore::new("../options-data");

// Read a date range
let bars = store.read_range("TQQQ",
    NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
    NaiveDate::from_ymd_opt(2025, 1, 31).unwrap(),
)?;

// Filter by contract type and strike range
let filter = OptionsFilter {
    contract_type: Some(ContractType::Call),
    strike_range: Some((dec!(85.000), dec!(95.000))),
    min_volume: Some(100),
    ..Default::default()
};

let filtered = store.read_range_filtered("TQQQ",
    NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
    NaiveDate::from_ymd_opt(2025, 1, 31).unwrap(),
    &filter,
)?;

// Check what's available
let underlyings = store.list_underlyings()?;
let dates = store.list_dates("TQQQ")?;
let missing = store.missing_dates("TQQQ",
    NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
    NaiveDate::from_ymd_opt(2025, 1, 31).unwrap(),
);
```

## Configuration

Copy `.env.example` to `.env` and fill in your credentials:

```
ALPACA_API_KEY_ID=your_key_id
ALPACA_API_SECRET_KEY=your_secret_key
```

## Development

```bash
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

## Versioning

Semver. All crates share the workspace version. Releases are triggered by git tags (`v0.1.0`).
