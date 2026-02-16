use std::path::Path;
use std::sync::Arc;

use arrow::array::{ArrayRef, Date32Array, Int64Array, StringArray, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;

use crate::bar::{ContractType, OptionsBar};
use crate::error::OptionsDataError;

pub fn options_bar_schema() -> Schema {
    Schema::new(vec![
        Field::new(
            "timestamp",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
        Field::new("contract_symbol", DataType::Utf8, false),
        Field::new("underlying", DataType::Utf8, false),
        Field::new("expiration", DataType::Date32, false),
        Field::new("strike", DataType::Utf8, false),
        Field::new("contract_type", DataType::Utf8, false),
        Field::new("open", DataType::Utf8, false),
        Field::new("high", DataType::Utf8, false),
        Field::new("low", DataType::Utf8, false),
        Field::new("close", DataType::Utf8, false),
        Field::new("volume", DataType::Int64, false),
        Field::new("trade_count", DataType::Int64, false),
        Field::new("vwap", DataType::Utf8, false),
    ])
}

pub fn bars_to_record_batch(bars: &[OptionsBar]) -> Result<RecordBatch, OptionsDataError> {
    let schema = Arc::new(options_bar_schema());

    let timestamps: Vec<i64> = bars
        .iter()
        .map(|b| b.timestamp.timestamp_micros())
        .collect();

    let contract_symbols: Vec<String> = bars.iter().map(|b| b.contract_symbol.clone()).collect();
    let underlyings: Vec<String> = bars.iter().map(|b| b.underlying.clone()).collect();

    // Date32 is days since Unix epoch
    let expirations: Vec<i32> = bars
        .iter()
        .map(|b| {
            (b.expiration - chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap()).num_days() as i32
        })
        .collect();

    let strikes: Vec<String> = bars.iter().map(|b| b.strike.to_string()).collect();
    let contract_types: Vec<String> = bars
        .iter()
        .map(|b| b.contract_type.as_char().to_string())
        .collect();
    let opens: Vec<String> = bars.iter().map(|b| b.open.to_string()).collect();
    let highs: Vec<String> = bars.iter().map(|b| b.high.to_string()).collect();
    let lows: Vec<String> = bars.iter().map(|b| b.low.to_string()).collect();
    let closes: Vec<String> = bars.iter().map(|b| b.close.to_string()).collect();
    let volumes: Vec<i64> = bars.iter().map(|b| b.volume).collect();
    let trade_counts: Vec<i64> = bars.iter().map(|b| b.trade_count).collect();
    let vwaps: Vec<String> = bars.iter().map(|b| b.vwap.to_string()).collect();

    let columns: Vec<ArrayRef> = vec![
        Arc::new(TimestampMicrosecondArray::from(timestamps).with_timezone("UTC")),
        Arc::new(StringArray::from(
            contract_symbols
                .iter()
                .map(|s| s.as_ref())
                .collect::<Vec<&str>>(),
        )),
        Arc::new(StringArray::from(
            underlyings
                .iter()
                .map(|s| s.as_ref())
                .collect::<Vec<&str>>(),
        )),
        Arc::new(Date32Array::from(expirations)),
        Arc::new(StringArray::from(
            strikes.iter().map(|s| s.as_ref()).collect::<Vec<&str>>(),
        )),
        Arc::new(StringArray::from(
            contract_types
                .iter()
                .map(|s| s.as_ref())
                .collect::<Vec<&str>>(),
        )),
        Arc::new(StringArray::from(
            opens.iter().map(|s| s.as_ref()).collect::<Vec<&str>>(),
        )),
        Arc::new(StringArray::from(
            highs.iter().map(|s| s.as_ref()).collect::<Vec<&str>>(),
        )),
        Arc::new(StringArray::from(
            lows.iter().map(|s| s.as_ref()).collect::<Vec<&str>>(),
        )),
        Arc::new(StringArray::from(
            closes.iter().map(|s| s.as_ref()).collect::<Vec<&str>>(),
        )),
        Arc::new(Int64Array::from(volumes)),
        Arc::new(Int64Array::from(trade_counts)),
        Arc::new(StringArray::from(
            vwaps.iter().map(|s| s.as_ref()).collect::<Vec<&str>>(),
        )),
    ];

    Ok(RecordBatch::try_new(schema, columns)?)
}

pub fn record_batch_to_bars(batch: &RecordBatch) -> Result<Vec<OptionsBar>, OptionsDataError> {
    let timestamps = batch
        .column(0)
        .as_any()
        .downcast_ref::<TimestampMicrosecondArray>()
        .ok_or_else(|| OptionsDataError::InvalidData("expected timestamp column".into()))?;

    let contract_symbols = batch
        .column(1)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| OptionsDataError::InvalidData("expected contract_symbol column".into()))?;

    let underlyings = batch
        .column(2)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| OptionsDataError::InvalidData("expected underlying column".into()))?;

    let expirations = batch
        .column(3)
        .as_any()
        .downcast_ref::<Date32Array>()
        .ok_or_else(|| OptionsDataError::InvalidData("expected expiration column".into()))?;

    let strikes = batch
        .column(4)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| OptionsDataError::InvalidData("expected strike column".into()))?;

    let contract_types = batch
        .column(5)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| OptionsDataError::InvalidData("expected contract_type column".into()))?;

    let opens = batch
        .column(6)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| OptionsDataError::InvalidData("expected open column".into()))?;

    let highs = batch
        .column(7)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| OptionsDataError::InvalidData("expected high column".into()))?;

    let lows = batch
        .column(8)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| OptionsDataError::InvalidData("expected low column".into()))?;

    let closes = batch
        .column(9)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| OptionsDataError::InvalidData("expected close column".into()))?;

    let volumes = batch
        .column(10)
        .as_any()
        .downcast_ref::<Int64Array>()
        .ok_or_else(|| OptionsDataError::InvalidData("expected volume column".into()))?;

    let trade_counts = batch
        .column(11)
        .as_any()
        .downcast_ref::<Int64Array>()
        .ok_or_else(|| OptionsDataError::InvalidData("expected trade_count column".into()))?;

    let vwaps = batch
        .column(12)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| OptionsDataError::InvalidData("expected vwap column".into()))?;

    let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
    let mut bars = Vec::with_capacity(batch.num_rows());

    for i in 0..batch.num_rows() {
        let micros = timestamps.value(i);
        let timestamp = chrono::DateTime::from_timestamp_micros(micros)
            .ok_or_else(|| OptionsDataError::InvalidData(format!("invalid timestamp: {micros}")))?;

        let expiration_days = expirations.value(i);
        let expiration = epoch + chrono::Duration::days(expiration_days as i64);

        let ct_str = contract_types.value(i);
        let contract_type = ct_str
            .chars()
            .next()
            .and_then(ContractType::from_char)
            .ok_or_else(|| {
                OptionsDataError::InvalidData(format!("invalid contract type: '{ct_str}'"))
            })?;

        let strike = strikes
            .value(i)
            .parse()
            .map_err(|e| OptionsDataError::InvalidData(format!("invalid strike: {e}")))?;

        let open = opens
            .value(i)
            .parse()
            .map_err(|e| OptionsDataError::InvalidData(format!("invalid open: {e}")))?;
        let high = highs
            .value(i)
            .parse()
            .map_err(|e| OptionsDataError::InvalidData(format!("invalid high: {e}")))?;
        let low = lows
            .value(i)
            .parse()
            .map_err(|e| OptionsDataError::InvalidData(format!("invalid low: {e}")))?;
        let close = closes
            .value(i)
            .parse()
            .map_err(|e| OptionsDataError::InvalidData(format!("invalid close: {e}")))?;
        let vwap = vwaps
            .value(i)
            .parse()
            .map_err(|e| OptionsDataError::InvalidData(format!("invalid vwap: {e}")))?;

        bars.push(OptionsBar {
            timestamp,
            contract_symbol: contract_symbols.value(i).to_string(),
            underlying: underlyings.value(i).to_string(),
            expiration,
            strike,
            contract_type,
            open,
            high,
            low,
            close,
            volume: volumes.value(i),
            trade_count: trade_counts.value(i),
            vwap,
        });
    }

    Ok(bars)
}

pub fn write_parquet(path: &Path, bars: &[OptionsBar]) -> Result<(), OptionsDataError> {
    let batch = bars_to_record_batch(bars)?;

    let props = WriterProperties::builder()
        .set_compression(Compression::SNAPPY)
        .build();

    let file = std::fs::File::create(path)?;
    let mut writer = ArrowWriter::try_new(file, batch.schema(), Some(props))?;
    writer.write(&batch)?;
    writer.close()?;

    Ok(())
}

pub fn read_parquet(path: &Path) -> Result<Vec<OptionsBar>, OptionsDataError> {
    let file = std::fs::File::open(path)?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
    let reader = builder.build()?;

    let mut all_bars = Vec::new();
    for batch in reader {
        let batch = batch?;
        let mut bars = record_batch_to_bars(&batch)?;
        all_bars.append(&mut bars);
    }

    Ok(all_bars)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, TimeZone, Utc};
    use rust_decimal_macros::dec;

    use crate::bar::ContractType;

    fn sample_bars() -> Vec<OptionsBar> {
        vec![
            OptionsBar {
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
            },
            OptionsBar {
                timestamp: Utc.with_ymd_and_hms(2025, 2, 21, 14, 35, 0).unwrap(),
                contract_symbol: "SPY260116P00400000".to_string(),
                underlying: "SPY".to_string(),
                expiration: NaiveDate::from_ymd_opt(2026, 1, 16).unwrap(),
                strike: dec!(400.000),
                contract_type: ContractType::Put,
                open: dec!(5.80),
                high: dec!(6.00),
                low: dec!(5.70),
                close: dec!(5.90),
                volume: 1200,
                trade_count: 85,
                vwap: dec!(5.85),
            },
        ]
    }

    #[test]
    fn record_batch_roundtrip() {
        let bars = sample_bars();
        let batch = bars_to_record_batch(&bars).unwrap();
        let result = record_batch_to_bars(&batch).unwrap();
        assert_eq!(bars, result);
    }

    #[test]
    fn empty_bars_roundtrip() {
        let bars: Vec<OptionsBar> = vec![];
        let batch = bars_to_record_batch(&bars).unwrap();
        assert_eq!(batch.num_rows(), 0);
        let result = record_batch_to_bars(&batch).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn parquet_file_roundtrip() {
        let bars = sample_bars();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.parquet");

        write_parquet(&path, &bars).unwrap();
        let result = read_parquet(&path).unwrap();
        assert_eq!(bars, result);
    }

    #[test]
    fn decimal_precision_preserved() {
        let bar = OptionsBar {
            timestamp: Utc.with_ymd_and_hms(2025, 2, 21, 14, 30, 0).unwrap(),
            contract_symbol: "AAPL250221C00012500".to_string(),
            underlying: "AAPL".to_string(),
            expiration: NaiveDate::from_ymd_opt(2025, 2, 21).unwrap(),
            strike: dec!(12.500),
            contract_type: ContractType::Call,
            open: dec!(0.01),
            high: dec!(99999.9999),
            low: dec!(0.0001),
            close: dec!(123.4567),
            volume: 1,
            trade_count: 1,
            vwap: dec!(50.1234),
        };

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("precision.parquet");

        write_parquet(&path, &[bar.clone()]).unwrap();
        let result = read_parquet(&path).unwrap();

        assert_eq!(result[0].open, dec!(0.01));
        assert_eq!(result[0].high, dec!(99999.9999));
        assert_eq!(result[0].low, dec!(0.0001));
        assert_eq!(result[0].close, dec!(123.4567));
        assert_eq!(result[0].strike, dec!(12.500));
        assert_eq!(result[0].vwap, dec!(50.1234));
    }

    #[test]
    fn expiration_date_preserved() {
        let bar = OptionsBar {
            timestamp: Utc.with_ymd_and_hms(2025, 2, 21, 14, 30, 0).unwrap(),
            contract_symbol: "SPY280121C00500000".to_string(),
            underlying: "SPY".to_string(),
            expiration: NaiveDate::from_ymd_opt(2028, 1, 21).unwrap(),
            strike: dec!(500.000),
            contract_type: ContractType::Call,
            open: dec!(10.00),
            high: dec!(10.00),
            low: dec!(10.00),
            close: dec!(10.00),
            volume: 1,
            trade_count: 1,
            vwap: dec!(10.00),
        };

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("expiry.parquet");

        write_parquet(&path, &[bar.clone()]).unwrap();
        let result = read_parquet(&path).unwrap();

        assert_eq!(
            result[0].expiration,
            NaiveDate::from_ymd_opt(2028, 1, 21).unwrap()
        );
    }
}
