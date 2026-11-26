//! Headless assistant engine: turn state machine, context, provider fallback.
//!
//! The engine talks to every outside service through the port traits in
//! [`ports`], so it runs the same with real adapters in the app and with
//! fakes in tests. It takes [`Input`] events and emits [`Output`] events for
//! whatever window or panel is showing the conversation.

mod actions;
mod assistant;
pub mod ports;

pub use assistant::{Assistant, Input, Output};
pub use ports::{
    Action, Actions, App, Bang, Browser, BrowserKind, ChatError, ChatProvider, Clock, Folder,
    MediaKey, Message, Ports, Reply, Role, Speaker, Store, ToolCall, ToolSpec, Volume,
};
