use crate::calendar::config::{self, RatesConfig};
use crate::calendar::google;
use crate::calendar::{OutputFormat, generate_report, month_range};
use anyhow::Result;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    response::Html,
    routing::get,
};
use chrono::Utc;
use serde::Deserialize;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Clone)]
struct AppState {
    prefix: Option<String>,
    calendar_config_path: String,
    rates_config_path: String,
}

#[derive(Debug, Deserialize, Default)]
struct AppQuery {
    rates_config: Option<String>,
    format: Option<String>,
}

fn build_cost_lookup(config: &RatesConfig) -> HashMap<String, f64> {
    config::build_cost_lookup(config)
}

async fn home() -> Html<String> {
    let html = std::fs::read_to_string("templates/home.html")
        .unwrap_or_else(|_| "<p>Template not found</p>".to_string());
    Html(html)
}

#[derive(serde::Serialize)]
struct JsonError {
    error: String,
}

async fn get_rates(
    Query(query): Query<AppQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<axum::response::Response<String>, (axum::http::StatusCode, Json<JsonError>)> {
    let rates_config_path = query
        .rates_config
        .unwrap_or(state.rates_config_path.clone());
    let format = query.format.unwrap_or_else(|| "markdown".to_string());

    let config = config::load_rates_config(&rates_config_path).map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(JsonError {
                error: e.to_string(),
            }),
        )
    })?;

    let mut rows = build_cost_lookup(&config)
        .into_iter()
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    if format == "json" {
        let json_rows = rows
            .into_iter()
            .map(|(student, rate)| serde_json::json!({ "student": student, "rate": rate }))
            .collect::<Vec<_>>();
        let body = serde_json::json!({ "rates": json_rows }).to_string();
        return Ok(axum::response::Response::builder()
            .header("Content-Type", "application/json")
            .body(body)
            .unwrap());
    }

    let mut lines = vec![
        "# Rates".to_string(),
        "".to_string(),
        "| Student | Hourly Rate |".to_string(),
        "| --- | ---: |".to_string(),
    ];

    for row in &rows {
        let (student, rate) = row;
        lines.push(format!("| {} | {:.2}€ |", student, rate));
    }

    if rows.is_empty() {
        lines.push("No rates configured.".to_string());
    }

    let content = lines.join("\n");
    Ok(axum::response::Response::builder()
        .header("Content-Type", "text/markdown")
        .body(content)
        .unwrap())
}

#[derive(Debug, Deserialize)]
struct RatePostPayload {
    student: String,
    rate: f64,
}

async fn post_rates(
    Query(query): Query<AppQuery>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RatePostPayload>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<JsonError>)> {
    let rates_config_path = query
        .rates_config
        .unwrap_or(state.rates_config_path.clone());

    let mut config = config::load_rates_config(&rates_config_path).map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(JsonError {
                error: e.to_string(),
            }),
        )
    })?;

    let per_client_hourly = config.per_client_hourly.get_or_insert_with(HashMap::new);
    per_client_hourly.insert(payload.student, payload.rate);

    write_config_with_backup(&rates_config_path, &config).map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(JsonError {
                error: e.to_string(),
            }),
        )
    })?;

    Ok(Json(serde_json::json!({ "status": "ok" })))
}

#[derive(Debug, Deserialize)]
struct RateDeletePayload {
    student: String,
}

async fn delete_rates(
    Query(query): Query<AppQuery>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RateDeletePayload>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<JsonError>)> {
    let rates_config_path = query
        .rates_config
        .unwrap_or(state.rates_config_path.clone());

    let mut config = config::load_rates_config(&rates_config_path).map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(JsonError {
                error: e.to_string(),
            }),
        )
    })?;

    let mut removed = false;
    if let Some(ref mut per_client_hourly) = config.per_client_hourly {
        if per_client_hourly.remove(&payload.student).is_some() {
            removed = true;
        }
    }

    if removed {
        write_config_with_backup(&rates_config_path, &config).map_err(|e| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(JsonError {
                    error: e.to_string(),
                }),
            )
        })?;
    }

    if !removed {
        return Err((
            axum::http::StatusCode::NOT_FOUND,
            Json(JsonError {
                error: "Student not found".to_string(),
            }),
        ));
    }

    Ok(Json(serde_json::json!({ "status": "ok" })))
}

fn write_config_with_backup(path: &str, config: &RatesConfig) -> Result<()> {
    use chrono::Local;
    let timestamp = Local::now().format("%Y%m%d_%H%M%S");
    let backup_path = format!("{}.{}.bak", path, timestamp);

    if std::path::Path::new(path).exists() {
        std::fs::copy(path, &backup_path)?;
    }

    let yaml = serde_yaml::to_string(config)?;
    std::fs::write(path, yaml)?;

    Ok(())
}

async fn rates_ui() -> Html<String> {
    let template = std::fs::read_to_string("templates/rates.html")
        .unwrap_or_else(|_| "<p>Template not found</p>".to_string());

    Html(template)
}

#[derive(Debug, Deserialize)]
struct ReportQuery {
    calendar_config: Option<String>,
    rates_config: Option<String>,
}

async fn get_report(
    Path((year, month)): Path<(i32, u32)>,
    Query(query): Query<ReportQuery>,
    State(state): State<Arc<AppState>>,
) -> Html<String> {
    let calendar_config_path = query
        .calendar_config
        .unwrap_or(state.calendar_config_path.clone());
    let rates_config_path = query
        .rates_config
        .unwrap_or(state.rates_config_path.clone());

    let calendar_config = match config::load_calendar_config(&calendar_config_path) {
        Ok(c) => c,
        Err(e) => {
            return Html(format!("<p>Error loading calendar config: {}</p>", e));
        }
    };

    let google_cfg = match calendar_config.google.as_ref() {
        Some(g) => g,
        None => {
            return Html("<p>Missing google config</p>".to_string());
        }
    };

    let calendar_id = google_cfg.calendar_id.as_deref().unwrap_or("primary");
    let timezone_name = google_cfg.timezone.as_deref().unwrap_or("UTC");

    let tz: chrono_tz::Tz = match timezone_name.parse() {
        Ok(t) => t,
        Err(_) => {
            return Html("<p>Invalid timezone</p>".to_string());
        }
    };

    let (start, end) = month_range(year, month, &tz);

    let service = match google::build_calendar_service(&calendar_config) {
        Ok(s) => s,
        Err(e) => {
            return Html(format!("<p>Error connecting to Google Calendar: {}</p>", e));
        }
    };

    let events = match service.fetch_events(
        calendar_id,
        start.with_timezone(&Utc),
        end.with_timezone(&Utc),
    ) {
        Ok(e) => e,
        Err(e) => {
            return Html(format!("<p>Error fetching events: {}</p>", e));
        }
    };

    let rates_config = match config::load_rates_config(&rates_config_path) {
        Ok(c) => c,
        Err(e) => {
            return Html(format!("<p>Error loading rates config: {}</p>", e));
        }
    };

    let cost_lookup = build_cost_lookup(&rates_config);

    let report = generate_report(
        &events,
        month,
        year,
        &tz,
        state.prefix.clone().as_deref(),
        &cost_lookup,
        OutputFormat::Html,
    );

    Html(report)
}

pub async fn run(
    prefix: Option<String>,
    host: String,
    port: u16,
    calendar_config: String,
    rates_config: String,
) -> Result<()> {
    let state = Arc::new(AppState {
        prefix: prefix,
        calendar_config_path: calendar_config,
        rates_config_path: rates_config,
    });

    let app = Router::new()
        .route("/", get(home))
        .route(
            "/rates",
            get(get_rates).post(post_rates).delete(delete_rates),
        )
        .route("/rates-ui", get(rates_ui))
        .route("/report/{year}/{month}", get(get_report))
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
    println!("Starting server on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
