use std::collections::{BTreeMap, HashMap};

use async_trait::async_trait;
use chrono::{NaiveDate, TimeZone, Utc};
use options_data_core::bar::OptionsBar;
use options_data_core::contract::parse_occ_symbol;
use reqwest::Client;
use rust_decimal::Decimal;
use serde::Deserialize;
use tracing::{debug, info, warn};

use crate::error::ProviderError;
use crate::provider::OptionsBarProvider;

const ALPACA_DATA_BASE_URL: &str = "https://data.alpaca.markets/v1beta1/options";

/// Maximum number of retries on rate limit before giving up.
const MAX_RATE_LIMIT_RETRIES: u32 = 5;

/// Alpaca options data provider.
/// Fetches 5-minute options bars via the Alpaca Options Data API.
pub struct AlpacaOptionsProvider {
    client: Client,
    api_key_id: String,
    api_secret_key: String,
    base_url: String,
}

impl AlpacaOptionsProvider {
    /// Create from environment variables `ALPACA_API_KEY_ID` and `ALPACA_API_SECRET_KEY`.
    pub fn from_env() -> Result<Self, ProviderError> {
        let api_key_id = std::env::var("ALPACA_API_KEY_ID")
            .map_err(|_| ProviderError::Config("ALPACA_API_KEY_ID not set".into()))?;
        let api_secret_key = std::env::var("ALPACA_API_SECRET_KEY")
            .map_err(|_| ProviderError::Config("ALPACA_API_SECRET_KEY not set".into()))?;

        Ok(Self {
            client: Client::new(),
            api_key_id,
            api_secret_key,
            base_url: ALPACA_DATA_BASE_URL.to_string(),
        })
    }

    /// Create with explicit credentials and optional base URL override.
    pub fn new(api_key_id: String, api_secret_key: String, base_url: Option<String>) -> Self {
        Self {
            client: Client::new(),
            api_key_id,
            api_secret_key,
            base_url: base_url.unwrap_or_else(|| ALPACA_DATA_BASE_URL.to_string()),
        }
    }

    /// Fetch options bars with pagination and rate limit retry.
    /// The Alpaca options bars endpoint returns bars keyed by contract symbol.
    async fn fetch_bars_paginated(
        &self,
        underlying: &str,
        start: &str,
        end: &str,
    ) -> Result<HashMap<String, Vec<AlpacaOptionsBar>>, ProviderError> {
        let mut all_bars: HashMap<String, Vec<AlpacaOptionsBar>> = HashMap::new();
        let mut page_token: Option<String> = None;

        loop {
            let response = self
                .fetch_bars_page(underlying, start, end, page_token.as_deref())
                .await?;

            if let Some(bars_map) = response.bars {
                for (contract, bars) in bars_map {
                    all_bars.entry(contract).or_default().extend(bars);
                }
            }

            match response.next_page_token {
                Some(token) if !token.is_empty() => page_token = Some(token),
                _ => break,
            }
        }

        Ok(all_bars)
    }

    /// Fetch a single page of options bars with rate limit retry.
    async fn fetch_bars_page(
        &self,
        underlying: &str,
        start: &str,
        end: &str,
        page_token: Option<&str>,
    ) -> Result<AlpacaOptionsBarsResponse, ProviderError> {
        let mut retries = 0;

        loop {
            let mut request = self
                .client
                .get(format!("{}/bars", self.base_url))
                .header("APCA-API-KEY-ID", &self.api_key_id)
                .header("APCA-API-SECRET-KEY", &self.api_secret_key)
                .query(&[
                    ("symbols", underlying),
                    ("timeframe", "5Min"),
                    ("start", start),
                    ("end", end),
                    ("limit", "10000"),
                ]);

            if let Some(token) = page_token {
                request = request.query(&[("page_token", token)]);
            }

            let response = request.send().await?;

            if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                retries += 1;
                if retries > MAX_RATE_LIMIT_RETRIES {
                    return Err(ProviderError::RateLimited {
                        retry_after_secs: 60,
                    });
                }

                let wait_secs = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(60);

                warn!(
                    "{underlying}: rate limited, waiting {wait_secs}s (retry {retries}/{MAX_RATE_LIMIT_RETRIES})"
                );
                tokio::time::sleep(std::time::Duration::from_secs(wait_secs)).await;
                continue;
            }

            if !response.status().is_success() {
                let status = response.status().as_u16();
                let body = response.text().await.unwrap_or_default();
                return Err(ProviderError::Api {
                    status,
                    message: body,
                });
            }

            let body: AlpacaOptionsBarsResponse = response
                .json()
                .await
                .map_err(|e| ProviderError::Parse(format!("failed to parse response: {e}")))?;

            return Ok(body);
        }
    }
}

/// Convert raw Alpaca bars (keyed by contract symbol) into OptionsBar structs.
fn convert_bars(
    bars_map: HashMap<String, Vec<AlpacaOptionsBar>>,
) -> Result<Vec<OptionsBar>, ProviderError> {
    let mut result = Vec::new();

    for (contract_symbol, bars) in bars_map {
        let parsed = parse_occ_symbol(&contract_symbol).map_err(|e| {
            ProviderError::Parse(format!("failed to parse contract '{contract_symbol}': {e}"))
        })?;

        for bar in bars {
            // Skip zero-volume bars
            if bar.v == 0 {
                continue;
            }

            let timestamp = chrono::DateTime::parse_from_rfc3339(&bar.t)
                .map_err(|e| ProviderError::Parse(format!("invalid timestamp '{}': {e}", bar.t)))?
                .with_timezone(&Utc);

            result.push(OptionsBar {
                timestamp,
                contract_symbol: contract_symbol.clone(),
                underlying: parsed.underlying.clone(),
                expiration: parsed.expiration,
                strike: parsed.strike,
                contract_type: parsed.contract_type,
                open: bar.o,
                high: bar.h,
                low: bar.l,
                close: bar.c,
                volume: bar.v,
                trade_count: bar.n,
                vwap: bar.vw,
            });
        }
    }

    // Sort by (contract_symbol, timestamp)
    result.sort_by(|a, b| {
        a.contract_symbol
            .cmp(&b.contract_symbol)
            .then(a.timestamp.cmp(&b.timestamp))
    });

    Ok(result)
}

/// Group bars by their UTC date.
fn group_bars_by_date(bars: Vec<OptionsBar>) -> Vec<(NaiveDate, Vec<OptionsBar>)> {
    let mut by_date: BTreeMap<NaiveDate, Vec<OptionsBar>> = BTreeMap::new();
    for bar in bars {
        let date = bar.timestamp.date_naive();
        by_date.entry(date).or_default().push(bar);
    }
    // Sort within each date by (contract_symbol, timestamp)
    for bars in by_date.values_mut() {
        bars.sort_by(|a, b| {
            a.contract_symbol
                .cmp(&b.contract_symbol)
                .then(a.timestamp.cmp(&b.timestamp))
        });
    }
    by_date.into_iter().collect()
}

#[derive(Debug, Deserialize)]
struct AlpacaOptionsBarsResponse {
    bars: Option<HashMap<String, Vec<AlpacaOptionsBar>>>,
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AlpacaOptionsBar {
    /// Timestamp in RFC3339 format
    t: String,
    /// Open price
    o: Decimal,
    /// High price
    h: Decimal,
    /// Low price
    l: Decimal,
    /// Close price
    c: Decimal,
    /// Volume
    v: i64,
    /// Number of trades
    n: i64,
    /// Volume-weighted average price
    vw: Decimal,
}

#[async_trait]
impl OptionsBarProvider for AlpacaOptionsProvider {
    fn name(&self) -> &str {
        "alpaca"
    }

    async fn fetch_bars(
        &self,
        underlying: &str,
        date: NaiveDate,
    ) -> Result<Vec<OptionsBar>, ProviderError> {
        let start = Utc
            .from_utc_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
            .to_rfc3339();
        let end = Utc
            .from_utc_datetime(&date.succ_opt().unwrap().and_hms_opt(0, 0, 0).unwrap())
            .to_rfc3339();

        let bars_map = self.fetch_bars_paginated(underlying, &start, &end).await?;
        convert_bars(bars_map)
    }

    async fn fetch_bars_range(
        &self,
        underlying: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<(NaiveDate, Vec<OptionsBar>)>, ProviderError> {
        let start_rfc = Utc
            .from_utc_datetime(&start.and_hms_opt(0, 0, 0).unwrap())
            .to_rfc3339();
        let end_rfc = Utc
            .from_utc_datetime(&end.succ_opt().unwrap().and_hms_opt(0, 0, 0).unwrap())
            .to_rfc3339();

        info!("{underlying}: fetching options bars {start} to {end} from Alpaca");

        let bars_map = self
            .fetch_bars_paginated(underlying, &start_rfc, &end_rfc)
            .await?;

        let total_contracts: usize = bars_map.len();
        let total_bars: usize = bars_map.values().map(|v| v.len()).sum();
        debug!("{underlying}: received {total_bars} bars across {total_contracts} contracts");

        let bars = convert_bars(bars_map)?;
        debug!("{underlying}: {} bars after volume>0 filtering", bars.len());

        Ok(group_bars_by_date(bars))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use options_data_core::bar::ContractType;
    use rust_decimal_macros::dec;

    #[test]
    fn parse_alpaca_options_bar() {
        let bar = AlpacaOptionsBar {
            t: "2025-02-21T14:30:00Z".to_string(),
            o: dec!(1.25),
            h: dec!(1.50),
            l: dec!(1.10),
            c: dec!(1.40),
            v: 500,
            n: 42,
            vw: dec!(1.35),
        };

        let mut map = HashMap::new();
        map.insert("TQQQ250221C00090000".to_string(), vec![bar]);

        let result = convert_bars(map).unwrap();
        assert_eq!(result.len(), 1);

        let b = &result[0];
        assert_eq!(b.underlying, "TQQQ");
        assert_eq!(b.contract_type, ContractType::Call);
        assert_eq!(b.strike, dec!(90.000));
        assert_eq!(b.open, dec!(1.25));
        assert_eq!(b.volume, 500);
        assert_eq!(b.trade_count, 42);
        assert_eq!(b.vwap, dec!(1.35));
    }

    #[test]
    fn parse_alpaca_options_response_json() {
        let json = r#"{
            "bars": {
                "TQQQ250221C00090000": [
                    {"t": "2025-02-21T14:30:00Z", "o": 1.25, "h": 1.50, "l": 1.10, "c": 1.40, "v": 500, "n": 42, "vw": 1.35},
                    {"t": "2025-02-21T14:35:00Z", "o": 1.40, "h": 1.55, "l": 1.30, "c": 1.45, "v": 300, "n": 25, "vw": 1.42}
                ],
                "TQQQ250221P00085000": [
                    {"t": "2025-02-21T14:30:00Z", "o": 0.50, "h": 0.60, "l": 0.45, "c": 0.55, "v": 200, "n": 15, "vw": 0.52}
                ]
            },
            "next_page_token": null
        }"#;

        let response: AlpacaOptionsBarsResponse = serde_json::from_str(json).unwrap();
        let bars_map = response.bars.unwrap();
        assert_eq!(bars_map.len(), 2);
        assert_eq!(bars_map["TQQQ250221C00090000"].len(), 2);
        assert_eq!(bars_map["TQQQ250221P00085000"].len(), 1);
        assert!(response.next_page_token.is_none());
    }

    #[test]
    fn parse_alpaca_options_response_empty() {
        let json = r#"{"bars": null, "next_page_token": null}"#;
        let response: AlpacaOptionsBarsResponse = serde_json::from_str(json).unwrap();
        assert!(response.bars.is_none());
    }

    #[test]
    fn parse_alpaca_options_response_with_pagination() {
        let json = r#"{
            "bars": {
                "SPY260116P00400000": [
                    {"t": "2025-02-21T14:30:00Z", "o": 5.80, "h": 6.00, "l": 5.70, "c": 5.90, "v": 1200, "n": 85, "vw": 5.85}
                ]
            },
            "next_page_token": "abc123"
        }"#;

        let response: AlpacaOptionsBarsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.next_page_token.as_deref(), Some("abc123"));
    }

    #[test]
    fn zero_volume_bars_filtered() {
        let bars = vec![
            AlpacaOptionsBar {
                t: "2025-02-21T14:30:00Z".to_string(),
                o: dec!(1.25),
                h: dec!(1.50),
                l: dec!(1.10),
                c: dec!(1.40),
                v: 500,
                n: 42,
                vw: dec!(1.35),
            },
            AlpacaOptionsBar {
                t: "2025-02-21T14:35:00Z".to_string(),
                o: dec!(1.40),
                h: dec!(1.40),
                l: dec!(1.40),
                c: dec!(1.40),
                v: 0,
                n: 0,
                vw: dec!(0.00),
            },
        ];

        let mut map = HashMap::new();
        map.insert("TQQQ250221C00090000".to_string(), bars);

        let result = convert_bars(map).unwrap();
        assert_eq!(result.len(), 1); // Zero-volume bar filtered out
        assert_eq!(result[0].volume, 500);
    }

    #[test]
    fn group_by_date_works() {
        let bar1 = OptionsBar {
            timestamp: Utc.with_ymd_and_hms(2025, 2, 19, 14, 30, 0).unwrap(),
            contract_symbol: "TQQQ250221C00090000".to_string(),
            underlying: "TQQQ".to_string(),
            expiration: NaiveDate::from_ymd_opt(2025, 2, 21).unwrap(),
            strike: dec!(90.000),
            contract_type: ContractType::Call,
            open: dec!(1.25),
            high: dec!(1.50),
            low: dec!(1.10),
            close: dec!(1.40),
            volume: 500,
            trade_count: 42,
            vwap: dec!(1.35),
        };

        let mut bar2 = bar1.clone();
        bar2.timestamp = Utc.with_ymd_and_hms(2025, 2, 20, 14, 30, 0).unwrap();

        let mut bar3 = bar1.clone();
        bar3.timestamp = Utc.with_ymd_and_hms(2025, 2, 19, 14, 35, 0).unwrap();

        let grouped = group_bars_by_date(vec![bar1, bar2, bar3]);
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped[0].0, NaiveDate::from_ymd_opt(2025, 2, 19).unwrap());
        assert_eq!(grouped[0].1.len(), 2);
        assert!(grouped[0].1[0].timestamp < grouped[0].1[1].timestamp);
        assert_eq!(grouped[1].0, NaiveDate::from_ymd_opt(2025, 2, 20).unwrap());
        assert_eq!(grouped[1].1.len(), 1);
    }

    #[test]
    fn convert_bars_multiple_contracts() {
        let mut map = HashMap::new();
        map.insert(
            "TQQQ250221C00090000".to_string(),
            vec![AlpacaOptionsBar {
                t: "2025-02-21T14:30:00Z".to_string(),
                o: dec!(1.25),
                h: dec!(1.50),
                l: dec!(1.10),
                c: dec!(1.40),
                v: 500,
                n: 42,
                vw: dec!(1.35),
            }],
        );
        map.insert(
            "TQQQ250221P00085000".to_string(),
            vec![AlpacaOptionsBar {
                t: "2025-02-21T14:30:00Z".to_string(),
                o: dec!(0.50),
                h: dec!(0.60),
                l: dec!(0.45),
                c: dec!(0.55),
                v: 200,
                n: 15,
                vw: dec!(0.52),
            }],
        );

        let result = convert_bars(map).unwrap();
        assert_eq!(result.len(), 2);
        // Should be sorted by contract_symbol
        assert!(result[0].contract_symbol < result[1].contract_symbol);
    }
}
