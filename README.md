# options-data

Historical options data store for backtesting and analysis.

> **Status:** Placeholder — not yet implemented.

## Purpose and Principles

This repository will handle **options market data** — options chains, greeks, and pricing history. It is the options counterpart to [market-data](https://github.com/piekstra/market-data), which handles equity/ETF OHLCV candles.

It is part of a modular ecosystem of independent repositories. Each repo has a focused responsibility and can be used on its own or composed with others.

### Intended Scope

- Historical options chain data (calls, puts, strikes, expirations)
- Greeks (delta, gamma, theta, vega, rho) where available
- Bid/ask spreads, open interest, and volume
- Efficient querying by underlying symbol, expiration date, and strike
- A clean Rust API for consumers

### What This Repo Is Not

- **Not for equity/ETF candle data.** Use [market-data](https://github.com/piekstra/market-data) for OHLCV candles.
- **Not for real-time streaming.** Use [market-data-live](https://github.com/piekstra/market-data-live) for live data feeds.
- **Not a trading engine.** No strategy logic, order management, or execution belongs here.
- **Not a pricing model.** This stores observed market data, not theoretical option values.

### Relationship to market-data

| Concern | market-data | options-data |
|---------|-------------|--------------|
| Instrument type | Equities / ETFs | Options contracts |
| Data shape | OHLCV candles (fixed schema) | Chains, strikes, greeks (complex schema) |
| Key dimensions | Symbol + Date + Time | Underlying + Expiration + Strike + Type |
| File granularity | One file per symbol per day | TBD |

## License

MIT
