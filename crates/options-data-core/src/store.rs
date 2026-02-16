use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use rust_decimal::Decimal;

use crate::bar::{ContractType, OptionsBar};
use crate::error::OptionsDataError;
use crate::schema;
use crate::trading_calendar;

/// Filter for querying options bars along multiple dimensions.
#[derive(Debug, Clone, Default)]
pub struct OptionsFilter {
    /// Filter to a specific contract type (call or put).
    pub contract_type: Option<ContractType>,
    /// Filter to a specific expiration date.
    pub expiration: Option<NaiveDate>,
    /// Filter to expirations within a range (inclusive).
    pub expiration_range: Option<(NaiveDate, NaiveDate)>,
    /// Filter to strikes within a range (inclusive).
    pub strike_range: Option<(Decimal, Decimal)>,
    /// Filter to a specific contract symbol.
    pub contract_symbol: Option<String>,
    /// Filter to bars with at least this much volume.
    pub min_volume: Option<i64>,
}

impl OptionsFilter {
    /// Check if a bar matches this filter.
    pub fn matches(&self, bar: &OptionsBar) -> bool {
        if let Some(ct) = &self.contract_type
            && bar.contract_type != *ct
        {
            return false;
        }
        if let Some(exp) = &self.expiration
            && bar.expiration != *exp
        {
            return false;
        }
        if let Some((start, end)) = &self.expiration_range
            && (bar.expiration < *start || bar.expiration > *end)
        {
            return false;
        }
        if let Some((low, high)) = &self.strike_range
            && (bar.strike < *low || bar.strike > *high)
        {
            return false;
        }
        if let Some(sym) = &self.contract_symbol
            && bar.contract_symbol != *sym
        {
            return false;
        }
        if let Some(min_vol) = &self.min_volume
            && bar.volume < *min_vol
        {
            return false;
        }
        true
    }
}

/// Filesystem-backed store for 5-minute options bar data in Parquet format.
///
/// Directory layout: `{root}/data/{UNDERLYING}/{YYYY}/{MM}/{YYYY-MM-DD}.parquet`
pub struct OptionsBarStore {
    data_dir: PathBuf,
}

impl OptionsBarStore {
    /// Create a store rooted at the given directory.
    /// The `data/` subdirectory is used automatically.
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            data_dir: root.as_ref().join("data"),
        }
    }

    /// Create a store pointing directly at the data directory (no `data/` suffix).
    pub fn from_data_dir(data_dir: impl AsRef<Path>) -> Self {
        Self {
            data_dir: data_dir.as_ref().to_path_buf(),
        }
    }

    /// Path to the Parquet file for a given underlying and date.
    pub fn file_path(&self, underlying: &str, date: NaiveDate) -> PathBuf {
        self.data_dir
            .join(underlying)
            .join(date.format("%Y").to_string())
            .join(date.format("%m").to_string())
            .join(format!("{}.parquet", date.format("%Y-%m-%d")))
    }

    /// Check if data exists for an underlying on a given date.
    pub fn has_data(&self, underlying: &str, date: NaiveDate) -> bool {
        self.file_path(underlying, date).exists()
    }

    /// Find which weekdays in a range are missing data for an underlying.
    pub fn missing_dates(
        &self,
        underlying: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Vec<NaiveDate> {
        trading_calendar::weekdays(start, end)
            .into_iter()
            .filter(|d| !self.has_data(underlying, *d))
            .collect()
    }

    /// Write bars for a single date to a Parquet file.
    /// Creates parent directories as needed. Overwrites if file already exists.
    pub fn write_day(
        &self,
        underlying: &str,
        date: NaiveDate,
        bars: &[OptionsBar],
    ) -> Result<(), OptionsDataError> {
        let path = self.file_path(underlying, date);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        schema::write_parquet(&path, bars)
    }

    /// Read all bars for an underlying on a specific date.
    pub fn read_day(
        &self,
        underlying: &str,
        date: NaiveDate,
    ) -> Result<Vec<OptionsBar>, OptionsDataError> {
        let path = self.file_path(underlying, date);
        if !path.exists() {
            return Err(OptionsDataError::NoData {
                underlying: underlying.to_string(),
                date,
            });
        }
        schema::read_parquet(&path)
    }

    /// Read bars for an underlying across a date range (inclusive).
    /// Returns bars sorted by (contract_symbol, timestamp). Skips dates without data.
    pub fn read_range(
        &self,
        underlying: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<OptionsBar>, OptionsDataError> {
        let dates = trading_calendar::weekdays(start, end);
        let mut all_bars = Vec::new();

        for date in dates {
            if self.has_data(underlying, date) {
                let mut bars = schema::read_parquet(&self.file_path(underlying, date))?;
                all_bars.append(&mut bars);
            }
        }

        all_bars.sort_by(|a, b| {
            a.contract_symbol
                .cmp(&b.contract_symbol)
                .then(a.timestamp.cmp(&b.timestamp))
        });
        Ok(all_bars)
    }

    /// Read bars for a specific date, filtered by an OptionsFilter.
    pub fn read_day_filtered(
        &self,
        underlying: &str,
        date: NaiveDate,
        filter: &OptionsFilter,
    ) -> Result<Vec<OptionsBar>, OptionsDataError> {
        let bars = self.read_day(underlying, date)?;
        Ok(bars.into_iter().filter(|b| filter.matches(b)).collect())
    }

    /// Read bars across a date range, filtered by an OptionsFilter.
    pub fn read_range_filtered(
        &self,
        underlying: &str,
        start: NaiveDate,
        end: NaiveDate,
        filter: &OptionsFilter,
    ) -> Result<Vec<OptionsBar>, OptionsDataError> {
        let bars = self.read_range(underlying, start, end)?;
        Ok(bars.into_iter().filter(|b| filter.matches(b)).collect())
    }

    /// List all underlyings that have data in the store.
    pub fn list_underlyings(&self) -> Result<Vec<String>, OptionsDataError> {
        if !self.data_dir.exists() {
            return Ok(Vec::new());
        }

        let mut underlyings = Vec::new();
        for entry in std::fs::read_dir(&self.data_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir()
                && let Some(name) = entry.file_name().to_str()
            {
                underlyings.push(name.to_string());
            }
        }
        underlyings.sort();
        Ok(underlyings)
    }

    /// List all dates with data for a given underlying, sorted ascending.
    pub fn list_dates(&self, underlying: &str) -> Result<Vec<NaiveDate>, OptionsDataError> {
        let underlying_dir = self.data_dir.join(underlying);
        if !underlying_dir.exists() {
            return Ok(Vec::new());
        }

        let mut dates = Vec::new();

        for year_entry in std::fs::read_dir(&underlying_dir)? {
            let year_entry = year_entry?;
            if !year_entry.file_type()?.is_dir() {
                continue;
            }

            for month_entry in std::fs::read_dir(year_entry.path())? {
                let month_entry = month_entry?;
                if !month_entry.file_type()?.is_dir() {
                    continue;
                }

                for file_entry in std::fs::read_dir(month_entry.path())? {
                    let file_entry = file_entry?;
                    let file_name = file_entry.file_name();
                    let name = file_name.to_string_lossy();
                    if let Some(date_str) = name.strip_suffix(".parquet")
                        && let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
                    {
                        dates.push(date);
                    }
                }
            }
        }

        dates.sort();
        Ok(dates)
    }

    /// Get the date range (earliest, latest) for an underlying, or None if no data.
    pub fn date_range(
        &self,
        underlying: &str,
    ) -> Result<Option<(NaiveDate, NaiveDate)>, OptionsDataError> {
        let dates = self.list_dates(underlying)?;
        Ok(dates.first().copied().zip(dates.last().copied()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, TimeZone, Utc};
    use rust_decimal_macros::dec;

    use crate::bar::ContractType;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn make_bar(
        hour: u32,
        min: u32,
        contract_symbol: &str,
        contract_type: ContractType,
        strike: Decimal,
    ) -> OptionsBar {
        OptionsBar {
            timestamp: Utc.with_ymd_and_hms(2025, 2, 21, hour, min, 0).unwrap(),
            contract_symbol: contract_symbol.to_string(),
            underlying: "TQQQ".to_string(),
            expiration: date(2025, 2, 21),
            strike,
            contract_type,
            open: dec!(1.25),
            high: dec!(1.50),
            low: dec!(1.10),
            close: dec!(1.40),
            volume: 500,
            trade_count: 42,
            vwap: dec!(1.35),
        }
    }

    fn make_bars_for_date(year: i32, month: u32, day: u32) -> Vec<OptionsBar> {
        vec![
            OptionsBar {
                timestamp: Utc.with_ymd_and_hms(year, month, day, 14, 30, 0).unwrap(),
                contract_symbol: "TQQQ250221C00090000".to_string(),
                underlying: "TQQQ".to_string(),
                expiration: date(2025, 2, 21),
                strike: dec!(90.000),
                contract_type: ContractType::Call,
                open: dec!(1.25),
                high: dec!(1.50),
                low: dec!(1.10),
                close: dec!(1.40),
                volume: 500,
                trade_count: 42,
                vwap: dec!(1.35),
            },
            OptionsBar {
                timestamp: Utc.with_ymd_and_hms(year, month, day, 14, 35, 0).unwrap(),
                contract_symbol: "TQQQ250221P00085000".to_string(),
                underlying: "TQQQ".to_string(),
                expiration: date(2025, 2, 21),
                strike: dec!(85.000),
                contract_type: ContractType::Put,
                open: dec!(0.50),
                high: dec!(0.60),
                low: dec!(0.45),
                close: dec!(0.55),
                volume: 300,
                trade_count: 28,
                vwap: dec!(0.52),
            },
        ]
    }

    #[test]
    fn file_path_format() {
        let dir = tempfile::tempdir().unwrap();
        let store = OptionsBarStore::new(dir.path());
        let path = store.file_path("TQQQ", date(2025, 2, 21));
        let expected = dir.path().join("data/TQQQ/2025/02/2025-02-21.parquet");
        assert_eq!(path, expected);
    }

    #[test]
    fn has_data_false_before_write() {
        let dir = tempfile::tempdir().unwrap();
        let store = OptionsBarStore::new(dir.path());
        assert!(!store.has_data("TQQQ", date(2025, 2, 21)));
    }

    #[test]
    fn write_and_read_day() {
        let dir = tempfile::tempdir().unwrap();
        let store = OptionsBarStore::new(dir.path());
        let bars = make_bars_for_date(2025, 2, 21);

        store.write_day("TQQQ", date(2025, 2, 21), &bars).unwrap();
        assert!(store.has_data("TQQQ", date(2025, 2, 21)));

        let result = store.read_day("TQQQ", date(2025, 2, 21)).unwrap();
        assert_eq!(result, bars);
    }

    #[test]
    fn read_day_missing_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let store = OptionsBarStore::new(dir.path());
        let result = store.read_day("TQQQ", date(2025, 2, 21));
        assert!(result.is_err());
    }

    #[test]
    fn missing_dates_returns_weekdays_without_files() {
        let dir = tempfile::tempdir().unwrap();
        let store = OptionsBarStore::new(dir.path());

        // Wed Feb 19
        store
            .write_day("TQQQ", date(2025, 2, 19), &make_bars_for_date(2025, 2, 19))
            .unwrap();

        // Mon-Fri Feb 17-21: should be missing Mon, Tue, Thu, Fri
        let missing = store.missing_dates("TQQQ", date(2025, 2, 17), date(2025, 2, 21));
        assert_eq!(
            missing,
            vec![
                date(2025, 2, 17),
                date(2025, 2, 18),
                date(2025, 2, 20),
                date(2025, 2, 21),
            ]
        );
    }

    #[test]
    fn read_range_multiple_days() {
        let dir = tempfile::tempdir().unwrap();
        let store = OptionsBarStore::new(dir.path());

        let bars_19 = make_bars_for_date(2025, 2, 19);
        let bars_20 = make_bars_for_date(2025, 2, 20);

        store
            .write_day("TQQQ", date(2025, 2, 19), &bars_19)
            .unwrap();
        store
            .write_day("TQQQ", date(2025, 2, 20), &bars_20)
            .unwrap();

        let result = store
            .read_range("TQQQ", date(2025, 2, 19), date(2025, 2, 20))
            .unwrap();
        assert_eq!(result.len(), 4);
    }

    #[test]
    fn read_range_skips_missing_dates() {
        let dir = tempfile::tempdir().unwrap();
        let store = OptionsBarStore::new(dir.path());

        store
            .write_day("TQQQ", date(2025, 2, 19), &make_bars_for_date(2025, 2, 19))
            .unwrap();

        let result = store
            .read_range("TQQQ", date(2025, 2, 17), date(2025, 2, 21))
            .unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn filter_by_contract_type() {
        let dir = tempfile::tempdir().unwrap();
        let store = OptionsBarStore::new(dir.path());

        store
            .write_day("TQQQ", date(2025, 2, 21), &make_bars_for_date(2025, 2, 21))
            .unwrap();

        let filter = OptionsFilter {
            contract_type: Some(ContractType::Call),
            ..Default::default()
        };

        let result = store
            .read_day_filtered("TQQQ", date(2025, 2, 21), &filter)
            .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].contract_type, ContractType::Call);
    }

    #[test]
    fn filter_by_strike_range() {
        let dir = tempfile::tempdir().unwrap();
        let store = OptionsBarStore::new(dir.path());

        store
            .write_day("TQQQ", date(2025, 2, 21), &make_bars_for_date(2025, 2, 21))
            .unwrap();

        let filter = OptionsFilter {
            strike_range: Some((dec!(88.000), dec!(95.000))),
            ..Default::default()
        };

        let result = store
            .read_day_filtered("TQQQ", date(2025, 2, 21), &filter)
            .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].strike, dec!(90.000));
    }

    #[test]
    fn filter_by_min_volume() {
        let dir = tempfile::tempdir().unwrap();
        let store = OptionsBarStore::new(dir.path());

        store
            .write_day("TQQQ", date(2025, 2, 21), &make_bars_for_date(2025, 2, 21))
            .unwrap();

        let filter = OptionsFilter {
            min_volume: Some(400),
            ..Default::default()
        };

        let result = store
            .read_day_filtered("TQQQ", date(2025, 2, 21), &filter)
            .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].volume, 500);
    }

    #[test]
    fn filter_by_contract_symbol() {
        let dir = tempfile::tempdir().unwrap();
        let store = OptionsBarStore::new(dir.path());

        store
            .write_day("TQQQ", date(2025, 2, 21), &make_bars_for_date(2025, 2, 21))
            .unwrap();

        let filter = OptionsFilter {
            contract_symbol: Some("TQQQ250221P00085000".to_string()),
            ..Default::default()
        };

        let result = store
            .read_day_filtered("TQQQ", date(2025, 2, 21), &filter)
            .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].contract_type, ContractType::Put);
    }

    #[test]
    fn filter_combined() {
        let dir = tempfile::tempdir().unwrap();
        let store = OptionsBarStore::new(dir.path());

        let mut bars = make_bars_for_date(2025, 2, 21);
        bars.push(make_bar(
            14,
            30,
            "TQQQ250221C00095000",
            ContractType::Call,
            dec!(95.000),
        ));

        store.write_day("TQQQ", date(2025, 2, 21), &bars).unwrap();

        let filter = OptionsFilter {
            contract_type: Some(ContractType::Call),
            strike_range: Some((dec!(88.000), dec!(92.000))),
            ..Default::default()
        };

        let result = store
            .read_day_filtered("TQQQ", date(2025, 2, 21), &filter)
            .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].strike, dec!(90.000));
    }

    #[test]
    fn read_range_filtered() {
        let dir = tempfile::tempdir().unwrap();
        let store = OptionsBarStore::new(dir.path());

        store
            .write_day("TQQQ", date(2025, 2, 19), &make_bars_for_date(2025, 2, 19))
            .unwrap();
        store
            .write_day("TQQQ", date(2025, 2, 20), &make_bars_for_date(2025, 2, 20))
            .unwrap();

        let filter = OptionsFilter {
            contract_type: Some(ContractType::Put),
            ..Default::default()
        };

        let result = store
            .read_range_filtered("TQQQ", date(2025, 2, 19), date(2025, 2, 20), &filter)
            .unwrap();
        assert_eq!(result.len(), 2); // One put per day
        assert!(result.iter().all(|b| b.contract_type == ContractType::Put));
    }

    #[test]
    fn list_underlyings() {
        let dir = tempfile::tempdir().unwrap();
        let store = OptionsBarStore::new(dir.path());

        store
            .write_day("TQQQ", date(2025, 2, 21), &make_bars_for_date(2025, 2, 21))
            .unwrap();

        let mut spy_bars = make_bars_for_date(2025, 2, 21);
        for bar in &mut spy_bars {
            bar.underlying = "SPY".to_string();
        }
        store
            .write_day("SPY", date(2025, 2, 21), &spy_bars)
            .unwrap();

        let underlyings = store.list_underlyings().unwrap();
        assert_eq!(underlyings, vec!["SPY", "TQQQ"]);
    }

    #[test]
    fn list_underlyings_empty_store() {
        let dir = tempfile::tempdir().unwrap();
        let store = OptionsBarStore::new(dir.path());
        let underlyings = store.list_underlyings().unwrap();
        assert!(underlyings.is_empty());
    }

    #[test]
    fn list_dates_and_date_range() {
        let dir = tempfile::tempdir().unwrap();
        let store = OptionsBarStore::new(dir.path());

        store
            .write_day("TQQQ", date(2025, 2, 19), &make_bars_for_date(2025, 2, 19))
            .unwrap();
        store
            .write_day("TQQQ", date(2025, 2, 20), &make_bars_for_date(2025, 2, 20))
            .unwrap();
        store
            .write_day("TQQQ", date(2025, 3, 3), &make_bars_for_date(2025, 3, 3))
            .unwrap();

        let dates = store.list_dates("TQQQ").unwrap();
        assert_eq!(
            dates,
            vec![date(2025, 2, 19), date(2025, 2, 20), date(2025, 3, 3)]
        );

        let range = store.date_range("TQQQ").unwrap();
        assert_eq!(range, Some((date(2025, 2, 19), date(2025, 3, 3))));
    }

    #[test]
    fn date_range_empty() {
        let dir = tempfile::tempdir().unwrap();
        let store = OptionsBarStore::new(dir.path());
        assert_eq!(store.date_range("TQQQ").unwrap(), None);
    }

    #[test]
    fn write_day_creates_directories() {
        let dir = tempfile::tempdir().unwrap();
        let store = OptionsBarStore::new(dir.path());

        store
            .write_day("TQQQ", date(2025, 2, 21), &make_bars_for_date(2025, 2, 21))
            .unwrap();

        assert!(dir.path().join("data/TQQQ/2025/02").exists());
    }

    #[test]
    fn write_day_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let store = OptionsBarStore::new(dir.path());
        let d = date(2025, 2, 21);

        let bars1 = make_bars_for_date(2025, 2, 21);
        store.write_day("TQQQ", d, &bars1).unwrap();

        let bars2 = vec![bars1[0].clone()];
        store.write_day("TQQQ", d, &bars2).unwrap();

        let result = store.read_day("TQQQ", d).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn filter_expiration_range() {
        let dir = tempfile::tempdir().unwrap();
        let store = OptionsBarStore::new(dir.path());

        let mut bars = make_bars_for_date(2025, 2, 21);
        // Add a bar with a different expiration
        let mut far_exp_bar = bars[0].clone();
        far_exp_bar.expiration = date(2025, 6, 20);
        far_exp_bar.contract_symbol = "TQQQ250620C00090000".to_string();
        bars.push(far_exp_bar);

        store.write_day("TQQQ", date(2025, 2, 21), &bars).unwrap();

        let filter = OptionsFilter {
            expiration_range: Some((date(2025, 2, 1), date(2025, 3, 1))),
            ..Default::default()
        };

        let result = store
            .read_day_filtered("TQQQ", date(2025, 2, 21), &filter)
            .unwrap();
        // Only bars with expiration in Feb should match
        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|b| b.expiration <= date(2025, 3, 1)));
    }
}
