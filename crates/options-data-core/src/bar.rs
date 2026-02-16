use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Option contract type: call or put.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContractType {
    Call,
    Put,
}

impl ContractType {
    /// Single-character code used in OCC symbols and Parquet storage.
    pub fn as_char(&self) -> char {
        match self {
            ContractType::Call => 'C',
            ContractType::Put => 'P',
        }
    }

    /// Parse from single-character code.
    pub fn from_char(c: char) -> Option<ContractType> {
        match c {
            'C' => Some(ContractType::Call),
            'P' => Some(ContractType::Put),
            _ => None,
        }
    }
}

impl std::fmt::Display for ContractType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_char())
    }
}

/// A single 5-minute options bar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionsBar {
    /// Bar start time (UTC).
    pub timestamp: DateTime<Utc>,
    /// OCC contract symbol (e.g., "TQQQ250221C00090000").
    pub contract_symbol: String,
    /// Underlying ticker (e.g., "TQQQ").
    pub underlying: String,
    /// Contract expiration date.
    pub expiration: NaiveDate,
    /// Strike price.
    pub strike: Decimal,
    /// Call or put.
    pub contract_type: ContractType,
    /// Open price.
    pub open: Decimal,
    /// High price.
    pub high: Decimal,
    /// Low price.
    pub low: Decimal,
    /// Close price.
    pub close: Decimal,
    /// Volume.
    pub volume: i64,
    /// Number of trades in the bar.
    pub trade_count: i64,
    /// Volume-weighted average price.
    pub vwap: Decimal,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use rust_decimal_macros::dec;

    #[test]
    fn contract_type_roundtrip() {
        assert_eq!(ContractType::from_char('C'), Some(ContractType::Call));
        assert_eq!(ContractType::from_char('P'), Some(ContractType::Put));
        assert_eq!(ContractType::from_char('X'), None);
        assert_eq!(ContractType::Call.as_char(), 'C');
        assert_eq!(ContractType::Put.as_char(), 'P');
    }

    #[test]
    fn options_bar_construction() {
        let bar = OptionsBar {
            timestamp: Utc.with_ymd_and_hms(2025, 2, 21, 14, 30, 0).unwrap(),
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

        assert_eq!(bar.underlying, "TQQQ");
        assert_eq!(bar.contract_type, ContractType::Call);
        assert_eq!(bar.strike, dec!(90.000));
    }
}
