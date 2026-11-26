//! The interfaces the core uses to reach the outside world.

use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

impl Message {
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
        }
    }
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
    /// Send the whole conversation, system prompt first; returns the reply text.
    fn chat(&mut self, messages: &[Message]) -> Result<String, ChatError>;
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
}

/// The clock the app uses.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// Everything the assistant needs from outside.
pub struct Ports {
    pub chat: Box<dyn ChatProvider>,
    pub speaker: Box<dyn Speaker>,
    pub store: Box<dyn Store>,
    pub clock: Box<dyn Clock>,
}
