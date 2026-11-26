//! The main window: the conversation so far and a box to type questions.
//!
//! The conversation lives in a [`Conversation`] entity owned by the app, not by
//! the window, so closing the window (which only hides Jarvis) loses nothing
//! and reopening it from the tray shows the same conversation.

use std::rc::Rc;

use gpui_kit::{
    component::input::{Input as TextInput, InputEvent, InputState},
    component::label::Label,
    *,
};
use jarvis_core::{Input, Output};

/// One line in the conversation view.
#[derive(Clone)]
pub enum Line {
    You(String),
    Jarvis(String),
    Problem(String),
}

/// What the main window shows. Updated from the assistant's outputs.
#[derive(Default)]
pub struct Conversation {
    pub lines: Vec<Line>,
    pub thinking: bool,
}

impl Conversation {
    pub fn apply(&mut self, output: Output) {
        match output {
            Output::ConversationReset => self.lines.clear(),
            Output::UserSaid(text) => self.lines.push(Line::You(text)),
            Output::Thinking => self.thinking = true,
            Output::Reply(text) => {
                self.thinking = false;
                self.lines.push(Line::Jarvis(text));
            }
            Output::Problem(text) => {
                self.thinking = false;
                self.lines.push(Line::Problem(text));
            }
        }
    }
}

pub struct MainWindow {
    conversation: Entity<Conversation>,
    input: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

impl MainWindow {
    pub fn new(
        conversation: Entity<Conversation>,
        send: Rc<dyn Fn(Input)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Ask Jarvis something…"));
        let subscriptions = vec![
            cx.observe(&conversation, |_, _, cx| cx.notify()),
            cx.subscribe_in(&input, window, move |_, input, event, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let text = input.read(cx).value().trim().to_string();
                    if !text.is_empty() {
                        input.update(cx, |input, cx| input.set_value("", window, cx));
                        send(Input::Typed(text));
                    }
                }
            }),
        ];
        input.update(cx, |input, cx| input.focus(window, cx));
        Self {
            conversation,
            input,
            _subscriptions: subscriptions,
        }
    }
}

impl Render for MainWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let conversation = self.conversation.read(cx);
        let mut lines = div()
            .id("conversation")
            .flex_1()
            .flex()
            .flex_col()
            .gap_2()
            .overflow_y_scroll();
        if conversation.lines.is_empty() && !conversation.thinking {
            lines = lines.child(Label::new("Type a question and press Enter."));
        }
        for line in &conversation.lines {
            lines = lines.child(match line {
                Line::You(text) => Label::new(format!("You: {text}")),
                Line::Jarvis(text) => Label::new(format!("Jarvis: {text}")),
                Line::Problem(text) => Label::new(format!("⚠ {text}")),
            });
        }
        if conversation.thinking {
            lines = lines.child(Label::new("Thinking…"));
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .child(lines)
            .child(TextInput::new(&self.input))
    }
}
