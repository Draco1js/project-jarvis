//! Weather from wttr.in.
//!
//! A place the user names always wins; otherwise the city setting; otherwise
//! wttr.in is asked with no place, and it uses the caller's IP location.

use std::time::Duration;

use chrono::NaiveDate;
use jarvis_core::Weather;
use serde_json::Value;

/// The public wttr.in service.
pub const WTTR_URL: &str = "https://wttr.in";

const TIMEOUT: Duration = Duration::from_secs(8);

/// The [`Weather`] port backed by wttr.in's JSON (`?format=j1`) answer.
pub struct Wttr {
    base_url: String,
    city: Option<String>,
    agent: ureq::Agent,
}

impl Wttr {
    /// `city` is the city setting, used when the user names no place.
    pub fn new(base_url: &str, city: Option<String>) -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            .http_status_as_error(false)
            .build()
            .into();
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            city: city.map(|c| c.trim().to_string()).filter(|c| !c.is_empty()),
            agent,
        }
    }

    fn url(&self, place: Option<&str>) -> String {
        let place = place
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .or(self.city.as_deref())
            .unwrap_or("");
        format!("{}/{}?format=j1", self.base_url, encode(place))
    }
}

impl Weather for Wttr {
    fn report(&mut self, place: Option<&str>) -> Result<String, String> {
        let mut response = self
            .agent
            .get(&self.url(place))
            .header("accept", "application/json")
            .call()
            .map_err(|e| format!("wttr.in unreachable: {e}"))?;
        let status = response.status().as_u16();
        if status != 200 {
            return Err(format!("wttr.in answered {status}"));
        }
        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("wttr.in answer unreadable: {e}"))?;
        let json: Value =
            serde_json::from_str(&body).map_err(|e| format!("wttr.in answer unreadable: {e}"))?;
        summarise(&json).ok_or_else(|| "wttr.in answer had no weather in it".to_string())
    }
}

/// A short plain-text report the model can read out.
fn summarise(json: &Value) -> Option<String> {
    let now = json["current_condition"].get(0)?;
    let area = json["nearest_area"].get(0);
    let name = area.and_then(|a| first_value(&a["areaName"]));
    let country = area.and_then(|a| first_value(&a["country"]));
    let place = match (name, country) {
        (Some(name), Some(country)) => format!("{name}, {country}"),
        (Some(name), None) => name.to_string(),
        _ => "here".to_string(),
    };

    let mut report = format!(
        "Weather for {place}. Now {}°C, feels like {}°C, {}, wind {} km/h, humidity {}%.",
        text(&now["temp_C"])?,
        text(&now["FeelsLikeC"])?,
        first_value(&now["weatherDesc"])?.to_lowercase(),
        text(&now["windspeedKmph"])?,
        text(&now["humidity"])?,
    );
    for day in json["weather"].as_array().into_iter().flatten() {
        let date = NaiveDate::parse_from_str(text(&day["date"])?, "%Y-%m-%d").ok()?;
        let hourly = day["hourly"].as_array();
        // Midday is the most telling description for the day.
        let midday = hourly
            .and_then(|h| h.iter().find(|h| h["time"] == "1200").or(h.first()))
            .and_then(|h| first_value(&h["weatherDesc"]));
        let rain = hourly
            .into_iter()
            .flatten()
            .filter_map(|h| text(&h["chanceofrain"])?.parse::<u32>().ok())
            .max();
        report.push_str(&format!(
            " {}: {} to {}°C",
            date.format("%a %-d %b"),
            text(&day["mintempC"])?,
            text(&day["maxtempC"])?,
        ));
        if let Some(desc) = midday {
            report.push_str(&format!(", {}", desc.to_lowercase()));
        }
        if let Some(rain) = rain {
            report.push_str(&format!(", {rain}% chance of rain"));
        }
        report.push('.');
    }
    Some(report)
}

fn text(value: &Value) -> Option<&str> {
    value.as_str().map(str::trim)
}

/// wttr.in wraps most strings as `[{"value": "..."}]`.
fn first_value(value: &Value) -> Option<&str> {
    text(&value.get(0)?["value"]).filter(|s| !s.is_empty())
}

/// Percent-encode a place for the URL path.
fn encode(place: &str) -> String {
    let mut out = String::new();
    for byte in place.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.,~+".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}
