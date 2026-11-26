//! Memory: conversation summaries and tags written by the model, notes, and
//! the `search_memory` tool.

use std::time::SystemTime;

use chrono::{DateTime, FixedOffset, Utc};
use serde_json::json;

use crate::ports::{
    ChatProvider, MemoryHit, MemoryKind, Message, Ports, Reply, Role, Tagged, ToolSpec,
};

/// Most tags kept for one conversation or note.
pub const MAX_TAGS: usize = 10;

const SUMMARY_PROMPT: &str = "You file conversations and notes so they can be found later. \
Reply with exactly two lines and nothing else:\n\
Summary: one or two sentences saying what it is about.\n\
Tags: 5 to 10 comma-separated search tags (topics, synonyms, names, places).";

/// Ways of asking for a note, lowercase, matched at the start of what was said.
const NOTE_PREFIXES: &[&str] = &[
    "note that ",
    "make a note that ",
    "take a note that ",
    "make a note: ",
    "take a note: ",
    "note: ",
];

/// The note to save, if `said` asks for one ("Note that …").
pub fn note_text(said: &str) -> Option<&str> {
    let said = said.trim();
    let said = match said.get(..7) {
        Some(head) if head.eq_ignore_ascii_case("jarvis,") => said[7..].trim_start(),
        _ => said,
    };
    NOTE_PREFIXES.iter().find_map(|prefix| {
        let head = said.get(..prefix.len())?;
        let rest = said[prefix.len()..].trim();
        (head.eq_ignore_ascii_case(prefix) && !rest.is_empty()).then_some(rest)
    })
}

/// The conversation as plain lines, for the summary request.
pub fn transcript(history: &[Message]) -> String {
    history
        .iter()
        .filter_map(|m| match m.role {
            Role::User => Some(format!("User: {}", m.content)),
            Role::Assistant if !m.content.is_empty() => Some(format!("Assistant: {}", m.content)),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The most of a transcript sent for summarising, so a long conversation
/// cannot make the request slow enough to time out.
const SUMMARY_MAX_CHARS: usize = 12_000;

/// The last `max` characters of `text`.
fn tail(text: &str, max: usize) -> &str {
    match text.char_indices().rev().nth(max.saturating_sub(1)) {
        Some((i, _)) if text.chars().count() > max => &text[i..],
        _ => text,
    }
}

/// One model call for the summary and tags of `text`. On failure the text is
/// still worth keeping, so this returns an empty summary instead of an error.
pub fn summarize(chat: &mut dyn ChatProvider, text: &str) -> Tagged {
    let request = [
        Message::new(Role::System, SUMMARY_PROMPT),
        Message::new(Role::User, tail(text, SUMMARY_MAX_CHARS)),
    ];
    match chat.chat(&request, &[]) {
        Ok(Reply::Text(reply)) => parse(&reply),
        Ok(Reply::ToolCalls(_)) | Err(_) => Tagged::default(),
    }
}

/// Reads the "Summary: …" and "Tags: …" lines. Unlabelled lines count as summary.
fn parse(reply: &str) -> Tagged {
    let mut summary = Vec::new();
    let mut tags: Vec<String> = Vec::new();
    for line in reply.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Some(rest) = strip_label(line, "tags:") {
            for tag in rest.split(',').map(|t| t.trim().trim_matches('#').trim()) {
                if !tag.is_empty() && !tags.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
                    tags.push(tag.to_string());
                }
            }
        } else {
            summary.push(strip_label(line, "summary:").unwrap_or(line));
        }
    }
    tags.truncate(MAX_TAGS);
    Tagged {
        summary: summary.join(" "),
        tags,
    }
}

fn strip_label<'a>(line: &'a str, label: &str) -> Option<&'a str> {
    let line = line.trim_start_matches(['*', '-', ' ']);
    let head = line.get(..label.len())?;
    head.eq_ignore_ascii_case(label)
        .then(|| line[label.len()..].trim_start_matches('*').trim())
}

/// Most memory search results handed to the model.
pub const SEARCH_LIMIT: usize = 5;

pub fn search_spec() -> ToolSpec {
    ToolSpec {
        name: "search_memory",
        description: "Search past conversations and the user's notes. Use it when the user \
refers to something said or noted before. Returns dated summaries, tags and excerpts.",
        parameters: json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "Keywords to look for, e.g. names, topics or places."},
            },
            "required": ["query"],
        }),
    }
}

/// Run `search_memory`; the result goes back to the model as text.
pub fn search(ports: &mut Ports, query: &str) -> Result<String, String> {
    if query.is_empty() {
        return Err("search_memory needs a \"query\" string".into());
    }
    let hits = ports.store.search(query, SEARCH_LIMIT);
    if hits.is_empty() {
        return Ok(format!("Nothing found in memory for \"{query}\"."));
    }
    let offset = FixedOffset::east_opt(ports.clock.utc_offset_secs())
        .unwrap_or_else(|| FixedOffset::east_opt(0).expect("zero offset"));
    let mut text = format!("Found {} result(s) for \"{query}\":", hits.len());
    for (i, hit) in hits.iter().enumerate() {
        text.push_str(&describe_hit(i + 1, hit, offset));
    }
    Ok(text)
}

fn describe_hit(n: usize, hit: &MemoryHit, offset: FixedOffset) -> String {
    let kind = match hit.kind {
        MemoryKind::Conversation => "Conversation",
        MemoryKind::Note => "Note",
    };
    let mut text = format!("\n{n}. {kind} on {}.", date(hit.at, offset));
    if !hit.summary.is_empty() {
        text.push_str(&format!(" Summary: {}", hit.summary));
    }
    if !hit.tags.is_empty() {
        text.push_str(&format!(" Tags: {}.", hit.tags.join(", ")));
    }
    if !hit.excerpt.is_empty() {
        text.push_str(&format!(" Excerpt: {}", hit.excerpt));
    }
    text
}

/// `YYYY-MM-DD` in the user's time zone.
fn date(at: SystemTime, offset: FixedOffset) -> String {
    DateTime::<Utc>::from(at)
        .with_timezone(&offset)
        .format("%Y-%m-%d")
        .to_string()
}
