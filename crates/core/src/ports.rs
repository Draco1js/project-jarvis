//! The interfaces the core uses to reach the outside world.

use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    System,
    User,
    Assistant,
    /// The result of a tool call, answering the assistant message that asked for it.
    Tool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub role: Role,
    pub content: String,
    /// Tools the assistant asked to run (assistant messages only).
    pub tool_calls: Vec<ToolCall>,
    /// The call this message answers (tool messages only).
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

    /// An assistant turn that asks for tools instead of answering.
    pub fn tool_request(calls: Vec<ToolCall>) -> Self {
        Self {
            tool_calls: calls,
            ..Self::new(Role::Assistant, "")
        }
    }

    /// The result of running `call`.
    pub fn tool_result(call: &ToolCall, content: impl Into<String>) -> Self {
        Self {
            tool_call_id: Some(call.id.clone()),
            ..Self::new(Role::Tool, content)
        }
    }
}

/// A tool the model may call. `parameters` is a JSON Schema object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: &'static str,
}

/// The model asking to run one tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// The arguments as a JSON object.
    pub arguments: String,
    /// Provider data that must be sent back with the call unchanged
    /// (Gemini 3's thought signature rides here), as JSON.
    pub extra: Option<String>,
}

/// What a chat request with tools produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatReply {
    /// A plain answer.
    Text(String),
    /// The model wants these tools run before it answers.
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
    /// Send the whole conversation, system prompt first; returns the reply text.
    fn chat(&mut self, messages: &[Message]) -> Result<String, ChatError>;

    /// Like [`chat`](Self::chat), offering `tools` the model may call.
    /// Providers without tool support just answer.
    fn chat_with_tools(
        &mut self,
        messages: &[Message],
        tools: &[ToolSpec],
    ) -> Result<ChatReply, ChatError> {
        let _ = tools;
        self.chat(messages).map(ChatReply::Text)
    }
}

/// Speech output.
pub trait Speaker: Send {
    /// Start saying `text`, replacing anything still being said.
    fn speak(&mut self, text: &str);
}

/// A short summary plus search tags, written by the model for a finished
/// conversation or a new note.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Tagged {
    pub summary: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryKind {
    Conversation,
    Note,
}

/// One search result from memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryHit {
    pub kind: MemoryKind,
    /// When the conversation started or the note was written.
    pub at: SystemTime,
    pub summary: String,
    pub tags: Vec<String>,
    /// The matching part of the conversation or note text.
    pub excerpt: String,
}

/// Persistent history, memory and notes.
pub trait Store: Send {
    /// Add a message to the current conversation, starting one if needed.
    fn save_message(&mut self, at: SystemTime, message: &Message);
    /// Close the current conversation with its summary and tags, making it searchable.
    fn end_conversation(&mut self, at: SystemTime, tagged: &Tagged);
    /// Delete all saved conversations. Notes are kept.
    fn clear_history(&mut self);
    /// Full-text search over conversations and notes, best match first.
    fn search(&mut self, query: &str, limit: usize) -> Vec<MemoryHit>;
    /// Save a note; returns where it went, or why it could not be saved.
    fn save_note(&mut self, at: SystemTime, text: &str, tagged: &Tagged) -> Result<String, String>;
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
