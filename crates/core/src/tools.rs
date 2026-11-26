//! The tools offered to the model, and running the calls it makes.

use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::ports::{MemoryHit, MemoryKind, Ports, ToolCall, ToolSpec};

/// Most memory search results handed to the model.
pub const SEARCH_LIMIT: usize = 5;

const SEARCH_MEMORY: ToolSpec = ToolSpec {
    name: "search_memory",
    description: "Search past conversations and the user's notes. Use it when the user \
refers to something said or noted before. Returns dated summaries, tags and excerpts.",
    parameters: r#"{"type":"object","properties":{"query":{"type":"string","description":"Keywords to look for, e.g. names, topics or places."}},"required":["query"]}"#,
};

/// Every tool the model may call.
pub const SPECS: &[ToolSpec] = &[SEARCH_MEMORY];

/// Run one call; the result (or what went wrong) goes back to the model as text.
pub fn run(call: &ToolCall, ports: &mut Ports) -> String {
    match call.name.as_str() {
        "search_memory" => match string_argument(&call.arguments, "query") {
            Some(query) => describe_hits(&query, &ports.store.search(&query, SEARCH_LIMIT)),
            None => "Error: search_memory needs a \"query\" string argument.".into(),
        },
        other => format!("Error: there is no tool called {other}."),
    }
}

fn string_argument(arguments: &str, name: &str) -> Option<String> {
    let value: Value = serde_json::from_str(arguments).ok()?;
    let text = value.get(name)?.as_str()?.trim();
    (!text.is_empty()).then(|| text.to_string())
}

fn describe_hits(query: &str, hits: &[MemoryHit]) -> String {
    if hits.is_empty() {
        return format!("Nothing found in memory for \"{query}\".");
    }
    let mut text = format!("Found {} result(s) for \"{query}\":", hits.len());
    for (i, hit) in hits.iter().enumerate() {
        let kind = match hit.kind {
            MemoryKind::Conversation => "Conversation",
            MemoryKind::Note => "Note",
        };
        text.push_str(&format!("\n{}. {kind} on {}.", i + 1, date(hit.at)));
        if !hit.summary.is_empty() {
            text.push_str(&format!(" Summary: {}", hit.summary));
        }
        if !hit.tags.is_empty() {
            text.push_str(&format!(" Tags: {}.", hit.tags.join(", ")));
        }
        if !hit.excerpt.is_empty() {
            text.push_str(&format!(" Excerpt: {}", hit.excerpt));
        }
    }
    text
}

/// `YYYY-MM-DD` in UTC.
fn date(at: SystemTime) -> String {
    let days = at
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0) as i64;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}
