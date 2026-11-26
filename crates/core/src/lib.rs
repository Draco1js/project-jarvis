//! Headless assistant engine: turn state machine, context, provider fallback.
//!
//! The engine talks to every outside service through the port traits in
//! [`ports`], so it runs the same with real adapters in the app and with
//! fakes in tests. It takes [`Input`] events and emits [`Output`] events for
//! whatever window or panel is showing the conversation.

mod assistant;
pub mod ports;

pub use assistant::{Assistant, Input, Output};
pub use ports::{ChatError, ChatProvider, Clock, Message, Ports, Role, Speaker, Store};
