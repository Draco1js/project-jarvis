//! Conversation summaries and tags, written by the model.

use crate::ports::{ChatProvider, Message, Role, Tagged};

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

/// One model call for the summary and tags of `text`. On failure the text is
/// still worth keeping, so this returns an empty summary instead of an error.
pub fn summarize(chat: &mut dyn ChatProvider, text: &str) -> Tagged {
    let request = [
        Message::new(Role::System, SUMMARY_PROMPT),
        Message::new(Role::User, text),
    ];
    match chat.chat(&request) {
        Ok(reply) => parse(&reply),
        Err(_) => Tagged::default(),
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
