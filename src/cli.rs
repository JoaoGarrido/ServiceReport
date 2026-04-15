use crate::calendar::config::{self, CalendarConfig, RatesConfig};
use crate::calendar::google;
use crate::calendar::{OutputFormat, generate_report, month_range};
use anyhow::Result;
use chrono::Utc;

pub fn load_config(path: &str) -> Result<CalendarConfig> {
    config::load_calendar_config(path)
}

pub fn load_rates(path: &str) -> Result<RatesConfig> {
    config::load_rates_config(path)
}

pub fn run_report(
    calendar_config: &CalendarConfig,
    rates_config: &RatesConfig,
    prefix: Option<&str>,
    month: u32,
    year: i32,
) -> Result<()> {
    let google_cfg = calendar_config
        .google
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Missing google config"))?;

    let calendar_id = google_cfg
        .calendar_id
        .as_ref()
        .map(|s| s.as_str())
        .unwrap_or("primary");

    let timezone_name = google_cfg
        .timezone
        .as_ref()
        .map(|s| s.as_str())
        .unwrap_or("UTC");

    let tz = google::parse_timezone(timezone_name)?;

    let cost_lookup = config::build_cost_lookup(rates_config);

    let (start, end) = month_range(year, month, &tz);

    let service = google::build_calendar_service(calendar_config)?;
    let events = service.fetch_events(
        calendar_id,
        start.with_timezone(&Utc),
        end.with_timezone(&Utc),
    )?;
    println!("Fetched {} events from Google Calendar", events.len());

    let report: String = generate_report(
        &events,
        month,
        year,
        &tz,
        prefix,
        &cost_lookup,
        OutputFormat::Stdout,
    );

    println!("{}", report);

    Ok(())
}
