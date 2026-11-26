//! Fake ports for driving the core in tests. Each fake shares its state
//! through an `Arc<Mutex<..>>`, so a test keeps a clone and inspects what the
//! engine did after handing the other clone to the engine.

#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use jarvis_core::{
    Assistant, ChatError, ChatProvider, ChatReply, Clock, Input, MemoryHit, Message, Output, Ports,
    Role, Speaker, Store, Tagged, ToolCall, ToolSpec,
};

#[derive(Clone, Default)]
pub struct FakeChat {
    replies: Arc<Mutex<VecDeque<Result<ChatReply, ChatError>>>>,
    requests: Arc<Mutex<Vec<Vec<Message>>>>,
    offered: Arc<Mutex<Vec<Vec<&'static str>>>>,
}

impl FakeChat {
    pub fn will_reply(&self, text: &str) -> &Self {
        self.replies
            .lock()
            .unwrap()
            .push_back(Ok(ChatReply::Text(text.to_string())));
        self
    }

    /// The next reply asks to run one tool with these JSON arguments.
    pub fn will_call(&self, tool: &str, arguments: &str) -> &Self {
        let mut replies = self.replies.lock().unwrap();
        let id = format!("call-{}", replies.len());
        replies.push_back(Ok(ChatReply::ToolCalls(vec![ToolCall {
            id,
            name: tool.to_string(),
            arguments: arguments.to_string(),
            extra: Some(r#"{"google":{"thought_signature":"sig"}}"#.to_string()),
        }])));
        self
    }

    pub fn will_fail(&self, error: ChatError) -> &Self {
        self.replies.lock().unwrap().push_back(Err(error));
        self
    }

    /// Every message list the engine sent, in order.
    pub fn requests(&self) -> Vec<Vec<Message>> {
        self.requests.lock().unwrap().clone()
    }

    pub fn last_request(&self) -> Vec<Message> {
        self.requests()
            .last()
            .cloned()
            .expect("no chat request made")
    }

    /// Tool names offered with each request (empty for plain requests).
    pub fn offered_tools(&self) -> Vec<Vec<&'static str>> {
        self.offered.lock().unwrap().clone()
    }

    fn next(&self, messages: &[Message], tools: &[ToolSpec]) -> Result<ChatReply, ChatError> {
        self.requests.lock().unwrap().push(messages.to_vec());
        self.offered
            .lock()
            .unwrap()
            .push(tools.iter().map(|t| t.name).collect());
        self.replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("FakeChat got a request it had no reply for")
    }
}

impl ChatProvider for FakeChat {
    fn chat(&mut self, messages: &[Message]) -> Result<String, ChatError> {
        match self.next(messages, &[])? {
            ChatReply::Text(text) => Ok(text),
            ChatReply::ToolCalls(_) => panic!("tool call queued for a request without tools"),
        }
    }

    fn chat_with_tools(
        &mut self,
        messages: &[Message],
        tools: &[ToolSpec],
    ) -> Result<ChatReply, ChatError> {
        self.next(messages, tools)
    }
}

#[derive(Clone, Default)]
pub struct FakeSpeaker {
    spoken: Arc<Mutex<Vec<String>>>,
}

impl FakeSpeaker {
    pub fn spoken(&self) -> Vec<String> {
        self.spoken.lock().unwrap().clone()
    }
}

impl Speaker for FakeSpeaker {
    fn speak(&mut self, text: &str) {
        self.spoken.lock().unwrap().push(text.to_string());
    }
}

/// A saved note: when, its text, and its summary and tags.
pub type SavedNote = (SystemTime, String, Tagged);

#[derive(Clone, Default)]
pub struct FakeStore {
    saved: Arc<Mutex<Vec<(SystemTime, Role, String)>>>,
    ended: Arc<Mutex<Vec<(SystemTime, Tagged)>>>,
    clears: Arc<Mutex<usize>>,
    searches: Arc<Mutex<Vec<String>>>,
    hits: Arc<Mutex<Vec<MemoryHit>>>,
    notes: Arc<Mutex<Vec<SavedNote>>>,
    note_error: Arc<Mutex<Option<String>>>,
}

impl FakeStore {
    pub fn saved(&self) -> Vec<(SystemTime, Role, String)> {
        self.saved.lock().unwrap().clone()
    }

    /// Conversations closed, with the time and summary.
    pub fn ended(&self) -> Vec<(SystemTime, Tagged)> {
        self.ended.lock().unwrap().clone()
    }

    pub fn clears(&self) -> usize {
        *self.clears.lock().unwrap()
    }

    /// Every search query, in order.
    pub fn searches(&self) -> Vec<String> {
        self.searches.lock().unwrap().clone()
    }

    /// Searches return these hits.
    pub fn will_find(&self, hits: Vec<MemoryHit>) {
        *self.hits.lock().unwrap() = hits;
    }

    pub fn notes(&self) -> Vec<SavedNote> {
        self.notes.lock().unwrap().clone()
    }

    /// Saving a note fails with this reason.
    pub fn notes_fail(&self, reason: &str) {
        *self.note_error.lock().unwrap() = Some(reason.to_string());
    }
}

impl Store for FakeStore {
    fn save_message(&mut self, at: SystemTime, message: &Message) {
        self.saved
            .lock()
            .unwrap()
            .push((at, message.role, message.content.clone()));
    }

    fn end_conversation(&mut self, at: SystemTime, tagged: &Tagged) {
        self.ended.lock().unwrap().push((at, tagged.clone()));
    }

    fn clear_history(&mut self) {
        *self.clears.lock().unwrap() += 1;
    }

    fn search(&mut self, query: &str, limit: usize) -> Vec<MemoryHit> {
        self.searches.lock().unwrap().push(query.to_string());
        self.hits
            .lock()
            .unwrap()
            .iter()
            .take(limit)
            .cloned()
            .collect()
    }

    fn save_note(&mut self, at: SystemTime, text: &str, tagged: &Tagged) -> Result<String, String> {
        if let Some(reason) = self.note_error.lock().unwrap().clone() {
            return Err(reason);
        }
        self.notes
            .lock()
            .unwrap()
            .push((at, text.to_string(), tagged.clone()));
        Ok("Jarvis Notes/note.md".to_string())
    }
}

#[derive(Clone)]
pub struct FakeClock {
    now: Arc<Mutex<SystemTime>>,
}

impl FakeClock {
    pub fn starting_at(start: SystemTime) -> Self {
        Self {
            now: Arc::new(Mutex::new(start)),
        }
    }

    pub fn advance(&self, by: Duration) {
        *self.now.lock().unwrap() += by;
    }
}

impl Clock for FakeClock {
    fn now(&self) -> SystemTime {
        *self.now.lock().unwrap()
    }
}

/// The start time every test clock uses: 2026-11-26 12:00:00 UTC.
pub fn start_time() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_795_694_400)
}

/// An assistant wired to fresh fakes, plus handles to inspect them.
pub struct Harness {
    pub assistant: Assistant,
    pub chat: FakeChat,
    pub speaker: FakeSpeaker,
    pub store: FakeStore,
    pub clock: FakeClock,
}

impl Harness {
    pub fn new() -> Self {
        let chat = FakeChat::default();
        let speaker = FakeSpeaker::default();
        let store = FakeStore::default();
        let clock = FakeClock::starting_at(start_time());
        let assistant = Assistant::new(Ports {
            chat: Box::new(chat.clone()),
            speaker: Box::new(speaker.clone()),
            store: Box::new(store.clone()),
            clock: Box::new(clock.clone()),
        });
        Self {
            assistant,
            chat,
            speaker,
            store,
            clock,
        }
    }

    /// Feed one input and collect everything the assistant emitted.
    pub fn send(&mut self, input: Input) -> Vec<Output> {
        let mut outputs = Vec::new();
        self.assistant.handle(input, &mut |o| outputs.push(o));
        outputs
    }

    /// The app's periodic timer firing.
    pub fn tick(&mut self) -> Vec<Output> {
        self.send(Input::Tick)
    }

    pub fn type_text(&mut self, text: &str) -> Vec<Output> {
        self.send(Input::Typed(text.to_string()))
    }
}
