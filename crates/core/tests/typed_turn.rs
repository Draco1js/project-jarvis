//! Seam 1: the assistant core driven by fake ports. Tests feed inputs and
//! check outputs and what the fakes recorded, never internal state.

mod fakes;

use std::time::Duration;

use fakes::Harness;
use jarvis_core::{ChatError, Output, Role};

#[test]
fn typed_question_is_answered_shown_and_spoken() {
    let mut h = Harness::new();
    h.chat.will_reply("It is sunny in Pune today.");

    let outputs = h.type_text("What's the weather in Pune?");

    assert_eq!(
        outputs,
        vec![
            Output::UserSaid("What's the weather in Pune?".into()),
            Output::Thinking,
            Output::Reply("It is sunny in Pune today.".into()),
        ]
    );
    assert_eq!(h.speaker.spoken(), vec!["It is sunny in Pune today."]);

    let request = h.chat.last_request();
    let last = request.last().unwrap();
    assert_eq!(last.role, Role::User);
    assert_eq!(last.content, "What's the weather in Pune?");
}

#[test]
fn system_prompt_forbids_saying_jarvis_and_asks_for_english() {
    let mut h = Harness::new();
    h.chat.will_reply("Hello.");

    h.type_text("Namaste, kaise ho?");

    let request = h.chat.last_request();
    let system = &request[0];
    assert_eq!(system.role, Role::System);
    let prompt = system.content.to_lowercase();
    assert!(prompt.contains("never say \"jarvis\""), "prompt: {prompt}");
    assert!(
        prompt.contains("always reply in english"),
        "prompt: {prompt}"
    );
    assert_eq!(request.iter().filter(|m| m.role == Role::System).count(), 1);
}

fn turns(request: &[jarvis_core::Message]) -> Vec<(Role, &str)> {
    request
        .iter()
        .filter(|m| m.role != Role::System)
        .map(|m| (m.role, m.content.as_str()))
        .collect()
}

#[test]
fn follow_up_question_includes_the_conversation_so_far() {
    let mut h = Harness::new();
    h.chat
        .will_reply("It is sunny in Pune.")
        .will_reply("Rain is expected.");

    h.type_text("Weather in Pune?");
    h.clock.advance(Duration::from_secs(60));
    h.type_text("And tomorrow?");

    assert_eq!(
        turns(&h.chat.last_request()),
        vec![
            (Role::User, "Weather in Pune?"),
            (Role::Assistant, "It is sunny in Pune."),
            (Role::User, "And tomorrow?"),
        ]
    );
}

#[test]
fn conversation_starts_fresh_after_five_idle_minutes() {
    let mut h = Harness::new();
    h.chat
        .will_reply("It is sunny.")
        .will_reply("Still here.")
        .will_reply("Hello.");

    h.type_text("Weather?");
    h.clock.advance(Duration::from_secs(4 * 60 + 59));
    h.type_text("Still there?");
    assert_eq!(turns(&h.chat.last_request()).len(), 3, "kept under 5 min");

    h.clock.advance(Duration::from_secs(5 * 60));
    let outputs = h.type_text("Hi");

    assert_eq!(turns(&h.chat.last_request()), vec![(Role::User, "Hi")]);
    assert_eq!(outputs[0], Output::ConversationReset);
}

#[test]
fn question_and_reply_are_saved_with_the_time() {
    let mut h = Harness::new();
    h.chat.will_reply("Four.");

    h.type_text("Two plus two?");

    let at = fakes::start_time();
    assert_eq!(
        h.store.saved(),
        vec![
            (at, Role::User, "Two plus two?".to_string()),
            (at, Role::Assistant, "Four.".to_string()),
        ]
    );
}

#[test]
fn chat_failure_is_shown_but_not_spoken_or_kept() {
    let cases = [
        (ChatError::MissingKey, "JARVIS_GEMINI_API_KEY"),
        (ChatError::InvalidKey, "key was rejected"),
        (ChatError::RateLimited { retry_after: None }, "rate limit"),
        (ChatError::Server { status: 503 }, "503"),
        (ChatError::Timeout, "too long"),
        (ChatError::Unreachable, "reach"),
    ];
    for (error, expected) in cases {
        let mut h = Harness::new();
        h.chat.will_fail(error.clone()).will_reply("Hello again.");

        let outputs = h.type_text("Hello?");

        let Some(Output::Problem(message)) = outputs.last() else {
            panic!("{error:?}: expected a problem, got {outputs:?}");
        };
        assert!(message.contains(expected), "{error:?}: {message}");
        assert!(h.speaker.spoken().is_empty(), "{error:?}");

        // The failed question is not kept, so a retry is not doubled up.
        h.type_text("Hello?");
        assert_eq!(turns(&h.chat.last_request()), vec![(Role::User, "Hello?")]);
    }
}
