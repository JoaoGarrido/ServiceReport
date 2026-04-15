pub mod config;
pub mod google;

use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Timelike};
use chrono_tz::Tz;
use std::collections::{HashMap, HashSet};

pub type Event = HashMap<String, serde_json::Value>;

#[derive(Clone, Copy)]
pub enum OutputFormat {
    Html,
    Stdout,
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
    service_name: &str,
    client: &str,
    cost_lookup: &HashMap<String, HashMap<String, f64>>,
    missing_services: &mut HashSet<String>,
    missing_clients: &mut HashSet<(String, String)>,
) -> Option<f64> {
    let rates = match cost_lookup.get(service_name) {
        Some(r) => r,
        None => {
            if missing_services.insert(service_name.to_string()) {
                tracing::warn!("Missing cost config for service: {}", service_name);
            }
            return None;
        }
    };
    match rates.get(client) {
        Some(rate) => Some(*rate),
        None => {
            let key = (service_name.to_string(), client.to_string());
            if missing_clients.insert(key) {
                tracing::warn!(
                    "Missing cost config for service/client: {} / {}",
                    service_name,
                    client
                );
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

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn generate_report(
    events: &[Event],
    month: u32,
    year: i32,
    tz: &Tz,
    prefix: Option<&str>,
    cost_lookup: &HashMap<String, HashMap<String, f64>>,
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
                tracing::warn!("Event without prefix: {}", summary);
                false
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
        OutputFormat::Html => html_report(&events_by_client, cost_lookup),
        OutputFormat::Stdout => stdout_report(&events_by_client, cost_lookup),
    }
}

pub fn html_report(
    rows_by_client: &HashMap<String, Vec<ServiceRow>>,
    cost_lookup: &HashMap<String, HashMap<String, f64>>,
) -> String {
    let missing_service_costs: HashSet<String> = HashSet::new();
    let missing_client_costs: HashSet<(String, String)> = HashSet::new();

    if rows_by_client.is_empty() {
        return "<p>No matching events found.</p>".to_string();
    }

    let mut total_hours = 0.0;
    let mut total_earned = 0.0;
    for (client_name, items) in rows_by_client {
        for item in items {
            total_hours += item.hours;
            if let Some(rate) = resolve_hourly_rate(
                &item.service,
                client_name,
                cost_lookup,
                &mut missing_service_costs.clone(),
                &mut missing_client_costs.clone(),
            ) {
                total_earned += item.hours * rate;
            }
        }
    }

    let mut parts = vec![
        "<html>".to_string(),
        "<head>".to_string(),
        r#"<meta charset="utf-8" />"#.to_string(),
        "<style>".to_string(),
        "table { border-collapse: collapse; width: 100%; }".to_string(),
        "th, td { border: 1px solid #ccc; padding: 6px 8px; text-align: right; }".to_string(),
        "th:first-child, td:first-child { text-align: left; }".to_string(),
        "th:nth-child(2), th:nth-child(3), td:nth-child(2), td:nth-child(3) { text-align: center; }".to_string(),
        "h2 { margin-top: 24px; }".to_string(),
        "</style>".to_string(),
        "</head>".to_string(),
        "<body>".to_string(),
        "<p><strong>Congratulations!</strong></p>".to_string(),
        format!("<p>Hours worked {:.2}</p>", total_hours),
        format!("<p>Earned {:.2} this month</p>", total_earned),
    ];

    let mut clients: Vec<&String> = rows_by_client.keys().collect();
    clients.sort();

    for client in clients {
        parts.push(format!("<h2>{}</h2>", html_escape(client)));
        parts.push("<table>".to_string());
        parts.push("<thead><tr><th>Day</th><th>Start</th><th>End</th><th>Hours</th><th>Cost</th></tr></thead>".to_string());
        parts.push("<tbody>".to_string());

        let rows = {
            let mut r = rows_by_client.get(client).unwrap().clone();
            r.sort_by(|a, b| (a.day, a.start, a.end).cmp(&(b.day, b.start, b.end)));
            r
        };

        let mut total_cost = 0.0;
        for item in rows.iter() {
            let (start_str, end_str) = format_time(item.start, item.end);
            let hourly_rate = resolve_hourly_rate(
                &item.service,
                client,
                cost_lookup,
                &mut missing_service_costs.clone(),
                &mut missing_client_costs.clone(),
            );
            let cost_display = match hourly_rate {
                Some(rate) => {
                    let cost_value = item.hours * rate;
                    total_cost += cost_value;
                    format!("{:.2}", cost_value)
                }
                None => "-".to_string(),
            };

            parts.push(format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{:.2}</td><td>{}</td></tr>",
                html_escape(&item.day.format("%Y-%m-%d").to_string()),
                html_escape(&start_str),
                html_escape(&end_str),
                item.hours,
                html_escape(&cost_display)
            ));
        }

        if !rows.is_empty() {
            parts.push(format!(
                "<tr><td><strong>Total</strong></td><td></td><td></td><td></td><td><strong>{:.2}</strong></td></tr>",
                total_cost
            ));
        }

        parts.push("</tbody></table>".to_string());
    }

    parts.extend(["</body>".to_string(), "</html>".to_string()]);
    parts.join("\n")
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
    cost_lookup: &HashMap<String, HashMap<String, f64>>,
) -> String {
    let mut missing_service_costs: HashSet<String> = HashSet::new();
    let mut missing_client_costs: HashSet<(String, String)> = HashSet::new();

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
            let hourly_rate = resolve_hourly_rate(
                &item.service,
                client,
                cost_lookup,
                &mut missing_service_costs,
                &mut missing_client_costs,
            );
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
    cost_lookup: &HashMap<String, HashMap<String, f64>>,
) -> (f64, f64) {
    let mut total_hours = 0.0;
    let mut total_cost = 0.0;
    let mut missing_service_costs: HashSet<String> = HashSet::new();
    let mut missing_client_costs: HashSet<(String, String)> = HashSet::new();

    fn resolve_hourly_rate(
        service_name: &str,
        client: &str,
        cost_lookup: &HashMap<String, HashMap<String, f64>>,
        missing_services: &mut HashSet<String>,
        missing_clients: &mut HashSet<(String, String)>,
    ) -> Option<f64> {
        let rates = match cost_lookup.get(service_name) {
            Some(r) => r,
            None => {
                if missing_services.insert(service_name.to_string()) {
                    tracing::warn!("Missing cost config for service: {}", service_name);
                }
                return None;
            }
        };
        match rates.get(client) {
            Some(rate) => Some(*rate),
            None => {
                let key = (service_name.to_string(), client.to_string());
                if missing_clients.insert(key) {
                    tracing::warn!(
                        "Missing cost config for service/client: {} / {}",
                        service_name,
                        client
                    );
                }
                None
            }
        }
    }

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

        let (service_name, client) = parsed;

        let parse_result = parse_event_datetimes(event, tz);
        let (start_dt, end_dt, _) = match parse_result {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("Failed to parse event datetime ({}): {}", summary, e);
                continue;
            }
        };

        let hourly_rate = resolve_hourly_rate(
            &service_name,
            &client,
            cost_lookup,
            &mut missing_service_costs,
            &mut missing_client_costs,
        );

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
