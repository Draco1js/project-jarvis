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

    /// An assistant turn that asks for tools to be run.
    pub fn calls(tool_calls: Vec<ToolCall>) -> Self {
        Self {
            tool_calls,
            ..Self::new(Role::Assistant, "")
        }
    }

    /// The result of running the tool call `call_id`.
    pub fn tool_result(call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            tool_call_id: Some(call_id.into()),
            ..Self::new(Role::Tool, content)
        }
    }
}

/// A request from the model to run one tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// The arguments as a JSON object, as the model wrote them.
    pub arguments: String,
    /// Provider data that must go back with the call unchanged, as raw JSON
    /// (Gemini 3's `extra_content`, which holds the thought signature).
    pub provider_data: Option<String>,
}

/// A tool offered to the model.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON Schema of the arguments object.
    pub parameters: serde_json::Value,
}

/// What the model answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    /// A final answer for the user.
    Text(String),
    /// Run these tools and send the results back.
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
}

/// The clock the app uses.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// Acts on the computer. The core decides *what* to do (which URL, which
/// app, private or not); this only carries it out. Only these actions exist:
/// there is no shell.
pub trait Actions: Send {
    /// The browser links and searches open in: the setting, or the system default.
    fn browser(&mut self) -> Browser;
    /// The spoken-name-to-bang list from the settings.
    fn bangs(&mut self) -> Vec<Bang>;
    /// Every app that can be opened by name.
    fn apps(&mut self) -> Vec<App>;
    /// Do it. `Ok` carries an optional detail for the model ("opened report.pdf").
    fn run(&mut self, action: &Action) -> Result<String, String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Open `url` in [`Actions::browser`], in a private window when `private`.
    OpenUrl {
        url: String,
        private: bool,
    },
    OpenApp(App),
    OpenFolder(Folder),
    /// Open the most recently changed file in a folder.
    OpenRecentFile(Folder),
    Volume(Volume),
    Media(MediaKey),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserKind {
    Chrome,
    Brave,
    Chromium,
    Edge,
    Firefox,
    Opera,
    Safari,
    /// A browser we don't know how to open privately.
    Other,
}

impl BrowserKind {
    /// The command-line flag that opens a private window, if there is one.
    pub fn private_flag(self) -> Option<&'static str> {
        match self {
            Self::Chrome | Self::Brave | Self::Chromium => Some("--incognito"),
            Self::Edge => Some("--inprivate"),
            Self::Firefox => Some("--private-window"),
            Self::Opera => Some("--private"),
            Self::Safari | Self::Other => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Browser {
    pub kind: BrowserKind,
    /// Display name, e.g. "Safari".
    pub name: String,
}

/// A DuckDuckGo-style bang for a site the user may name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bang {
    /// What the user says, e.g. "YouTube".
    pub name: String,
    /// The bang without `!`, e.g. "yt".
    pub bang: String,
}

/// An app found in the app index (Start Menu shortcut, `.app`, `.desktop` file).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    pub name: String,
    /// What the executor launches, e.g. the shortcut's path.
    pub target: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Folder {
    Home,
    Desktop,
    Documents,
    Downloads,
    Pictures,
    Music,
    Videos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Volume {
    /// Set to a percentage, 0 to 100.
    Set(u8),
    /// Raise by percentage points.
    Up(u8),
    /// Lower by percentage points.
    Down(u8),
    Mute,
    Unmute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKey {
    PlayPause,
    Next,
    Previous,
}

/// Everything the assistant needs from outside.
pub struct Ports {
    pub chat: Box<dyn ChatProvider>,
    pub speaker: Box<dyn Speaker>,
    pub store: Box<dyn Store>,
    pub clock: Box<dyn Clock>,
    pub actions: Box<dyn Actions>,
}
