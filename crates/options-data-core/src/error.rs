use thiserror::Error;

#[derive(Debug, Error)]
pub enum OptionsDataError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parquet error: {0}")]
    Parquet(#[from] parquet::errors::ParquetError),

    #[error("Arrow error: {0}")]
    Arrow(#[from] arrow::error::ArrowError),

    #[error("No data found for {underlying} on {date}")]
    NoData {
        underlying: String,
        date: chrono::NaiveDate,
    },

    #[error("Invalid data: {0}")]
    InvalidData(String),

    #[error("Invalid contract symbol: {0}")]
    InvalidContract(String),
}
