use async_trait::async_trait;
use chrono::{Datelike, NaiveDate};
use options_data_core::bar::OptionsBar;

use crate::error::ProviderError;

/// Trait for fetching 5-minute options bar data from an external source.
#[async_trait]
pub trait OptionsBarProvider: Send + Sync {
    /// Provider name (for logging/display).
    fn name(&self) -> &str;

    /// Fetch 5-minute options bars for an underlying on a specific date.
    /// Returns bars sorted by (contract_symbol, timestamp).
    /// Returns an empty vec if the date is not a trading day.
    async fn fetch_bars(
        &self,
        underlying: &str,
        date: NaiveDate,
    ) -> Result<Vec<OptionsBar>, ProviderError>;

    /// Fetch 5-minute options bars for an underlying across a date range (inclusive).
    /// Returns bars grouped by date, sorted by (contract_symbol, timestamp) within each group.
    /// Providers should override this for efficient batch fetching.
    async fn fetch_bars_range(
        &self,
        underlying: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<(NaiveDate, Vec<OptionsBar>)>, ProviderError> {
        // Default: fetch day by day
        let mut results = Vec::new();
        let mut current = start;
        while current <= end {
            if current.weekday().number_from_monday() <= 5 {
                let bars = self.fetch_bars(underlying, current).await?;
                if !bars.is_empty() {
                    results.push((current, bars));
                }
            }
            current = current.succ_opt().unwrap();
        }
        Ok(results)
    }
}
