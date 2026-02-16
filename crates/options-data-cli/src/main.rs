use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::NaiveDate;
use clap::{Parser, Subcommand};
use options_data_core::store::OptionsBarStore;
use options_data_providers::alpaca::AlpacaOptionsProvider;
use options_data_providers::provider::OptionsBarProvider;
use tracing::{info, warn};

/// Default underlying symbols to populate when using `-s all`.
const DEFAULT_UNDERLYINGS: &[&str] = &[
    "AMUU", "AMZU", "BTCL", "CURE", "DPST", "DRN", "FAS", "GGLL", "HOOD", "LABU", "METU", "NAIL",
    "NVDX", "O", "RETL", "ROBN", "SOXL", "SPXL", "SPY", "SVIX", "TQQQ", "TSLT", "UVIX", "VIX",
];

#[derive(Parser)]
#[command(
    name = "options-data",
    about = "Populate and manage 5-minute options bar data"
)]
struct Cli {
    /// Root directory for data storage (default: current directory)
    #[arg(long, default_value = ".")]
    data_dir: PathBuf,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Populate options bar data for underlyings
    Populate {
        /// Underlyings to fetch (comma-separated, or "all" for default list)
        #[arg(short = 's', long, value_delimiter = ',', required = true)]
        underlyings: Vec<String>,

        /// Start date (YYYY-MM-DD)
        #[arg(long)]
        start: NaiveDate,

        /// End date (YYYY-MM-DD, defaults to yesterday)
        #[arg(long)]
        end: Option<NaiveDate>,

        /// Force re-download even if data exists
        #[arg(long)]
        force: bool,
    },

    /// Show what data exists in the store
    Status {
        /// Filter by underlying (shows all if omitted)
        #[arg(short = 's', long)]
        underlying: Option<String>,
    },

    /// Validate Parquet files and report issues
    Validate {
        /// Underlyings to validate (all if omitted, comma-separated)
        #[arg(short = 's', long, value_delimiter = ',')]
        underlyings: Option<Vec<String>>,
    },
}

fn create_provider() -> Result<Box<dyn OptionsBarProvider>> {
    Ok(Box::new(
        AlpacaOptionsProvider::from_env().context("failed to create Alpaca options provider")?,
    ))
}

/// Resolve the underlyings list, expanding "all" to the default set.
fn resolve_underlyings(underlyings: &[String]) -> Vec<String> {
    if underlyings.len() == 1 && underlyings[0].eq_ignore_ascii_case("all") {
        DEFAULT_UNDERLYINGS.iter().map(|s| s.to_string()).collect()
    } else {
        underlyings.iter().map(|s| s.to_uppercase()).collect()
    }
}

/// Find contiguous date ranges from a sorted list of dates.
/// Groups consecutive weekdays together to minimize API calls.
fn contiguous_ranges(dates: &[NaiveDate]) -> Vec<(NaiveDate, NaiveDate)> {
    if dates.is_empty() {
        return Vec::new();
    }

    let mut ranges = Vec::new();
    let mut range_start = dates[0];
    let mut prev = dates[0];

    for &date in &dates[1..] {
        // Allow gaps of up to 3 days (weekend + possible holiday)
        let gap = (date - prev).num_days();
        if gap > 4 {
            ranges.push((range_start, prev));
            range_start = date;
        }
        prev = date;
    }
    ranges.push((range_start, prev));
    ranges
}

async fn cmd_populate(
    store: &OptionsBarStore,
    underlyings: &[String],
    start: NaiveDate,
    end: NaiveDate,
    force: bool,
) -> Result<()> {
    let provider = create_provider()?;
    info!("Using provider: {}", provider.name());

    for underlying in underlyings {
        let dates_to_fetch = if force {
            options_data_core::trading_calendar::weekdays(start, end)
        } else {
            store.missing_dates(underlying, start, end)
        };

        if dates_to_fetch.is_empty() {
            info!("{underlying}: all data present, skipping");
            continue;
        }

        info!(
            "{underlying}: {} missing date(s) from {} to {}",
            dates_to_fetch.len(),
            dates_to_fetch.first().unwrap(),
            dates_to_fetch.last().unwrap(),
        );

        let ranges = contiguous_ranges(&dates_to_fetch);
        info!("{underlying}: fetching in {} range(s)", ranges.len());

        for (range_start, range_end) in &ranges {
            match provider
                .fetch_bars_range(underlying, *range_start, *range_end)
                .await
            {
                Ok(day_groups) => {
                    let mut days_written = 0;
                    let mut total_bars = 0;
                    for (date, bars) in &day_groups {
                        if bars.is_empty() {
                            continue;
                        }
                        store
                            .write_day(underlying, *date, bars)
                            .with_context(|| format!("failed to write {underlying} {date}"))?;
                        days_written += 1;
                        total_bars += bars.len();
                    }
                    info!(
                        "{underlying}: {range_start} to {range_end}: wrote {total_bars} bar(s) across {days_written} day(s)"
                    );
                }
                Err(e) => {
                    warn!("{underlying}: {range_start} to {range_end}: fetch failed: {e}");
                }
            }
        }
    }

    Ok(())
}

fn cmd_status(store: &OptionsBarStore, underlying: Option<&str>) -> Result<()> {
    let underlyings = match underlying {
        Some(s) => vec![s.to_uppercase()],
        None => store
            .list_underlyings()
            .context("failed to list underlyings")?,
    };

    if underlyings.is_empty() {
        println!("No data in store.");
        return Ok(());
    }

    for sym in &underlyings {
        let dates = store
            .list_dates(sym)
            .with_context(|| format!("failed to list dates for {sym}"))?;

        if dates.is_empty() {
            println!("{sym}: no data");
            continue;
        }

        let first = dates.first().unwrap();
        let last = dates.last().unwrap();
        println!("{sym}: {} day(s), {first} to {last}", dates.len());
    }

    Ok(())
}

fn cmd_validate(store: &OptionsBarStore, underlyings: Option<&[String]>) -> Result<()> {
    let all_underlyings = store
        .list_underlyings()
        .context("failed to list underlyings")?;

    let to_check: Vec<&str> = match underlyings {
        Some(list) => list.iter().map(|s| s.as_str()).collect(),
        None => all_underlyings.iter().map(|s| s.as_str()).collect(),
    };

    if to_check.is_empty() {
        println!("No data to validate.");
        return Ok(());
    }

    let mut issues = 0;

    for sym in &to_check {
        let dates = store
            .list_dates(sym)
            .with_context(|| format!("failed to list dates for {sym}"))?;

        for date in &dates {
            match store.read_day(sym, *date) {
                Ok(bars) => {
                    if bars.is_empty() {
                        println!("WARN: {sym} {date}: empty file");
                        issues += 1;
                        continue;
                    }

                    // Check that bars are sorted by (contract_symbol, timestamp)
                    for i in 1..bars.len() {
                        let prev = &bars[i - 1];
                        let curr = &bars[i];
                        let order = prev
                            .contract_symbol
                            .cmp(&curr.contract_symbol)
                            .then(prev.timestamp.cmp(&curr.timestamp));
                        if order == std::cmp::Ordering::Greater {
                            println!("WARN: {sym} {date}: bars not sorted at index {i}");
                            issues += 1;
                            break;
                        }
                    }

                    // Check volume
                    let zero_volume = bars.iter().filter(|b| b.volume == 0).count();
                    if zero_volume > 0 {
                        println!("WARN: {sym} {date}: {zero_volume} bar(s) with zero volume");
                        issues += 1;
                    }
                }
                Err(e) => {
                    println!("ERROR: {sym} {date}: failed to read: {e}");
                    issues += 1;
                }
            }
        }
    }

    if issues == 0 {
        println!("All files valid.");
    } else {
        println!("{issues} issue(s) found.");
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&cli.log_level)),
        )
        .init();

    let store = OptionsBarStore::new(&cli.data_dir);

    match &cli.command {
        Commands::Populate {
            underlyings,
            start,
            end,
            force,
        } => {
            let resolved = resolve_underlyings(underlyings);
            let end_date = end
                .unwrap_or_else(|| (chrono::Utc::now() - chrono::Duration::days(1)).date_naive());
            cmd_populate(&store, &resolved, *start, end_date, *force).await?;
        }
        Commands::Status { underlying } => {
            cmd_status(&store, underlying.as_deref())?;
        }
        Commands::Validate { underlyings } => {
            cmd_validate(&store, underlyings.as_deref())?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parse_populate_args() {
        let cli = Cli::try_parse_from([
            "options-data",
            "populate",
            "-s",
            "TQQQ,SPY",
            "--start",
            "2025-01-01",
            "--end",
            "2025-01-31",
        ])
        .unwrap();

        match cli.command {
            Commands::Populate {
                underlyings,
                start,
                end,
                force,
            } => {
                assert_eq!(underlyings, vec!["TQQQ", "SPY"]);
                assert_eq!(start, NaiveDate::from_ymd_opt(2025, 1, 1).unwrap());
                assert_eq!(end, Some(NaiveDate::from_ymd_opt(2025, 1, 31).unwrap()));
                assert!(!force);
            }
            _ => panic!("expected Populate command"),
        }
    }

    #[test]
    fn parse_populate_all() {
        let cli = Cli::try_parse_from([
            "options-data",
            "populate",
            "-s",
            "all",
            "--start",
            "2025-01-01",
        ])
        .unwrap();

        match cli.command {
            Commands::Populate { underlyings, .. } => {
                let resolved = resolve_underlyings(&underlyings);
                assert_eq!(resolved.len(), DEFAULT_UNDERLYINGS.len());
                assert!(resolved.contains(&"TQQQ".to_string()));
                assert!(resolved.contains(&"SPY".to_string()));
            }
            _ => panic!("expected Populate command"),
        }
    }

    #[test]
    fn parse_status_args() {
        let cli = Cli::try_parse_from(["options-data", "status", "-s", "TQQQ"]).unwrap();
        match cli.command {
            Commands::Status { underlying } => {
                assert_eq!(underlying, Some("TQQQ".to_string()));
            }
            _ => panic!("expected Status command"),
        }
    }

    #[test]
    fn parse_status_no_underlying() {
        let cli = Cli::try_parse_from(["options-data", "status"]).unwrap();
        match cli.command {
            Commands::Status { underlying } => {
                assert!(underlying.is_none());
            }
            _ => panic!("expected Status command"),
        }
    }

    #[test]
    fn parse_validate_args() {
        let cli = Cli::try_parse_from(["options-data", "validate", "-s", "TQQQ,SPY"]).unwrap();
        match cli.command {
            Commands::Validate { underlyings } => {
                assert_eq!(
                    underlyings,
                    Some(vec!["TQQQ".to_string(), "SPY".to_string()])
                );
            }
            _ => panic!("expected Validate command"),
        }
    }

    #[test]
    fn parse_populate_force() {
        let cli = Cli::try_parse_from([
            "options-data",
            "populate",
            "-s",
            "TQQQ",
            "--start",
            "2025-01-01",
            "--force",
        ])
        .unwrap();

        match cli.command {
            Commands::Populate { force, .. } => {
                assert!(force);
            }
            _ => panic!("expected Populate command"),
        }
    }

    #[test]
    fn parse_populate_defaults() {
        let cli = Cli::try_parse_from([
            "options-data",
            "populate",
            "-s",
            "TQQQ",
            "--start",
            "2025-01-01",
        ])
        .unwrap();

        match cli.command {
            Commands::Populate { end, force, .. } => {
                assert!(end.is_none());
                assert!(!force);
            }
            _ => panic!("expected Populate command"),
        }
    }

    #[test]
    fn contiguous_ranges_groups_consecutive() {
        let dates = vec![
            NaiveDate::from_ymd_opt(2025, 1, 16).unwrap(), // Thu
            NaiveDate::from_ymd_opt(2025, 1, 17).unwrap(), // Fri
            NaiveDate::from_ymd_opt(2025, 1, 20).unwrap(), // Mon (next week, gap=3)
            NaiveDate::from_ymd_opt(2025, 1, 21).unwrap(), // Tue
        ];

        let ranges = contiguous_ranges(&dates);
        assert_eq!(ranges.len(), 1); // Gap is only 3 days (weekend), within threshold
    }

    #[test]
    fn contiguous_ranges_splits_on_large_gap() {
        let dates = vec![
            NaiveDate::from_ymd_opt(2025, 1, 6).unwrap(),
            NaiveDate::from_ymd_opt(2025, 1, 7).unwrap(),
            NaiveDate::from_ymd_opt(2025, 1, 20).unwrap(), // 13-day gap
            NaiveDate::from_ymd_opt(2025, 1, 21).unwrap(),
        ];

        let ranges = contiguous_ranges(&dates);
        assert_eq!(ranges.len(), 2);
        assert_eq!(
            ranges[0],
            (
                NaiveDate::from_ymd_opt(2025, 1, 6).unwrap(),
                NaiveDate::from_ymd_opt(2025, 1, 7).unwrap(),
            )
        );
    }

    #[test]
    fn contiguous_ranges_empty() {
        assert!(contiguous_ranges(&[]).is_empty());
    }

    #[test]
    fn resolve_underlyings_all() {
        let result = resolve_underlyings(&["all".to_string()]);
        assert_eq!(result.len(), DEFAULT_UNDERLYINGS.len());
    }

    #[test]
    fn resolve_underlyings_specific() {
        let result = resolve_underlyings(&["tqqq".to_string(), "spy".to_string()]);
        assert_eq!(result, vec!["TQQQ", "SPY"]);
    }
}
