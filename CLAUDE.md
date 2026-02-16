# options-data

## Repository Structure

```
options-data/
├── crates/
│   ├── options-data-core/       # Core types, Parquet I/O, OptionsBarStore API
│   ├── options-data-providers/  # Alpaca options data fetching
│   └── options-data-cli/        # CLI for populate, status, validate
├── data/                        # Gitignored, populated by CLI
└── .github/workflows/           # CI (PRs) and Release (tags)
```

## Key Commands

```bash
# Check everything
cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo test --workspace

# Run tests
cargo test --workspace

# Run CLI
cargo run -p options-data-cli -- populate -s TQQQ --start 2025-01-01
cargo run -p options-data-cli -- populate -s all --start 2025-01-01
cargo run -p options-data-cli -- status
cargo run -p options-data-cli -- validate
```

## Architecture

- **OptionsBar**: 13 fields — `timestamp`, `contract_symbol`, `underlying`, `expiration`, `strike`, `contract_type` (Call/Put), OHLCV, `trade_count`, `vwap`
- **ContractType**: `Call` or `Put` — represented as "C"/"P" in storage
- **OCC Symbol Parser**: Parses `TQQQ250221C00090000` → (TQQQ, 2025-02-21, Call, 90.000)
- **OptionsBarStore**: Filesystem-backed store, one Parquet file per underlying per trading day at `data/{UNDERLYING}/{YYYY}/{MM}/{YYYY-MM-DD}.parquet`
- **OptionsFilter**: Multi-dimensional query filter (contract_type, expiration, strike_range, contract_symbol, min_volume)
- **OptionsBarProvider**: Async trait for data fetching. Alpaca uses `/v1beta1/options/bars` endpoint.
- Prices stored as decimal strings in Parquet for exact `rust_decimal` precision
- Data is gitignored (~1.5 GB for 2 years across all symbols)

## Versioning

All crates share `workspace.package.version`. Releases via git tags (`v*`).
