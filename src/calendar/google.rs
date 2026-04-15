use super::{Event, config};
use anyhow::Result;
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use std::collections::HashMap;
use yup_oauth2::{InstalledFlowAuthenticator, InstalledFlowReturnMethod, read_application_secret};

const SCOPES: &[&str] = &["https://www.googleapis.com/auth/calendar.readonly"];

#[derive(Clone)]
pub struct GoogleCalendarService {
    access_token: String,
}

impl GoogleCalendarService {
    pub fn fetch_events(
        &self,
        calendar_id: &str,
        time_min: DateTime<Utc>,
        time_max: DateTime<Utc>,
    ) -> Result<Vec<Event>> {
        let client = reqwest::blocking::Client::builder()
            .user_agent("curl/8.14.1")
            .build()?;
        let mut events = vec![];
        let mut page_token: Option<String> = None;

        fn google_isoformat(dt: DateTime<Utc>) -> String {
            dt.format("%Y-%m-%dT%H:%M:%SZ").to_string()
        }
        loop {
            let url = format!(
                "https://www.googleapis.com/calendar/v3/calendars/{}/events?maxResults=2500&singleEvents=true&orderBy=startTime&timeMin={}&timeMax={}",
                calendar_id,
                google_isoformat(time_min),
                google_isoformat(time_max)
            );
            tracing::info!("Fetching events URL: {}", url);
            let mut request = client
                .get(&url)
                .header("Authorization", format!("Bearer {}", self.access_token));

            if let Some(ref token) = page_token {
                let url = format!(
                    "https://www.googleapis.com/calendar/v3/calendars/{}/events?maxResults=2500&singleEvents=true&pageToken={}&orderBy=startTime&timeMin={}&timeMax={}",
                    calendar_id,
                    token,
                    google_isoformat(time_min),
                    google_isoformat(time_max)
                );
                request = client
                    .get(&url)
                    .header("Authorization", format!("Bearer {}", self.access_token));
            }

            let response = request.send()?;
            if !response.status().is_success() {
                return Err(anyhow::anyhow!(
                    "Failed to fetch events: {}",
                    response.text().unwrap_or_default()
                ));
            }
            let json: serde_json::Value = response.json()?;

            if let Some(items) = json.get("items").and_then(|v| v.as_array()) {
                for item in items {
                    if let Some(obj) = item.as_object() {
                        let mut event = HashMap::new();
                        for (key, value) in obj {
                            event.insert(
                                key.clone(),
                                serde_json::to_value(value).unwrap_or(serde_json::Value::Null),
                            );
                        }
                        events.push(event);
                    }
                }
            }

            page_token = json
                .get("nextPageToken")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            if page_token.is_none() {
                break;
            }
        }

        Ok(events)
    }
}

pub async fn build_calendar_service_async(
    client_secret_file: &str,
    token_file: &str,
) -> Result<GoogleCalendarService> {
    let secret = read_application_secret(client_secret_file).await?;

    let auth = InstalledFlowAuthenticator::builder(secret, InstalledFlowReturnMethod::HTTPRedirect)
        .persist_tokens_to_disk(token_file)
        .build()
        .await?;

    let token = auth.token(SCOPES).await?;

    Ok(GoogleCalendarService {
        access_token: token.as_str().to_string(),
    })
}

pub fn build_calendar_service(config: &config::CalendarConfig) -> Result<GoogleCalendarService> {
    let google_cfg = config
        .google
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Missing google config"))?;

    let client_secret_file = google_cfg
        .client_secret_file
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Missing client_secret_file"))?;

    let token_file = google_cfg
        .token_file
        .as_ref()
        .map(|s| s.as_str())
        .unwrap_or("token.json");
    let client_secret = client_secret_file.to_string();
    let token = token_file.to_string();
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current()
            .block_on(async { build_calendar_service_async(&client_secret, &token).await })
    })
}

pub fn parse_timezone(tz_name: &str) -> Result<Tz> {
    tz_name
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid timezone: {}", tz_name))
}
