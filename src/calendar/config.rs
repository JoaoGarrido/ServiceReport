use std::collections::HashMap;
use std::path::Path;

pub fn load_config(path: &str) -> anyhow::Result<serde_yaml::Value> {
    if !Path::new(path).is_file() {
        return Err(anyhow::anyhow!("Config file not found: {}", path));
    }
    let content = std::fs::read_to_string(path)?;
    let parsed: serde_yaml::Value = serde_yaml::from_str(&content)?;
    Ok(parsed)
}

pub fn load_calendar_config(path: &str) -> anyhow::Result<CalendarConfig> {
    let value = load_config(path)?;
    let config: CalendarConfig = serde_yaml::from_value(value)?;
    Ok(config)
}

pub fn load_rates_config(path: &str) -> anyhow::Result<RatesConfig> {
    let value = load_config(path)?;
    let config: RatesConfig = serde_yaml::from_value(value)?;
    Ok(config)
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CalendarConfig {
    pub google: Option<GoogleConfig>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct GoogleConfig {
    pub client_secret_file: Option<String>,
    pub token_file: Option<String>,
    pub calendar_id: Option<String>,
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub struct RatesConfig {
    pub per_client_hourly: Option<HashMap<String, f64>>,
}

pub fn build_cost_lookup(config: &RatesConfig) -> HashMap<String, f64> {
    let mut cost_lookup = HashMap::new();
    if let Some(ref per_client) = config.per_client_hourly {
        for (client, rate) in per_client {
            cost_lookup.insert(client.clone(), *rate);
        }
    }
    cost_lookup
}
