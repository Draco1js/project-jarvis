//! Fake ports for driving the core in tests. Each fake shares its state
//! through an `Arc<Mutex<..>>`, so a test keeps a clone and inspects what the
//! engine did after handing the other clone to the engine.

#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use jarvis_core::{
    Assistant, ChatError, ChatProvider, Clock, Input, Message, Output, Ports, Role, Speaker, Store,
};

#[derive(Clone, Default)]
pub struct FakeChat {
    replies: Arc<Mutex<VecDeque<Result<String, ChatError>>>>,
    requests: Arc<Mutex<Vec<Vec<Message>>>>,
}

impl FakeChat {
    pub fn will_reply(&self, text: &str) -> &Self {
        self.replies.lock().unwrap().push_back(Ok(text.to_string()));
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
}

impl ChatProvider for FakeChat {
    fn chat(&mut self, messages: &[Message]) -> Result<String, ChatError> {
        self.requests.lock().unwrap().push(messages.to_vec());
        self.replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("FakeChat got a request it had no reply for")
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

#[derive(Clone, Default)]
pub struct FakeStore {
    saved: Arc<Mutex<Vec<(SystemTime, Role, String)>>>,
}

impl FakeStore {
    pub fn saved(&self) -> Vec<(SystemTime, Role, String)> {
        self.saved.lock().unwrap().clone()
    }
}

impl Store for FakeStore {
    fn save_message(&mut self, at: SystemTime, message: &Message) {
        self.saved
            .lock()
            .unwrap()
            .push((at, message.role, message.content.clone()));
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

    pub fn type_text(&mut self, text: &str) -> Vec<Output> {
        self.send(Input::Typed(text.to_string()))
    }
}
