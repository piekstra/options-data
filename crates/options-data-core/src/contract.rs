use chrono::NaiveDate;
use rust_decimal::Decimal;

use crate::bar::ContractType;
use crate::error::OptionsDataError;

/// Parsed components of an OCC options symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedContract {
    pub underlying: String,
    pub expiration: NaiveDate,
    pub contract_type: ContractType,
    pub strike: Decimal,
}

/// Parse an OCC-format options symbol into its components.
///
/// OCC format: `{UNDERLYING}{YYMMDD}{C|P}{STRIKE×1000}`
///
/// Examples:
/// - `TQQQ250221C00090000` → TQQQ, 2025-02-21, Call, 90.000
/// - `SPY260116P00400000` → SPY, 2026-01-16, Put, 400.000
/// - `O250321C00055000` → O, 2025-03-21, Call, 55.000
///
/// The strike is encoded as 8 digits representing the price × 1000
/// (5 digits integer + 3 digits decimal).
pub fn parse_occ_symbol(symbol: &str) -> Result<ParsedContract, OptionsDataError> {
    // Minimum length: 1 char underlying + 6 date + 1 C/P + 8 strike = 16
    if symbol.len() < 16 {
        return Err(OptionsDataError::InvalidContract(format!(
            "symbol too short: '{symbol}'"
        )));
    }

    // Parse from the right: last 8 chars = strike, then C/P, then 6 date, rest = underlying
    let len = symbol.len();

    // Strike: last 8 characters
    let strike_str = &symbol[len - 8..];
    let strike_raw: u64 = strike_str.parse().map_err(|_| {
        OptionsDataError::InvalidContract(format!("invalid strike in '{symbol}': '{strike_str}'"))
    })?;
    // Strike is encoded as price × 1000
    let strike = Decimal::new(strike_raw as i64, 3);

    // Contract type: character before strike
    let cp_char = symbol.as_bytes()[len - 9] as char;
    let contract_type = ContractType::from_char(cp_char).ok_or_else(|| {
        OptionsDataError::InvalidContract(format!(
            "invalid contract type in '{symbol}': '{cp_char}'"
        ))
    })?;

    // Expiration date: 6 characters before C/P
    let date_str = &symbol[len - 15..len - 9];
    let expiration = NaiveDate::parse_from_str(date_str, "%y%m%d").map_err(|_| {
        OptionsDataError::InvalidContract(format!("invalid expiration in '{symbol}': '{date_str}'"))
    })?;

    // Underlying: everything before the date
    let underlying = &symbol[..len - 15];
    if underlying.is_empty() {
        return Err(OptionsDataError::InvalidContract(format!(
            "empty underlying in '{symbol}'"
        )));
    }

    Ok(ParsedContract {
        underlying: underlying.to_string(),
        expiration,
        contract_type,
        strike,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn parse_tqqq_call() {
        let parsed = parse_occ_symbol("TQQQ250221C00090000").unwrap();
        assert_eq!(parsed.underlying, "TQQQ");
        assert_eq!(
            parsed.expiration,
            NaiveDate::from_ymd_opt(2025, 2, 21).unwrap()
        );
        assert_eq!(parsed.contract_type, ContractType::Call);
        assert_eq!(parsed.strike, dec!(90.000));
    }

    #[test]
    fn parse_spy_put() {
        let parsed = parse_occ_symbol("SPY260116P00400000").unwrap();
        assert_eq!(parsed.underlying, "SPY");
        assert_eq!(
            parsed.expiration,
            NaiveDate::from_ymd_opt(2026, 1, 16).unwrap()
        );
        assert_eq!(parsed.contract_type, ContractType::Put);
        assert_eq!(parsed.strike, dec!(400.000));
    }

    #[test]
    fn parse_single_char_underlying() {
        let parsed = parse_occ_symbol("O250321C00055000").unwrap();
        assert_eq!(parsed.underlying, "O");
        assert_eq!(
            parsed.expiration,
            NaiveDate::from_ymd_opt(2025, 3, 21).unwrap()
        );
        assert_eq!(parsed.contract_type, ContractType::Call);
        assert_eq!(parsed.strike, dec!(55.000));
    }

    #[test]
    fn parse_fractional_strike() {
        // Strike $12.50 = 00012500
        let parsed = parse_occ_symbol("AAPL250221C00012500").unwrap();
        assert_eq!(parsed.strike, dec!(12.500));
    }

    #[test]
    fn parse_high_strike() {
        // Strike $5000.000 = 05000000
        let parsed = parse_occ_symbol("SPX250221P05000000").unwrap();
        assert_eq!(parsed.underlying, "SPX");
        assert_eq!(parsed.strike, dec!(5000.000));
        assert_eq!(parsed.contract_type, ContractType::Put);
    }

    #[test]
    fn parse_too_short() {
        assert!(parse_occ_symbol("TQQQ").is_err());
    }

    #[test]
    fn parse_invalid_contract_type() {
        assert!(parse_occ_symbol("TQQQ250221X00090000").is_err());
    }

    #[test]
    fn parse_invalid_date() {
        assert!(parse_occ_symbol("TQQQ259921C00090000").is_err());
    }

    #[test]
    fn parse_invalid_strike() {
        assert!(parse_occ_symbol("TQQQ250221CABCDEFGH").is_err());
    }
}
