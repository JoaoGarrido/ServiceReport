pub mod config;
pub mod google;

use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Timelike};
use chrono_tz::Tz;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

pub type Event = HashMap<String, serde_json::Value>;

#[derive(Clone, Copy)]
pub enum OutputFormat {
    Json,
    Stdout,
}

#[derive(Serialize)]
pub struct ClientReport {
    pub name: String,
    pub rows: Vec<ServiceRowJson>,
    pub total_cost: f64,
}

#[derive(Serialize)]
pub struct ServiceRowJson {
    pub day: String,
    pub start: String,
    pub end: String,
    pub hours: f64,
    pub cost: f64,
}

#[derive(Serialize)]
pub struct ReportJson {
    pub total_hours: f64,
    pub total_earned: f64,
    pub clients: Vec<ClientReport>,
}

pub fn month_range(year: i32, month: u32, tz: &Tz) -> (DateTime<Tz>, DateTime<Tz>) {
    let start = tz.with_ymd_and_hms(year, month, 1, 0, 0, 0).unwrap();
    let end = if month == 12 {
        tz.with_ymd_and_hms(year + 1, 1, 1, 0, 0, 0).unwrap()
    } else {
        tz.with_ymd_and_hms(year, month + 1, 1, 0, 0, 0).unwrap()
    };
    (start, end)
}

pub fn parse_event_datetimes(
    event: &Event,
    tz: &Tz,
) -> Result<(DateTime<Tz>, DateTime<Tz>, bool), anyhow::Error> {
    let start = event
        .get("start")
        .and_then(|v| v.as_object())
        .ok_or_else(|| anyhow::anyhow!("Missing event start"))?;
    let end = event
        .get("end")
        .and_then(|v| v.as_object())
        .ok_or_else(|| anyhow::anyhow!("Missing event end"))?;

    if let Some(start_dt) = start.get("dateTime").and_then(|v| v.as_str()) {
        let end_dt = end
            .get("dateTime")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing end dateTime"))?;
        let start_parsed = DateTime::parse_from_rfc3339(start_dt)?;
        let end_parsed = DateTime::parse_from_rfc3339(end_dt)?;
        let start_tz = start_parsed.with_timezone(tz);
        let end_tz = end_parsed.with_timezone(tz);
        return Ok((start_tz, end_tz, false));
    }

    if let Some(start_date) = start.get("date").and_then(|v| v.as_str()) {
        let end_date = end
            .get("date")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing end date"))?;

        let start_parsed = NaiveDate::parse_from_str(start_date, "%Y-%m-%d")?;
        let end_parsed = NaiveDate::parse_from_str(end_date, "%Y-%m-%d")?;

        let start_dt = tz.from_utc_datetime(&start_parsed.and_hms_opt(0, 0, 0).unwrap());
        let end_dt = tz.from_utc_datetime(&end_parsed.and_hms_opt(0, 0, 0).unwrap());

        return Ok((start_dt, end_dt, true));
    }

    Err(anyhow::anyhow!("Invalid event datetime format"))
}

pub struct DaySegment {
    pub day: NaiveDate,
    pub start: DateTime<Tz>,
    pub end: DateTime<Tz>,
    pub hours: f64,
}

pub fn split_event_by_day(
    start_dt: DateTime<Tz>,
    end_dt: DateTime<Tz>,
    tz: &Tz,
) -> Vec<DaySegment> {
    if end_dt <= start_dt {
        return vec![];
    }

    let mut results = vec![];
    let mut current_day = start_dt.date_naive();
    let last_day = end_dt.date_naive();

    while current_day <= last_day {
        let day_start = tz
            .with_ymd_and_hms(
                current_day.year(),
                current_day.month(),
                current_day.day(),
                0,
                0,
                0,
            )
            .unwrap();
        let day_end = day_start + chrono::Duration::days(1);

        let segment_start = std::cmp::max(start_dt, day_start);
        let segment_end = std::cmp::min(end_dt, day_end);

        if segment_end > segment_start {
            let hours = (segment_end - segment_start).num_seconds() as f64 / 3600.0;
            results.push(DaySegment {
                day: current_day,
                start: segment_start,
                end: segment_end,
                hours,
            });
        }

        current_day = current_day.succ_opt().unwrap();
    }

    results
}

pub fn parse_summary(summary: &str, prefix: Option<&str>) -> Option<(String, String)> {
    if let Some(p) = prefix {
        if !summary.starts_with(p) {
            return None;
        }
        let parts: Vec<&str> = summary.split(": ").collect();
        if parts.len() == 2 {
            let service = parts[0].trim();
            let client = parts[1].trim();
            if !service.is_empty() && !client.is_empty() {
                return Some((service.to_string(), client.to_string()));
            }
        }
        let parts: Vec<&str> = summary.split(" - ").collect();
        if parts.len() == 2 {
            let service = parts[0].trim();
            let client = parts[1].trim();
            if !service.is_empty() && !client.is_empty() {
                return Some((service.to_string(), client.to_string()));
            }
        }
        return None;
    }

    if summary.contains(": ") {
        let parts: Vec<&str> = summary.split(": ").collect();
        if parts.len() == 2 {
            let service = parts[0].trim();
            let client = parts[1].trim();
            if !service.is_empty() && !client.is_empty() {
                return Some((service.to_string(), client.to_string()));
            }
        }
    }

    if summary.contains(" - ") {
        let parts: Vec<&str> = summary.split(" - ").collect();
        if parts.len() == 2 {
            let service = parts[0].trim();
            let client = parts[1].trim();
            if !service.is_empty() && !client.is_empty() {
                return Some((service.to_string(), client.to_string()));
            }
        }
    }

    let trimmed = summary.trim();
    if !trimmed.is_empty() {
        return Some(("Service".to_string(), trimmed.to_string()));
    }

    None
}

#[derive(Clone)]
pub struct ServiceRow {
    pub day: NaiveDate,
    pub start: DateTime<Tz>,
    pub end: DateTime<Tz>,
    pub hours: f64,
    pub service: String,
}

fn resolve_hourly_rate(
    client: &str,
    cost_lookup: &HashMap<String, f64>,
    missing_clients: &mut HashSet<String>,
) -> Option<f64> {
    match cost_lookup.get(client) {
        Some(rate) => Some(*rate),
        None => {
            if missing_clients.insert(client.to_string()) {
                tracing::warn!("Missing cost config for client: {}", client);
            }
            None
        }
    }
}

fn format_time(start_time: DateTime<Tz>, end_time: DateTime<Tz>) -> (String, String) {
    let start_str = start_time.format("%H:%M").to_string();
    let end_str = if end_time.hour() == 0
        && end_time.minute() == 0
        && end_time.date_naive() != start_time.date_naive()
    {
        "24:00".to_string()
    } else {
        end_time.format("%H:%M").to_string()
    };
    (start_str, end_str)
}

pub fn generate_report(
    events: &[Event],
    month: u32,
    year: i32,
    tz: &Tz,
    prefix: Option<&str>,
    cost_lookup: &HashMap<String, f64>,
    format: OutputFormat,
) -> String {
    let initial_date = tz.with_ymd_and_hms(year, month, 1, 0, 0, 0).unwrap();
    let final_date = if month == 12 {
        tz.with_ymd_and_hms(year + 1, 1, 1, 0, 0, 0).unwrap()
    } else {
        tz.with_ymd_and_hms(year, month + 1, 1, 0, 0, 0).unwrap()
    };

    let relevant_events: Vec<&Event> = events
        .iter()
        .filter(|event| {
            let parse_result = parse_event_datetimes(event, &initial_date.timezone());
            match parse_result {
                Ok((start_dt, end_dt, _)) => start_dt < final_date && end_dt > initial_date,
                Err(_) => {
                    tracing::warn!("Failed to parse event datetimes");
                    false
                }
            }
        })
        .filter(|event| {
            let summary = event.get("summary").and_then(|v| v.as_str()).unwrap_or("");
            if let Some(p) = prefix {
                summary.starts_with(p)
            } else {
                true
            }
        })
        .collect();
    let events_by_client = events_by_client(
        &relevant_events
            .iter()
            .map(|e| (*e).clone())
            .collect::<Vec<Event>>(),
    );
    match format {
        OutputFormat::Json => json_report(&events_by_client, cost_lookup),
        OutputFormat::Stdout => stdout_report(&events_by_client, cost_lookup),
    }
}

pub fn json_report(
    rows_by_client: &HashMap<String, Vec<ServiceRow>>,
    cost_lookup: &HashMap<String, f64>,
) -> String {
    let mut missing_client_costs: HashSet<String> = HashSet::new();

    if rows_by_client.is_empty() {
        return serde_json::to_string(&ReportJson {
            total_hours: 0.0,
            total_earned: 0.0,
            clients: vec![],
        })
        .unwrap();
    }

    let mut total_hours = 0.0;
    let mut total_earned = 0.0;
    let mut clients: Vec<ClientReport> = vec![];

    let mut client_names: Vec<&String> = rows_by_client.keys().collect();
    client_names.sort();

    for client_name in client_names {
        let rows = {
            let mut r = rows_by_client.get(client_name).unwrap().clone();
            r.sort_by(|a, b| (a.day, a.start, a.end).cmp(&(b.day, a.start, b.end)));
            r
        };

        let mut client_total_cost = 0.0;
        let mut rows_json: Vec<ServiceRowJson> = vec![];

        for item in rows.iter() {
            total_hours += item.hours;
            let (start_str, end_str) = format_time(item.start, item.end);
            let hourly_rate =
                resolve_hourly_rate(client_name, cost_lookup, &mut missing_client_costs);
            let cost = match hourly_rate {
                Some(rate) => {
                    let c = item.hours * rate;
                    client_total_cost += c;
                    total_earned += c;
                    c
                }
                None => 0.0,
            };

            rows_json.push(ServiceRowJson {
                day: item.day.format("%Y-%m-%d").to_string(),
                start: start_str,
                end: end_str,
                hours: item.hours,
                cost,
            });
        }

        clients.push(ClientReport {
            name: client_name.clone(),
            rows: rows_json,
            total_cost: client_total_cost,
        });
    }

    serde_json::to_string(&ReportJson {
        total_hours,
        total_earned,
        clients,
    })
    .unwrap()
}

pub fn events_by_client(events: &[Event]) -> HashMap<String, Vec<ServiceRow>> {
    let mut rows_by_client: HashMap<String, Vec<ServiceRow>> = HashMap::new();

    events.iter().for_each(|event| {
        let summary = event.get("summary").and_then(|v| v.as_str()).unwrap_or("");
        let parsed = parse_summary(summary, None);
        if let Some((service_name, client)) = parsed {
            let parse_result = parse_event_datetimes(event, &Tz::UTC);
            if let Ok((start_dt, end_dt, _)) = parse_result {
                rows_by_client
                    .entry(client.clone())
                    .or_insert_with(Vec::new)
                    .push(ServiceRow {
                        day: start_dt.date_naive(),
                        start: start_dt,
                        end: end_dt,
                        hours: (end_dt - start_dt).num_seconds() as f64 / 3600.0,
                        service: service_name.clone(),
                    });
            }
        }
    });
    rows_by_client
}

pub fn stdout_report(
    rows_by_client: &HashMap<String, Vec<ServiceRow>>,
    cost_lookup: &HashMap<String, f64>,
) -> String {
    let mut missing_client_costs: HashSet<String> = HashSet::new();

    let mut lines = vec![];
    let mut clients: Vec<&String> = rows_by_client.keys().collect();
    clients.sort();

    for client in clients {
        lines.push(format!("## {}", client));
        lines.push(String::new());
        lines.push("| Day | Start | End | Hours | Cost |".to_string());
        lines.push("| --- | ---: | ---: | ---: | ---: |".to_string());

        let rows = {
            let mut r = rows_by_client.get(client).unwrap().clone();
            r.sort_by(|a, b| (a.day, a.start, a.end).cmp(&(b.day, b.start, b.end)));
            r
        };

        let mut total_cost = 0.0;
        for item in rows.iter() {
            let (start_str, end_str) = format_time(item.start, item.end);
            let hourly_rate = resolve_hourly_rate(client, cost_lookup, &mut missing_client_costs);
            let cost_display = match hourly_rate {
                Some(rate) => {
                    let cost_value = item.hours * rate;
                    total_cost += cost_value;
                    format!("{:.2}", cost_value)
                }
                None => "-".to_string(),
            };
            lines.push(format!(
                "| {} | {} | {} | {:.2} | {} |",
                item.day.format("%Y-%m-%d").to_string(),
                start_str,
                end_str,
                item.hours,
                cost_display
            ));
        }

        if !rows.is_empty() {
            lines.push(format!("| **Total** |  |  |  | {:.2} |", total_cost));
        }

        lines.push(String::new());
    }

    if lines.is_empty() {
        lines.push("No matching events found.".to_string());
    }

    lines.join("\n")
}

pub fn calculate_month_totals(
    events: &[Event],
    month: u32,
    year: i32,
    tz: &Tz,
    prefix: Option<&str>,
    cost_lookup: &HashMap<String, f64>,
) -> (f64, f64) {
    let mut total_hours = 0.0;
    let mut total_cost = 0.0;
    let mut missing_clients: HashSet<String> = HashSet::new();

    for event in events {
        let summary = event.get("summary").and_then(|v| v.as_str()).unwrap_or("");
        let parsed = parse_summary(summary, prefix);
        let parsed = match parsed {
            Some(p) => p,
            None => {
                if prefix.map_or(true, |p| summary.starts_with(p)) {
                    tracing::warn!("Invalid summary format: {}", summary);
                }
                continue;
            }
        };

        let (_service_name, client) = parsed;

        let parse_result = parse_event_datetimes(event, tz);
        let (start_dt, end_dt, _) = match parse_result {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("Failed to parse event datetime ({}): {}", summary, e);
                continue;
            }
        };

        let hourly_rate = match cost_lookup.get(&client) {
            Some(rate) => Some(*rate),
            None => {
                if missing_clients.insert(client.clone()) {
                    tracing::warn!("Missing cost config for client: {}", client);
                }
                None
            }
        };

        for segment in split_event_by_day(start_dt, end_dt, tz) {
            if segment.day.month() == month && segment.day.year() == year {
                total_hours += segment.hours;
                if let Some(rate) = hourly_rate {
                    total_cost += segment.hours * rate;
                }
            }
        }
    }

    (total_hours, total_cost)
}
