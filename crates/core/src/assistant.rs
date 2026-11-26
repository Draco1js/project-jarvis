//! The assistant engine.

use std::time::{Duration, SystemTime};

use crate::memory;
use crate::ports::{ChatError, Message, Ports, Reply, Role};
use crate::tools;

/// Idle time after which the next question starts a new conversation.
pub const CONVERSATION_IDLE_RESET: Duration = Duration::from_secs(5 * 60);

/// How many times one question may go back to the model with tool results
/// before Jarvis gives up on it.
const MAX_TOOL_ROUNDS: usize = 5;

/// Said after a note is saved.
const NOTED: &str = "Noted.";

/// Sent first in every request. Jarvis must not say its own name, or it
/// could wake itself through the speakers (the only echo handling planned).
pub const SYSTEM_PROMPT: &str = "You are a helpful desktop voice assistant. \
Your replies are read aloud, so keep them short, plain and conversational, without markdown. \
Always reply in English, whatever language the user speaks. \
Never say \"Jarvis\" in a reply, not even when the user calls you that.";

/// Something that happened that the assistant should react to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// The user typed a question and pressed Enter.
    Typed(String),
    /// Time passed. The app sends this at start and about once a second, so
    /// timers, alarms and reminders go off (late ones too, after a restart)
    /// and an idle conversation ends (and is summarised) on time.
    Tick,
    /// Delete saved conversation history (notes are kept) and start afresh.
    ClearHistory,
}

/// An update for the window or panel showing the conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    /// The previous conversation ended (idle too long); clear it from view.
    ConversationReset,
    /// What the user said or typed, to show in the conversation.
    UserSaid(String),
    /// Waiting for the model.
    Thinking,
    /// The model's answer (also sent to the speaker).
    Reply(String),
    /// No answer this time; a short explanation for the user.
    Problem(String),
    /// A timer, alarm or reminder went off (also notified and spoken).
    /// `late` means it was missed, e.g. while Jarvis was not running.
    ReminderDue {
        title: String,
        body: String,
        late: bool,
    },
}

pub struct Assistant {
    ports: Ports,
    history: Vec<Message>,
    last_activity: Option<SystemTime>,
}

impl Assistant {
    pub fn new(ports: Ports) -> Self {
        Self {
            ports,
            history: Vec::new(),
            last_activity: None,
        }
    }

    /// React to one input, sending each update to `emit` as it happens.
    pub fn handle(&mut self, input: Input, emit: &mut dyn FnMut(Output)) {
        match input {
            Input::Typed(text) => self.answer(text, emit),
            Input::Tick => {
                self.reset_if_idle(emit);
                tools::fire_due_reminders(&mut self.ports, emit);
            }
            Input::ClearHistory => {
                self.ports.store.clear_history();
                self.history.clear();
                self.last_activity = None;
                emit(Output::ConversationReset);
            }
        }
    }

    fn answer(&mut self, text: String, emit: &mut dyn FnMut(Output)) {
        self.reset_if_idle(emit);
        emit(Output::UserSaid(text.clone()));
        let question = Message::new(Role::User, text);
        emit(Output::Thinking);

        let mut turn = vec![question];
        let outcome = match memory::note_text(&turn[0].content) {
            Some(note) => {
                let note = note.to_string();
                self.save_note(&note)
            }
            None => self.ask(&mut turn),
        };

        match outcome {
            Ok(reply) => {
                let reply = Message::new(Role::Assistant, reply);
                for message in turn {
                    self.remember(message);
                }
                self.remember(reply.clone());
                self.ports.speaker.speak(&reply.content);
                emit(Output::Reply(reply.content));
            }
            Err(problem) => emit(Output::Problem(problem)),
        }
        self.last_activity = Some(self.ports.clock.now());
    }

    /// Ask the model, running the tools it calls, until it answers in text.
    /// The tool calls and results are added to `turn`.
    fn ask(&mut self, turn: &mut Vec<Message>) -> Result<String, String> {
        let specs = tools::specs();
        loop {
            if turn.len() > 2 * MAX_TOOL_ROUNDS {
                return Err("Gave up after too many tool calls.".to_string());
            }
            let mut request = Vec::with_capacity(self.history.len() + turn.len() + 1);
            request.push(Message::new(Role::System, self.system_prompt()));
            request.extend(self.history.iter().cloned());
            request.extend(turn.iter().cloned());

            match self.ports.chat.chat(&request, &specs) {
                Ok(Reply::Text(reply)) => return Ok(reply),
                Ok(Reply::ToolCalls(calls)) => {
                    let results: Vec<Message> = calls
                        .iter()
                        .map(|call| {
                            let result = tools::run(&mut self.ports, call);
                            Message::tool_result(call.id.clone(), result)
                        })
                        .collect();
                    turn.push(Message::tool_calls(calls));
                    turn.extend(results);
                }
                Err(error) => return Err(describe(&error)),
            }
        }
    }

    /// Save a note with model-written tags; the confirmation is the reply.
    fn save_note(&mut self, note: &str) -> Result<String, String> {
        let tagged = memory::summarize(self.ports.chat.as_mut(), note);
        let now = self.ports.clock.now();
        self.ports
            .store
            .save_note(now, note, &tagged)
            .map(|_| NOTED.to_string())
            .map_err(|reason| format!("Couldn't save the note: {reason}"))
    }

    /// The fixed prompt plus the current local time, which the model needs
    /// for dates, alarms and reminders.
    fn system_prompt(&self) -> String {
        format!(
            "{SYSTEM_PROMPT} {}",
            tools::prompt_context(self.ports.clock.as_ref())
        )
    }

    /// Keep a message in the conversation. Only what was said is saved to
    /// history; tool calls and results stay in memory.
    fn remember(&mut self, message: Message) {
        if matches!(message.role, Role::User | Role::Assistant) && message.tool_calls.is_empty() {
            let now = self.ports.clock.now();
            self.ports.store.save_message(now, &message);
        }
        self.history.push(message);
    }

    fn reset_if_idle(&mut self, emit: &mut dyn FnMut(Output)) {
        let Some(last) = self.last_activity else {
            return;
        };
        let idle = self
            .ports
            .clock
            .now()
            .duration_since(last)
            .unwrap_or_default();
        if idle >= CONVERSATION_IDLE_RESET {
            self.end_conversation();
            emit(Output::ConversationReset);
        }
    }

    /// Close the current conversation: one model call writes its summary
    /// and tags, then the store makes it searchable.
    fn end_conversation(&mut self) {
        let history = std::mem::take(&mut self.history);
        self.last_activity = None;
        if history.is_empty() {
            return;
        }
        let tagged = memory::summarize(self.ports.chat.as_mut(), &memory::transcript(&history));
        let now = self.ports.clock.now();
        self.ports.store.end_conversation(now, &tagged);
    }
}

/// A short message for the user explaining a failed request.
fn describe(error: &ChatError) -> String {
    match error {
        ChatError::MissingKey => {
            "No Gemini key is set. Set JARVIS_GEMINI_API_KEY and restart.".into()
        }
        ChatError::InvalidKey => {
            "Gemini: the key was rejected. Check JARVIS_GEMINI_API_KEY.".into()
        }
        ChatError::RateLimited {
            retry_after: Some(wait),
        } => format!(
            "Gemini hit its rate limit. Try again in {} s.",
            wait.as_secs().max(1)
        ),
        ChatError::RateLimited { retry_after: None } => {
            "Gemini hit its rate limit. Try again shortly.".into()
        }
        ChatError::Server { status } => format!("Gemini had a server error ({status})."),
        ChatError::Timeout => "Gemini took too long to answer.".into(),
        ChatError::Unreachable => "Couldn't reach Gemini. Check the internet connection.".into(),
        ChatError::Other(detail) => format!("Gemini request failed: {detail}"),
    }
}
