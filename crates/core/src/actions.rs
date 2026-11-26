//! The computer-action tools the model may call, and what each one does.
//! This decides everything (URLs, private windows, which app); the
//! [`Actions`] port only carries out the result.

use std::collections::HashSet;

use serde_json::{Value, json};

use crate::ports::{Action, Actions, App, Folder, MediaKey, ToolCall, ToolSpec, Volume};

/// What running one tool call led to.
pub(crate) enum Outcome {
    /// Send this result back to the model and let it answer.
    Done(String),
    /// Stop and ask the user this question instead of calling the model again.
    Ask(String),
}

#[derive(Default)]
pub(crate) struct Toolbox {
    /// Browsers the user has already been told have no private mode.
    told_no_private: HashSet<String>,
}

impl Toolbox {
    /// The tools offered to the model, built from the current settings.
    pub(crate) fn specs(&self, actions: &mut dyn Actions) -> Vec<ToolSpec> {
        let bangs = actions
            .bangs()
            .iter()
            .map(|b| format!("{} !{}", b.name, b.bang))
            .collect::<Vec<_>>()
            .join(", ");
        let folder = json!({
            "type": "string",
            "enum": FOLDERS.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
        });
        vec![
            spec(
                "web_search",
                format!(
                    "Search the web in the user's browser. When the user names a site, or a \
                     site clearly fits, pass its DuckDuckGo bang. Bangs for sites the user may \
                     name: {bangs}. Any other DuckDuckGo bang also works. Leave the bang out \
                     for a plain web search."
                ),
                json!({
                    "query": {"type": "string", "description": "What to search for."},
                    "bang": {"type": "string", "description": "A bang without the '!', e.g. \"yt\"."},
                }),
                &["query"],
            ),
            spec(
                "open_url",
                "Open a web page (http or https) in the user's browser.".into(),
                json!({"url": {"type": "string"}}),
                &["url"],
            ),
            spec(
                "open_app",
                "Open an installed app by name. If the name fits several apps, the user is \
                 asked which one; call again with the exact name they pick."
                    .into(),
                json!({"name": {"type": "string", "description": "The app's name as the user said it."}}),
                &["name"],
            ),
            spec(
                "open_folder",
                "Open one of the user's folders in the file manager.".into(),
                json!({"folder": folder}),
                &["folder"],
            ),
            spec(
                "open_recent_file",
                "Open the most recently changed file in one of the user's folders.".into(),
                json!({"folder": folder}),
                &["folder"],
            ),
            spec(
                "volume",
                "Change the system volume. `level` is the percentage for `set`, or how many \
                 points to move for `up` and `down` (default 10)."
                    .into(),
                json!({
                    "change": {"type": "string", "enum": ["set", "up", "down", "mute", "unmute"]},
                    "level": {"type": "integer", "minimum": 0, "maximum": 100},
                }),
                &["change"],
            ),
            spec(
                "media",
                "Press a media key: play or pause, next track, previous track.".into(),
                json!({"key": {"type": "string", "enum": ["play_pause", "next", "previous"]}}),
                &["key"],
            ),
        ]
    }

    pub(crate) fn run(
        &mut self,
        call: &ToolCall,
        actions: &mut dyn Actions,
        notices: &mut Vec<String>,
    ) -> Outcome {
        let args: Value = match serde_json::from_str(&call.arguments) {
            Ok(Value::Object(map)) => Value::Object(map),
            _ => return Outcome::Done("Error: the arguments must be a JSON object.".into()),
        };
        match call.name.as_str() {
            "web_search" => {
                let Some(query) = text(&args, "query") else {
                    return Outcome::Done("Error: `query` is required.".into());
                };
                let bang = text(&args, "bang").and_then(|b| bang_for(&b, actions));
                let q = match bang {
                    Some(bang) => format!("!{bang} {query}"),
                    None => query,
                };
                let url = format!("https://unduck.link?q={}", encode(&q));
                self.open_url(url, actions, notices)
            }
            "open_url" => match text(&args, "url").map(|u| web_url(&u)) {
                Some(Ok(url)) => self.open_url(url, actions, notices),
                Some(Err(error)) => Outcome::Done(error),
                None => Outcome::Done("Error: `url` is required.".into()),
            },
            "open_app" => {
                let Some(name) = text(&args, "name") else {
                    return Outcome::Done("Error: `name` is required.".into());
                };
                match find_app(&name, actions.apps()) {
                    Found::One(app) => done(actions.run(&Action::OpenApp(app))),
                    Found::Several(names) => {
                        Outcome::Ask(format!("Which one do you mean: {}?", or_list(&names)))
                    }
                    Found::None => Outcome::Done(format!("No app called \"{name}\" is installed.")),
                }
            }
            "open_folder" | "open_recent_file" => {
                let Some(folder) = text(&args, "folder").and_then(|f| folder(&f)) else {
                    return Outcome::Done("Error: unknown `folder`.".into());
                };
                let action = if call.name == "open_folder" {
                    Action::OpenFolder(folder)
                } else {
                    Action::OpenRecentFile(folder)
                };
                done(actions.run(&action))
            }
            "volume" => {
                let level = args
                    .get("level")
                    .and_then(Value::as_f64)
                    .map(|l| l.round().clamp(0.0, 100.0) as u8);
                let change = match text(&args, "change").as_deref() {
                    Some("set") => match level {
                        Some(level) => Volume::Set(level),
                        None => return Outcome::Done("Error: `set` needs a `level`.".into()),
                    },
                    Some("up") => Volume::Up(level.unwrap_or(DEFAULT_VOLUME_STEP)),
                    Some("down") => Volume::Down(level.unwrap_or(DEFAULT_VOLUME_STEP)),
                    Some("mute") => Volume::Mute,
                    Some("unmute") => Volume::Unmute,
                    _ => return Outcome::Done("Error: unknown `change`.".into()),
                };
                done(actions.run(&Action::Volume(change)))
            }
            "media" => {
                let key = match text(&args, "key").as_deref() {
                    Some("play_pause") => MediaKey::PlayPause,
                    Some("next") => MediaKey::Next,
                    Some("previous") => MediaKey::Previous,
                    _ => return Outcome::Done("Error: unknown `key`.".into()),
                };
                done(actions.run(&Action::Media(key)))
            }
            other => Outcome::Done(format!("Error: there is no tool called {other}.")),
        }
    }

    /// Opens `url` in a private window when the browser has one; otherwise
    /// in a normal window, telling the user once per browser.
    fn open_url(
        &mut self,
        url: String,
        actions: &mut dyn Actions,
        notices: &mut Vec<String>,
    ) -> Outcome {
        let browser = actions.browser();
        let private = browser.kind.private_flag().is_some();
        let result = done(actions.run(&Action::OpenUrl { url, private }));
        if !private && self.told_no_private.insert(browser.name.clone()) {
            notices.push(format!(
                "{} can't open private windows, so I used a normal one.",
                browser.name
            ));
        }
        result
    }
}

/// The bang to use for what the model passed: a bang (with or without `!`)
/// or a site name from the list.
fn bang_for(given: &str, actions: &mut dyn Actions) -> Option<String> {
    let given = given.trim_start_matches('!').trim();
    if given.is_empty() {
        return None;
    }
    let named = actions
        .bangs()
        .into_iter()
        .find(|b| b.name.eq_ignore_ascii_case(given))
        .map(|b| b.bang);
    Some(named.unwrap_or_else(|| given.to_string()))
}

/// How far "louder" or "quieter" moves the volume when no amount is given.
const DEFAULT_VOLUME_STEP: u8 = 10;

const FOLDERS: &[(&str, Folder)] = &[
    ("home", Folder::Home),
    ("desktop", Folder::Desktop),
    ("documents", Folder::Documents),
    ("downloads", Folder::Downloads),
    ("pictures", Folder::Pictures),
    ("music", Folder::Music),
    ("videos", Folder::Videos),
];

fn folder(name: &str) -> Option<Folder> {
    FOLDERS
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, f)| *f)
}

fn spec(name: &str, description: String, properties: Value, required: &[&str]) -> ToolSpec {
    ToolSpec {
        name: name.into(),
        description,
        parameters: json!({
            "type": "object",
            "properties": properties,
            "required": required,
        }),
    }
}

/// Only http and https pages may be opened; a bare domain gets `https://`.
fn web_url(url: &str) -> Result<String, String> {
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") {
        return Ok(url.to_string());
    }
    let has_scheme = url.split_once(':').is_some_and(|(scheme, rest)| {
        !scheme.is_empty()
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
            && !rest.chars().next().is_some_and(|c| c.is_ascii_digit())
    });
    if has_scheme || url.chars().any(char::is_whitespace) {
        return Err(format!(
            "Error: only http and https pages can be opened, not {url}"
        ));
    }
    Ok(format!("https://{url}"))
}

enum Found {
    One(App),
    Several(Vec<String>),
    None,
}

/// Most names offered when asking which app was meant.
const MAX_CHOICES: usize = 4;
/// How alike (Jaro-Winkler) a misspelled name must be to count.
const FUZZY_MIN: f64 = 0.85;
/// Fuzzy matches closer than this to the best one are too close to call.
const FUZZY_TIE: f64 = 0.03;

/// Lowercase words; `+` and `#` count as letters so "Notepad++" stays itself.
fn words(name: &str) -> Vec<String> {
    name.to_lowercase()
        .split(|c: char| !(c.is_alphanumeric() || c == '+' || c == '#'))
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// Finds the app the user meant: an exact name first, then names containing
/// every spoken word (as word starts), then close misspellings.
fn find_app(query: &str, apps: Vec<App>) -> Found {
    let mut apps = apps;
    let mut seen = HashSet::new();
    apps.retain(|app| seen.insert(words(&app.name)));

    let query = words(query);
    if query.is_empty() {
        return Found::None;
    }
    if let Some(app) = apps.iter().find(|app| words(&app.name) == query) {
        return Found::One(app.clone());
    }

    let containing: Vec<&App> = apps
        .iter()
        .filter(|app| {
            let name = words(&app.name);
            query
                .iter()
                .all(|q| name.iter().any(|w| w.starts_with(q.as_str())))
        })
        .collect();
    match containing.as_slice() {
        [app] => return Found::One((*app).clone()),
        [] => {}
        several => return Found::Several(names(several.iter().copied())),
    }

    let spoken = query.join(" ");
    let mut scored: Vec<(f64, &App)> = apps
        .iter()
        .map(|app| {
            let name = words(&app.name);
            let whole = strsim::jaro_winkler(&spoken, &name.join(" "));
            let best_word = name
                .iter()
                .map(|w| strsim::jaro_winkler(&spoken, w))
                .fold(0.0, f64::max);
            (whole.max(best_word), app)
        })
        .filter(|(score, _)| *score >= FUZZY_MIN)
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    match scored.as_slice() {
        [] => Found::None,
        [(best, app), rest @ ..] => {
            let close: Vec<&App> = rest
                .iter()
                .filter(|(score, _)| best - score < FUZZY_TIE)
                .map(|(_, app)| *app)
                .collect();
            if close.is_empty() {
                Found::One((*app).clone())
            } else {
                Found::Several(names(std::iter::once(*app).chain(close)))
            }
        }
    }
}

fn names<'a>(apps: impl Iterator<Item = &'a App>) -> Vec<String> {
    let mut names: Vec<String> = apps.map(|app| app.name.clone()).collect();
    names.sort();
    names.truncate(MAX_CHOICES);
    names
}

/// "A, B or C".
fn or_list(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} or {last}", rest.join(", ")),
    }
}

fn text(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn done(result: Result<String, String>) -> Outcome {
    match result {
        Ok(detail) if detail.is_empty() => Outcome::Done("Done.".into()),
        Ok(detail) => Outcome::Done(detail),
        Err(error) => Outcome::Done(format!("Failed: {error}")),
    }
}

/// Percent-encodes everything but unreserved characters and `!`.
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'!' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
