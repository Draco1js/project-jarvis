//! Fake ports for driving the core in tests. Each fake shares its state
//! through an `Arc<Mutex<..>>`, so a test keeps a clone and inspects what the
//! engine did after handing the other clone to the engine.

#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use jarvis_core::{
    Action, Actions, App, Assistant, Bang, Browser, BrowserKind, ChatError, ChatProvider, Clock,
    Input, Message, Output, Ports, Reply, Role, Speaker, Store, ToolCall, ToolSpec,
};

#[derive(Clone, Default)]
pub struct FakeChat {
    replies: Arc<Mutex<VecDeque<Result<Reply, ChatError>>>>,
    requests: Arc<Mutex<Vec<Vec<Message>>>>,
    tools: Arc<Mutex<Vec<Vec<ToolSpec>>>>,
    calls_made: Arc<Mutex<usize>>,
}

impl FakeChat {
    pub fn will_reply(&self, text: &str) -> &Self {
        self.replies
            .lock()
            .unwrap()
            .push_back(Ok(Reply::Text(text.to_string())));
        self
    }

    pub fn will_fail(&self, error: ChatError) -> &Self {
        self.replies.lock().unwrap().push_back(Err(error));
        self
    }

    /// The model will ask for one tool call; `arguments` is a JSON object.
    pub fn will_call(&self, name: &str, arguments: serde_json::Value) -> &Self {
        let id = {
            let mut n = self.calls_made.lock().unwrap();
            *n += 1;
            format!("call-{n}")
        };
        self.replies
            .lock()
            .unwrap()
            .push_back(Ok(Reply::ToolCalls(vec![ToolCall {
                id,
                name: name.to_string(),
                arguments: arguments.to_string(),
                provider_data: None,
            }])));
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

    /// The tools offered with the last request.
    pub fn last_tools(&self) -> Vec<ToolSpec> {
        self.tools
            .lock()
            .unwrap()
            .last()
            .cloned()
            .unwrap_or_default()
    }

    /// The tool called `name` from the last request.
    pub fn tool(&self, name: &str) -> ToolSpec {
        self.last_tools()
            .into_iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("no tool called {name} was offered"))
    }

    /// The tool results in the last request, in order.
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
        self.tools.lock().unwrap().push(tools.to_vec());
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

/// The fake action executor: records every action instead of doing it.
#[derive(Clone)]
pub struct FakeActions {
    browser: Arc<Mutex<Browser>>,
    bangs: Arc<Mutex<Vec<Bang>>>,
    apps: Arc<Mutex<Vec<App>>>,
    done: Arc<Mutex<Vec<Action>>>,
    results: Arc<Mutex<VecDeque<Result<String, String>>>>,
}

fn bangs(list: &[(&str, &str)]) -> Vec<Bang> {
    list.iter()
        .map(|(name, bang)| Bang {
            name: name.to_string(),
            bang: bang.to_string(),
        })
        .collect()
}

impl Default for FakeActions {
    fn default() -> Self {
        Self {
            browser: Arc::new(Mutex::new(Browser {
                kind: BrowserKind::Chrome,
                name: "Google Chrome".into(),
            })),
            bangs: Arc::new(Mutex::new(bangs(&[
                ("YouTube", "yt"),
                ("Reddit", "r"),
                ("GitHub", "gh"),
                ("Amazon", "a"),
                ("Wikipedia", "w"),
            ]))),
            apps: Arc::default(),
            done: Arc::default(),
            results: Arc::default(),
        }
    }
}

impl FakeActions {
    pub fn use_browser(&self, kind: BrowserKind, name: &str) {
        *self.browser.lock().unwrap() = Browser {
            kind,
            name: name.into(),
        };
    }

    pub fn set_bangs(&self, list: &[(&str, &str)]) {
        *self.bangs.lock().unwrap() = bangs(list);
    }

    /// Installed apps, by name; each app's target is `<name>.lnk`.
    pub fn install(&self, names: &[&str]) {
        *self.apps.lock().unwrap() = names
            .iter()
            .map(|name| App {
                name: name.to_string(),
                target: format!("{name}.lnk"),
            })
            .collect();
    }

    /// The next action will produce this result.
    pub fn will_return(&self, result: Result<&str, &str>) {
        self.results
            .lock()
            .unwrap()
            .push_back(result.map(str::to_string).map_err(str::to_string));
    }

    /// Every action run, in order.
    pub fn done(&self) -> Vec<Action> {
        self.done.lock().unwrap().clone()
    }
}

impl Actions for FakeActions {
    fn browser(&mut self) -> Browser {
        self.browser.lock().unwrap().clone()
    }

    fn bangs(&mut self) -> Vec<Bang> {
        self.bangs.lock().unwrap().clone()
    }

    fn apps(&mut self) -> Vec<App> {
        self.apps.lock().unwrap().clone()
    }

    fn run(&mut self, action: &Action) -> Result<String, String> {
        self.done.lock().unwrap().push(action.clone());
        self.results
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Ok(String::new()))
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
    pub actions: FakeActions,
}

impl Harness {
    pub fn new() -> Self {
        let chat = FakeChat::default();
        let speaker = FakeSpeaker::default();
        let store = FakeStore::default();
        let clock = FakeClock::starting_at(start_time());
        let actions = FakeActions::default();
        let assistant = Assistant::new(Ports {
            chat: Box::new(chat.clone()),
            speaker: Box::new(speaker.clone()),
            store: Box::new(store.clone()),
            clock: Box::new(clock.clone()),
            actions: Box::new(actions.clone()),
        });
        Self {
            assistant,
            chat,
            speaker,
            store,
            clock,
            actions,
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
