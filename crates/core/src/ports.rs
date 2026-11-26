//! The interfaces the core uses to reach the outside world.

use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    System,
    User,
    Assistant,
    /// The result of a tool call, answering the call with `tool_call_id`.
    Tool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub role: Role,
    pub content: String,
    /// Tools the model asked to call (assistant messages only).
    pub tool_calls: Vec<ToolCall>,
    /// Which call this message answers (tool messages only).
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    /// The model's request to call tools, kept in the conversation.
    pub fn tool_calls(calls: Vec<ToolCall>) -> Self {
        Self {
            tool_calls: calls,
            ..Self::new(Role::Assistant, "")
        }
    }

    /// What a tool call returned, sent back to the model.
    pub fn tool_result(call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            tool_call_id: Some(call_id.into()),
            ..Self::new(Role::Tool, content)
        }
    }
}

/// A tool the model may call, described as a JSON-schema function.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    /// JSON schema of the arguments object.
    pub parameters: serde_json::Value,
}

/// One tool call the model asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// The arguments as the JSON text the model sent.
    pub arguments: String,
    /// Provider data that must go back unchanged with this call (Gemini 3's
    /// thought signature lives here), as JSON text. The core never reads it.
    pub provider_data: Option<String>,
}

/// What the model answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    /// A final answer for the user.
    Text(String),
    /// Run these tools and send back their results.
    ToolCalls(Vec<ToolCall>),
}

/// Why a chat request produced no reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatError {
    /// No API key is configured.
    MissingKey,
    /// The provider rejected the key.
    InvalidKey,
    /// Rate-limited. `retry_after` is the reset time the provider gave, if any.
    RateLimited { retry_after: Option<Duration> },
    /// The provider answered with a server error (5xx, including 524).
    Server { status: u16 },
    /// No answer within the request timeout.
    Timeout,
    /// The provider could not be reached at all.
    Unreachable,
    /// Any other failure: an unexpected status or a reply that did not parse.
    Other(String),
}

/// A chat model (Gemini, later the local llama-server).
pub trait ChatProvider: Send {
    /// Send the whole conversation, system prompt first, offering `tools`.
    fn chat(&mut self, messages: &[Message], tools: &[ToolSpec]) -> Result<Reply, ChatError>;
}

/// Speech output.
pub trait Speaker: Send {
    /// Start saying `text`, replacing anything still being said.
    fn speak(&mut self, text: &str);
}

/// Persistent history.
pub trait Store: Send {
    fn save_message(&mut self, at: SystemTime, message: &Message);
}

/// Wall-clock time, so tests can control it.
pub trait Clock: Send {
    fn now(&self) -> SystemTime;

    /// The local time zone's offset from UTC right now, in seconds.
    fn utc_offset_secs(&self) -> i32 {
        0
    }
}

/// The clock the app uses.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }

    fn utc_offset_secs(&self) -> i32 {
        chrono::Local::now().offset().local_minus_utc()
    }
}

/// What a reminder is, which decides how it is announced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReminderKind {
    Timer,
    Alarm,
    Reminder,
}

/// A timer, alarm or reminder waiting to go off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reminder {
    pub id: i64,
    pub kind: ReminderKind,
    /// The label ("tea") or reminder text ("call mum"); may be empty.
    pub text: String,
    pub due: SystemTime,
}

/// Saved timers, alarms and reminders. Must survive restarts.
pub trait Reminders: Send {
    /// Save a new one and return its id.
    fn add(&mut self, kind: ReminderKind, text: &str, due: SystemTime) -> i64;
    /// Every one that has not gone off or been cancelled, soonest first.
    fn pending(&mut self) -> Vec<Reminder>;
    /// Forget one (it went off or was cancelled). False if there was none.
    fn remove(&mut self, id: i64) -> bool;
}

/// Native desktop notifications.
pub trait Notifier: Send {
    fn notify(&mut self, title: &str, body: &str);
}

/// Weather reports (wttr.in in the app).
pub trait Weather: Send {
    /// A short plain-text report for `place`, or for the user's own location
    /// (city setting, else IP location) when `place` is `None`.
    fn report(&mut self, place: Option<&str>) -> Result<String, String>;
}

/// Everything the assistant needs from outside.
pub struct Ports {
    pub chat: Box<dyn ChatProvider>,
    pub speaker: Box<dyn Speaker>,
    pub store: Box<dyn Store>,
    pub clock: Box<dyn Clock>,
    pub reminders: Box<dyn Reminders>,
    pub notifier: Box<dyn Notifier>,
    pub weather: Box<dyn Weather>,
}
