//! The tools offered to the model, and running the calls it makes.
//!
//! Timers, alarms and reminders are kept through the [`Reminders`] port and go
//! off on [`Input::Tick`](crate::Input::Tick); weather goes to the [`Weather`]
//! port.
//!
//! [`Reminders`]: crate::ports::Reminders
//! [`Weather`]: crate::ports::Weather

use std::time::{Duration, SystemTime};

use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone, Utc};
use serde_json::{Value, json};

use crate::assistant::Output;
use crate::memory;
use crate::ports::{Clock, Ports, Reminder, ReminderKind, ToolCall, ToolSpec};

/// A reminder announced this long after it was due counts as late (missed).
pub const LATE_AFTER: Duration = Duration::from_secs(60);

/// Every tool the model may call.
pub fn specs() -> Vec<ToolSpec> {
    let at = json!({
        "type": "string",
        "description": "Local date and time, ISO 8601 with the UTC offset, e.g. 2026-11-27T07:00:00+05:30.",
    });
    vec![
        ToolSpec {
            name: "set_timer",
            description: "Start a countdown timer.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "seconds": {"type": "integer", "description": "Length of the timer in seconds."},
                    "label": {"type": "string", "description": "Optional short name, e.g. \"tea\"."},
                },
                "required": ["seconds"],
            }),
        },
        ToolSpec {
            name: "set_alarm",
            description: "Set an alarm for a time of day. For \"7 am\" pass the next 7:00 to come.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "at": at,
                    "label": {"type": "string", "description": "Optional short name."},
                },
                "required": ["at"],
            }),
        },
        ToolSpec {
            name: "set_reminder",
            description: "Remind the user of something at a given time.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "at": at,
                    "text": {"type": "string", "description": "What to remind them of, e.g. \"call mum\"."},
                },
                "required": ["at", "text"],
            }),
        },
        ToolSpec {
            name: "list_reminders",
            description: "List the timers, alarms and reminders that are set, with their ids.",
            parameters: json!({"type": "object", "properties": {}}),
        },
        ToolSpec {
            name: "cancel_reminder",
            description: "Cancel a timer, alarm or reminder by id (from list_reminders or when it was set).",
            parameters: json!({
                "type": "object",
                "properties": {"id": {"type": "integer"}},
                "required": ["id"],
            }),
        },
        memory::search_spec(),
        ToolSpec {
            name: "get_weather",
            description: "Current weather and the forecast for today and the next two days. \
Leave out place for where the user is.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "place": {"type": "string", "description": "A city or place the user named."},
                },
            }),
        },
    ]
}

/// Facts the model needs in the system prompt: the current local time.
pub fn prompt_context(clock: &dyn Clock) -> String {
    let now = local(clock, clock.now());
    format!(
        "The current local time is {} ({}). Use the tools for timers, alarms, reminders and weather, \
and give times to them in that ISO 8601 form.",
        now.format("%A %-d %B %Y, %H:%M"),
        now.to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
    )
}

/// Run one tool call and return the text to send back to the model.
pub fn run(ports: &mut Ports, call: &ToolCall) -> String {
    let args: Value = serde_json::from_str(&call.arguments).unwrap_or(Value::Null);
    let text = |key: &str| args[key].as_str().unwrap_or("").trim().to_string();
    let result = match call.name.as_str() {
        "set_timer" => match args["seconds"].as_u64().filter(|s| *s > 0) {
            Some(seconds) => {
                let due = ports.clock.now() + Duration::from_secs(seconds);
                Ok(add(ports, ReminderKind::Timer, &text("label"), due))
            }
            None => Err("seconds must be a whole number above 0".to_string()),
        },
        "set_alarm" => future_time(ports, &text("at"))
            .map(|due| add(ports, ReminderKind::Alarm, &text("label"), due)),
        "set_reminder" => future_time(ports, &text("at"))
            .map(|due| add(ports, ReminderKind::Reminder, &text("text"), due)),
        "list_reminders" => Ok(list(ports)),
        "cancel_reminder" => match args["id"].as_i64() {
            Some(id) if ports.reminders.remove(id) => Ok(format!("Cancelled {id}.")),
            Some(id) => Err(format!("nothing is set with id {id}")),
            None => Err("id must be a number".to_string()),
        },
        "get_weather" => {
            let place = text("place");
            let place = (!place.is_empty()).then_some(place.as_str());
            ports
                .weather
                .report(place)
                .map_err(|e| format!("no weather report: {e}"))
        }
        "search_memory" => memory::search(ports, &text("query")),
        other => Err(format!("there is no tool called {other}")),
    };
    result.unwrap_or_else(|error| format!("Error: {error}."))
}

fn add(ports: &mut Ports, kind: ReminderKind, text: &str, due: SystemTime) -> String {
    let id = ports.reminders.add(kind, text, due);
    format!(
        "{} {id} set, going off at {}.",
        kind_name(kind),
        when(ports.clock.as_ref(), due)
    )
}

fn list(ports: &mut Ports) -> String {
    let pending = ports.reminders.pending();
    if pending.is_empty() {
        return "No timers, alarms or reminders are set.".into();
    }
    pending
        .iter()
        .map(|r| {
            format!(
                "{}: {} \"{}\" at {}",
                r.id,
                kind_name(r.kind),
                r.text,
                when(ports.clock.as_ref(), r.due)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn kind_name(kind: ReminderKind) -> &'static str {
    match kind {
        ReminderKind::Timer => "Timer",
        ReminderKind::Alarm => "Alarm",
        ReminderKind::Reminder => "Reminder",
    }
}

/// Parse a time the model gave and check it is still to come.
fn future_time(ports: &Ports, at: &str) -> Result<SystemTime, String> {
    let due = parse_time(ports.clock.as_ref(), at)
        .ok_or_else(|| format!("could not read the time {at:?}; use ISO 8601"))?;
    if due <= ports.clock.now() {
        return Err(format!(
            "{at} has already passed; it is now {}",
            when(ports.clock.as_ref(), ports.clock.now())
        ));
    }
    Ok(due)
}

/// ISO 8601 with an offset, or without one (taken as local time).
fn parse_time(clock: &dyn Clock, at: &str) -> Option<SystemTime> {
    if let Ok(time) = DateTime::parse_from_rfc3339(at) {
        return Some(time.into());
    }
    let naive = [
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
    ]
    .iter()
    .find_map(|format| NaiveDateTime::parse_from_str(at, format).ok())?;
    offset(clock)
        .from_local_datetime(&naive)
        .single()
        .map(Into::into)
}

fn offset(clock: &dyn Clock) -> FixedOffset {
    FixedOffset::east_opt(clock.utc_offset_secs()).unwrap_or(FixedOffset::east_opt(0).unwrap())
}

fn local(clock: &dyn Clock, time: SystemTime) -> DateTime<FixedOffset> {
    DateTime::<Utc>::from(time).with_timezone(&offset(clock))
}

/// "17:35", or "17:35 on Thu 26 Nov" when it is not today.
fn when(clock: &dyn Clock, time: SystemTime) -> String {
    let time = local(clock, time);
    let today = local(clock, clock.now()).date_naive();
    if time.date_naive() == today {
        time.format("%H:%M").to_string()
    } else {
        time.format("%H:%M on %a %-d %b").to_string()
    }
}

/// Announce every timer, alarm and reminder that is due: a native
/// notification, a spoken line and an [`Output::ReminderDue`]. Ones that went
/// off while Jarvis was not running are marked late.
pub fn fire_due_reminders(ports: &mut Ports, emit: &mut dyn FnMut(Output)) {
    let now = ports.clock.now();
    for reminder in ports.reminders.pending() {
        if reminder.due > now {
            break;
        }
        let late = now.duration_since(reminder.due).unwrap_or_default() >= LATE_AFTER;
        let (title, body) = announcement(ports.clock.as_ref(), &reminder, late);
        ports.notifier.notify(title, &body);
        ports.speaker.speak(&spoken(title, &reminder.text, late));
        ports.reminders.remove(reminder.id);
        emit(Output::ReminderDue {
            title: title.into(),
            body,
            late,
        });
    }
}

fn announcement(clock: &dyn Clock, reminder: &Reminder, late: bool) -> (&'static str, String) {
    let title = match reminder.kind {
        ReminderKind::Timer => "Timer done",
        ReminderKind::Alarm => "Alarm",
        ReminderKind::Reminder => "Reminder",
    };
    let mut body = reminder.text.clone();
    if late {
        let note = format!("late: was due at {}", when(clock, reminder.due));
        body = if body.is_empty() {
            note
        } else {
            format!("{body} ({note})")
        };
    }
    (title, body)
}

fn spoken(title: &str, text: &str, late: bool) -> String {
    let late = if late {
        " This one was missed earlier."
    } else {
        ""
    };
    if text.is_empty() {
        format!("{title}.{late}")
    } else {
        format!("{title}: {text}.{late}")
    }
}
