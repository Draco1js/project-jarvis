//! Fake ports for driving the core in tests. Each fake shares its state
//! through an `Arc<Mutex<..>>`, so a test keeps a clone and inspects what the
//! engine did after handing the other clone to the engine.

#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use jarvis_core::{
    Assistant, ChatError, ChatProvider, Clock, Input, Message, Notifier, Output, Ports, Reminder,
    ReminderKind, Reminders, Reply, Role, Speaker, Store, ToolCall, ToolSpec, Weather,
};

#[derive(Clone, Default)]
pub struct FakeChat {
    replies: Arc<Mutex<VecDeque<Result<Reply, ChatError>>>>,
    requests: Arc<Mutex<Vec<Vec<Message>>>>,
    tools: Arc<Mutex<Vec<Vec<String>>>>,
    next_call: Arc<Mutex<u32>>,
}

impl FakeChat {
    pub fn will_reply(&self, text: &str) -> &Self {
        self.replies
            .lock()
            .unwrap()
            .push_back(Ok(Reply::Text(text.to_string())));
        self
    }

    /// The next reply asks to call the tool `name` with these JSON arguments.
    pub fn will_call(&self, name: &str, arguments: serde_json::Value) -> &Self {
        let mut next = self.next_call.lock().unwrap();
        *next += 1;
        let call = ToolCall {
            id: format!("call-{next}"),
            name: name.to_string(),
            arguments: arguments.to_string(),
            provider_data: None,
        };
        self.replies
            .lock()
            .unwrap()
            .push_back(Ok(Reply::ToolCalls(vec![call])));
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

    /// The names of the tools offered with the last request.
    pub fn last_tools(&self) -> Vec<String> {
        self.tools
            .lock()
            .unwrap()
            .last()
            .cloned()
            .unwrap_or_default()
    }

    /// What the tools returned, as last sent back to the model.
    pub fn last_tool_results(&self) -> Vec<String> {
        self.last_request()
            .into_iter()
            .filter(|m| m.role == Role::Tool)
            .map(|m| m.content)
            .collect()
    }
}

impl ChatProvider for FakeChat {
    fn chat(&mut self, messages: &[Message], tools: &[ToolSpec]) -> Result<Reply, ChatError> {
        self.requests.lock().unwrap().push(messages.to_vec());
        self.tools
            .lock()
            .unwrap()
            .push(tools.iter().map(|t| t.name.to_string()).collect());
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
    offset: Arc<Mutex<i32>>,
}

impl FakeClock {
    pub fn starting_at(start: SystemTime) -> Self {
        Self {
            now: Arc::new(Mutex::new(start)),
            offset: Arc::new(Mutex::new(0)),
        }
    }

    pub fn advance(&self, by: Duration) {
        *self.now.lock().unwrap() += by;
    }

    /// Put the clock in a time zone `secs` ahead of UTC.
    pub fn set_utc_offset(&self, secs: i32) {
        *self.offset.lock().unwrap() = secs;
    }
}

impl Clock for FakeClock {
    fn now(&self) -> SystemTime {
        *self.now.lock().unwrap()
    }

    fn utc_offset_secs(&self) -> i32 {
        *self.offset.lock().unwrap()
    }
}

/// In-memory reminders. Clones share the same list, like one database file.
#[derive(Clone, Default)]
pub struct FakeReminders {
    items: Arc<Mutex<Vec<Reminder>>>,
    next_id: Arc<Mutex<i64>>,
}

impl FakeReminders {
    pub fn all(&self) -> Vec<Reminder> {
        self.items.lock().unwrap().clone()
    }
}

impl Reminders for FakeReminders {
    fn add(&mut self, kind: ReminderKind, text: &str, due: SystemTime) -> i64 {
        let mut next = self.next_id.lock().unwrap();
        *next += 1;
        self.items.lock().unwrap().push(Reminder {
            id: *next,
            kind,
            text: text.to_string(),
            due,
        });
        *next
    }

    fn pending(&mut self) -> Vec<Reminder> {
        let mut items = self.all();
        items.sort_by_key(|r| r.due);
        items
    }

    fn remove(&mut self, id: i64) -> bool {
        let mut items = self.items.lock().unwrap();
        let before = items.len();
        items.retain(|r| r.id != id);
        items.len() != before
    }
}

#[derive(Clone, Default)]
pub struct FakeNotifier {
    shown: Arc<Mutex<Vec<(String, String)>>>,
}

impl FakeNotifier {
    /// Every notification shown, as (title, body).
    pub fn shown(&self) -> Vec<(String, String)> {
        self.shown.lock().unwrap().clone()
    }
}

impl Notifier for FakeNotifier {
    fn notify(&mut self, title: &str, body: &str) {
        self.shown
            .lock()
            .unwrap()
            .push((title.to_string(), body.to_string()));
    }
}

#[derive(Clone, Default)]
pub struct FakeWeather {
    asked: Arc<Mutex<Vec<Option<String>>>>,
    answer: Arc<Mutex<Option<Result<String, String>>>>,
}

impl FakeWeather {
    pub fn will_report(&self, answer: Result<&str, &str>) {
        *self.answer.lock().unwrap() = Some(answer.map(str::to_string).map_err(str::to_string));
    }

    /// The place asked for in each lookup (`None` = the user's own location).
    pub fn asked(&self) -> Vec<Option<String>> {
        self.asked.lock().unwrap().clone()
    }
}

impl Weather for FakeWeather {
    fn report(&mut self, place: Option<&str>) -> Result<String, String> {
        self.asked.lock().unwrap().push(place.map(str::to_string));
        self.answer
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| Ok("Sunny, 25°C.".into()))
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
    pub reminders: FakeReminders,
    pub notifier: FakeNotifier,
    pub weather: FakeWeather,
}

impl Harness {
    pub fn new() -> Self {
        let chat = FakeChat::default();
        let speaker = FakeSpeaker::default();
        let store = FakeStore::default();
        let clock = FakeClock::starting_at(start_time());
        let reminders = FakeReminders::default();
        let notifier = FakeNotifier::default();
        let weather = FakeWeather::default();
        let assistant = Self::assistant(
            &chat, &speaker, &store, &clock, &reminders, &notifier, &weather,
        );
        Self {
            assistant,
            chat,
            speaker,
            store,
            clock,
            reminders,
            notifier,
            weather,
        }
    }

    fn assistant(
        chat: &FakeChat,
        speaker: &FakeSpeaker,
        store: &FakeStore,
        clock: &FakeClock,
        reminders: &FakeReminders,
        notifier: &FakeNotifier,
        weather: &FakeWeather,
    ) -> Assistant {
        Assistant::new(Ports {
            chat: Box::new(chat.clone()),
            speaker: Box::new(speaker.clone()),
            store: Box::new(store.clone()),
            clock: Box::new(clock.clone()),
            reminders: Box::new(reminders.clone()),
            notifier: Box::new(notifier.clone()),
            weather: Box::new(weather.clone()),
        })
    }

    /// Quit Jarvis and start it again: a new assistant on the same saved data.
    pub fn restart(&mut self) {
        self.assistant = Self::assistant(
            &self.chat,
            &self.speaker,
            &self.store,
            &self.clock,
            &self.reminders,
            &self.notifier,
            &self.weather,
        );
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

    pub fn tick(&mut self) -> Vec<Output> {
        self.send(Input::Tick)
    }
}
