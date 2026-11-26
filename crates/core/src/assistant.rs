//! The assistant engine.

use std::time::{Duration, SystemTime};

use crate::actions::{Outcome, Toolbox};
use crate::ports::{ChatError, Message, Ports, Reply, Role};

/// Most model calls in one turn, so a model stuck calling tools can't loop forever.
const MAX_TOOL_ROUNDS: usize = 6;

/// Idle time after which the next question starts a new conversation.
pub const CONVERSATION_IDLE_RESET: Duration = Duration::from_secs(5 * 60);

/// Sent first in every request. Jarvis must not say its own name, or it
/// could wake itself through the speakers (the only echo handling planned).
pub const SYSTEM_PROMPT: &str = "You are a helpful desktop voice assistant. \
Your replies are read aloud, so keep them short, plain and conversational, without markdown. \
Always reply in English, whatever language the user speaks. \
You can act on the computer only through the tools you are given; after acting, confirm it in a few words. \
Never say \"Jarvis\" in a reply, not even when the user calls you that.";

/// Something that happened that the assistant should react to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// The user typed a question and pressed Enter.
    Typed(String),
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
}

pub struct Assistant {
    ports: Ports,
    history: Vec<Message>,
    toolbox: Toolbox,
    last_activity: Option<SystemTime>,
}

impl Assistant {
    pub fn new(ports: Ports) -> Self {
        Self {
            ports,
            history: Vec::new(),
            toolbox: Toolbox::default(),
            last_activity: None,
        }
    }

    /// React to one input, sending each update to `emit` as it happens.
    pub fn handle(&mut self, input: Input, emit: &mut dyn FnMut(Output)) {
        match input {
            Input::Typed(text) => self.answer(text, emit),
        }
    }

    fn answer(&mut self, text: String, emit: &mut dyn FnMut(Output)) {
        self.reset_if_idle(emit);
        emit(Output::UserSaid(text.clone()));
        let question = Message::new(Role::User, text);
        emit(Output::Thinking);

        match self.run_turn(question) {
            Ok((turn, reply)) => {
                for message in turn {
                    self.remember(message);
                }
                self.ports.speaker.speak(&reply);
                emit(Output::Reply(reply));
            }
            Err(error) => emit(Output::Problem(describe(&error))),
        }
        self.last_activity = Some(self.ports.clock.now());
    }

    /// Talks to the model, running the tools it asks for, until it answers.
    /// Returns every message of the turn and the text to show and speak.
    fn run_turn(&mut self, question: Message) -> Result<(Vec<Message>, String), ChatError> {
        let tools = self.toolbox.specs(self.ports.actions.as_mut());
        let mut turn = vec![question];
        let mut notices = Vec::new();
        for _ in 0..MAX_TOOL_ROUNDS {
            let mut request = Vec::with_capacity(self.history.len() + turn.len() + 1);
            request.push(Message::new(Role::System, SYSTEM_PROMPT));
            request.extend(self.history.iter().cloned());
            request.extend(turn.iter().cloned());

            let calls = match self.ports.chat.chat(&request, &tools)? {
                Reply::Text(text) => {
                    let text = with_notices(text, &notices);
                    turn.push(Message::new(Role::Assistant, text.clone()));
                    return Ok((turn, text));
                }
                Reply::ToolCalls(calls) => calls,
            };
            turn.push(Message::calls(calls.clone()));
            let mut question = None;
            for call in &calls {
                let result = match self
                    .toolbox
                    .run(call, self.ports.actions.as_mut(), &mut notices)
                {
                    Outcome::Done(result) => result,
                    Outcome::Ask(ask) => {
                        let result = format!("Not done yet: asked the user \"{ask}\"");
                        question.get_or_insert(ask);
                        result
                    }
                };
                turn.push(Message::tool_result(&call.id, result));
            }
            if let Some(ask) = question {
                let text = with_notices(ask, &notices);
                turn.push(Message::new(Role::Assistant, text.clone()));
                return Ok((turn, text));
            }
        }
        Err(ChatError::Other("the model kept calling tools".into()))
    }

    fn remember(&mut self, message: Message) {
        let now = self.ports.clock.now();
        self.ports.store.save_message(now, &message);
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
            self.history.clear();
            self.last_activity = None;
            emit(Output::ConversationReset);
        }
    }
}

/// The reply followed by anything the user must be told once.
fn with_notices(reply: String, notices: &[String]) -> String {
    let mut text = reply;
    for notice in notices {
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(notice);
    }
    text
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
