//! Seam 1: memory (conversation end, summary and tags, search, notes, clearing)
//! driven through the core with fake ports.

mod fakes;

use std::time::Duration;

use fakes::Harness;
use jarvis_core::{Input, MemoryHit, MemoryKind, Output, Role, Tagged};

const FIVE_MINUTES: Duration = Duration::from_secs(5 * 60);

fn tags(list: &[&str]) -> Vec<String> {
    list.iter().map(|t| t.to_string()).collect()
}

#[test]
fn idle_conversation_ends_with_one_summary_call() {
    let mut h = Harness::new();
    h.chat.will_reply("Rain is expected in Pune tomorrow.");
    h.type_text("Will it rain in Pune tomorrow?");

    h.clock.advance(FIVE_MINUTES);
    h.chat.will_reply(
        "Summary: The user asked about rain in Pune; rain is expected tomorrow.\n\
         Tags: weather, rain, Pune, forecast, tomorrow",
    );
    let outputs = h.tick();

    assert_eq!(outputs, vec![Output::ConversationReset]);
    assert_eq!(
        h.chat.requests().len(),
        2,
        "one call for the answer, one for the summary"
    );
    let summary_request = h.chat.last_request();
    let transcript = &summary_request.last().unwrap().content;
    assert!(
        transcript.contains("Will it rain in Pune tomorrow?"),
        "{transcript}"
    );
    assert!(
        transcript.contains("Rain is expected in Pune tomorrow."),
        "{transcript}"
    );

    assert_eq!(
        h.store.ended(),
        vec![(
            fakes::start_time() + FIVE_MINUTES,
            Tagged {
                summary: "The user asked about rain in Pune; rain is expected tomorrow.".into(),
                tags: tags(&["weather", "rain", "Pune", "forecast", "tomorrow"]),
            }
        )]
    );
    assert_eq!(h.speaker.spoken().len(), 1, "the summary is not spoken");
}

#[test]
fn ticks_before_five_idle_minutes_keep_the_conversation() {
    let mut h = Harness::new();
    h.chat.will_reply("Hello.");
    h.type_text("Hi");

    h.clock.advance(FIVE_MINUTES - Duration::from_secs(1));
    assert_eq!(h.tick(), vec![]);

    assert!(h.store.ended().is_empty());
    assert_eq!(h.chat.requests().len(), 1);
}

#[test]
fn ticks_without_a_conversation_do_nothing() {
    let mut h = Harness::new();
    h.clock.advance(FIVE_MINUTES * 3);

    assert_eq!(h.tick(), vec![]);
    assert!(h.chat.requests().is_empty());
    assert!(h.store.ended().is_empty());
}

#[test]
fn conversation_ends_only_once() {
    let mut h = Harness::new();
    h.chat
        .will_reply("Hello.")
        .will_reply("Summary: A greeting.\nTags: greeting, hello");
    h.type_text("Hi");
    h.clock.advance(FIVE_MINUTES);
    h.tick();

    h.clock.advance(FIVE_MINUTES);
    assert_eq!(h.tick(), vec![]);
    assert_eq!(h.store.ended().len(), 1);
}

#[test]
fn next_question_after_idle_ends_the_old_conversation_first() {
    let mut h = Harness::new();
    h.chat
        .will_reply("Hello.")
        .will_reply("Summary: A greeting.\nTags: greeting, hello")
        .will_reply("Ten past twelve.");
    h.type_text("Hi");

    h.clock.advance(FIVE_MINUTES);
    let outputs = h.type_text("What time is it?");

    assert_eq!(outputs[0], Output::ConversationReset);
    assert_eq!(h.store.ended().len(), 1);
    assert_eq!(h.store.ended()[0].1.summary, "A greeting.");
}

#[test]
fn failed_summary_still_ends_the_conversation_untagged() {
    let mut h = Harness::new();
    h.chat
        .will_reply("Hello.")
        .will_fail(jarvis_core::ChatError::Timeout);
    h.type_text("Hi");

    h.clock.advance(FIVE_MINUTES);
    assert_eq!(h.tick(), vec![Output::ConversationReset]);

    assert_eq!(h.store.ended().len(), 1);
    assert_eq!(h.store.ended()[0].1, Tagged::default());
}

fn pune_hit() -> MemoryHit {
    MemoryHit {
        kind: MemoryKind::Conversation,
        at: fakes::start_time() - Duration::from_secs(6 * 24 * 3600),
        summary: "The user asked about rain in Pune.".into(),
        tags: tags(&["weather", "Pune"]),
        excerpt: "Rain is expected in Pune tomorrow.".into(),
    }
}

#[test]
fn every_question_offers_the_search_memory_tool() {
    let mut h = Harness::new();
    h.chat.will_reply("Hello.");
    h.type_text("Hi");

    assert_eq!(h.chat.offered_tools(), vec![vec!["search_memory"]]);
}

#[test]
fn search_memory_results_go_back_to_the_model_before_it_answers() {
    let mut h = Harness::new();
    h.store.will_find(vec![pune_hit()]);
    h.chat
        .will_call("search_memory", r#"{"query":"Pune rain"}"#)
        .will_reply("Last week I said rain was expected in Pune.");

    let outputs = h.type_text("What did you tell me about Pune last week?");

    assert_eq!(h.store.searches(), vec!["Pune rain"]);
    assert_eq!(
        outputs.last(),
        Some(&Output::Reply(
            "Last week I said rain was expected in Pune.".into()
        ))
    );
    assert_eq!(
        h.speaker.spoken(),
        vec!["Last week I said rain was expected in Pune."]
    );

    let follow_up = h.chat.last_request();
    let n = follow_up.len();
    let (asked, result) = (&follow_up[n - 2], &follow_up[n - 1]);
    assert_eq!(asked.role, Role::Assistant);
    assert_eq!(asked.tool_calls[0].name, "search_memory");
    assert_eq!(
        asked.tool_calls[0].extra.as_deref(),
        Some(r#"{"google":{"thought_signature":"sig"}}"#),
        "provider data goes back unchanged"
    );
    assert_eq!(result.role, Role::Tool);
    assert_eq!(
        result.tool_call_id.as_deref(),
        Some(asked.tool_calls[0].id.as_str())
    );
    for expected in [
        "2026-11-20",
        "rain in Pune",
        "Rain is expected",
        "weather, Pune",
    ] {
        assert!(
            result.content.contains(expected),
            "{expected}: {}",
            result.content
        );
    }
}

#[test]
fn search_memory_says_when_nothing_matches() {
    let mut h = Harness::new();
    h.chat
        .will_call("search_memory", r#"{"query":"giraffes"}"#)
        .will_reply("I don't remember talking about giraffes.");

    h.type_text("Did we talk about giraffes?");

    let result = h.chat.last_request().last().unwrap().content.clone();
    assert!(result.to_lowercase().contains("nothing found"), "{result}");
}

#[test]
fn tool_calls_stay_out_of_saved_history_and_later_context() {
    let mut h = Harness::new();
    h.chat
        .will_call("search_memory", r#"{"query":"Pune"}"#)
        .will_reply("Rain, last week.")
        .will_reply("You're welcome.");

    h.type_text("What about Pune?");
    h.type_text("Thanks");

    let roles: Vec<Role> = h.store.saved().iter().map(|s| s.1).collect();
    assert_eq!(
        roles,
        vec![Role::User, Role::Assistant, Role::User, Role::Assistant]
    );
    let later = h.chat.last_request();
    assert!(
        later
            .iter()
            .all(|m| m.role != Role::Tool && m.tool_calls.is_empty())
    );
}

#[test]
fn bad_tool_arguments_are_reported_to_the_model() {
    let mut h = Harness::new();
    h.chat
        .will_call("search_memory", "not json")
        .will_call("launch_rockets", "{}")
        .will_reply("Sorry, I couldn't search.");

    let outputs = h.type_text("Search my memory");

    assert!(h.store.searches().is_empty());
    let requests = h.chat.requests();
    assert!(requests[1].last().unwrap().content.contains("query"));
    assert!(
        requests[2]
            .last()
            .unwrap()
            .content
            .contains("launch_rockets")
    );
    assert_eq!(
        outputs.last(),
        Some(&Output::Reply("Sorry, I couldn't search.".into()))
    );
}

#[test]
fn endless_tool_calls_stop_with_a_problem() {
    let mut h = Harness::new();
    for _ in 0..10 {
        h.chat.will_call("search_memory", r#"{"query":"x"}"#);
    }

    let outputs = h.type_text("Loop forever");

    assert!(
        matches!(outputs.last(), Some(Output::Problem(_))),
        "{outputs:?}"
    );
    assert!(h.chat.requests().len() <= 5);
    assert!(h.speaker.spoken().is_empty());
}

#[test]
fn note_that_saves_a_tagged_note_and_confirms() {
    let mut h = Harness::new();
    h.chat.will_reply(
        "Summary: Where the Wi-Fi password is kept.\n\
         Tags: wifi, password, internet, fridge, home",
    );

    let outputs = h.type_text("Note that the wifi password is on the fridge.");

    assert_eq!(
        h.store.notes(),
        vec![(
            fakes::start_time(),
            "the wifi password is on the fridge.".to_string(),
            Tagged {
                summary: "Where the Wi-Fi password is kept.".into(),
                tags: tags(&["wifi", "password", "internet", "fridge", "home"]),
            }
        )]
    );
    assert_eq!(h.chat.requests().len(), 1, "one call, for the tags");
    assert_eq!(h.chat.offered_tools(), vec![Vec::<&str>::new()]);
    assert_eq!(
        h.chat.last_request().last().unwrap().content,
        "the wifi password is on the fridge."
    );
    assert_eq!(outputs.last(), Some(&Output::Reply("Noted.".into())));
    assert_eq!(h.speaker.spoken(), vec!["Noted."]);
}

#[test]
fn notes_can_be_asked_for_in_several_ways() {
    for said in [
        "note that milk is low",
        "Jarvis, note that milk is low",
        "Make a note that milk is low",
        "take a note: milk is low",
        "Note: milk is low",
    ] {
        let mut h = Harness::new();
        h.chat.will_reply("Summary: Milk.\nTags: milk, shopping");

        h.type_text(said);

        let notes = h.store.notes();
        assert_eq!(notes.len(), 1, "{said}");
        assert_eq!(notes[0].1, "milk is low", "{said}");
    }
}

#[test]
fn questions_about_notes_are_not_notes() {
    let mut h = Harness::new();
    h.chat.will_reply("Notes live in Documents.");

    h.type_text("Notes are where?");

    assert!(h.store.notes().is_empty());
    assert_eq!(h.chat.offered_tools(), vec![vec!["search_memory"]]);
}

#[test]
fn note_is_saved_untagged_when_tagging_fails() {
    let mut h = Harness::new();
    h.chat.will_fail(jarvis_core::ChatError::Unreachable);

    let outputs = h.type_text("Note that the meeting moved to Friday");

    assert_eq!(h.store.notes()[0].2, Tagged::default());
    assert_eq!(outputs.last(), Some(&Output::Reply("Noted.".into())));
}

#[test]
fn note_that_cannot_be_saved_is_reported() {
    let mut h = Harness::new();
    h.chat.will_reply("Summary: Milk.\nTags: milk");
    h.store.notes_fail("the notes folder is read-only");

    let outputs = h.type_text("Note that milk is low");

    let Some(Output::Problem(problem)) = outputs.last() else {
        panic!("{outputs:?}");
    };
    assert!(problem.contains("read-only"), "{problem}");
    assert!(h.speaker.spoken().is_empty());
}

#[test]
fn notes_become_part_of_the_conversation() {
    let mut h = Harness::new();
    h.chat
        .will_reply("Summary: Milk.\nTags: milk")
        .will_reply("You noted that milk is low.");

    h.type_text("Note that milk is low");
    h.type_text("What did I just note?");

    let request = h.chat.last_request();
    let contents: Vec<&str> = request.iter().map(|m| m.content.as_str()).collect();
    assert!(contents.contains(&"Note that milk is low"), "{contents:?}");
    assert!(contents.contains(&"Noted."), "{contents:?}");
}

#[test]
fn clearing_history_forgets_the_conversation_without_summarising_it() {
    let mut h = Harness::new();
    h.chat.will_reply("Hello.").will_reply("Hi again.");
    h.type_text("Hi");

    let outputs = h.send(Input::ClearHistory);

    assert_eq!(outputs, vec![Output::ConversationReset]);
    assert_eq!(h.store.clears(), 1);
    assert!(
        h.store.ended().is_empty(),
        "a cleared conversation is not saved"
    );

    h.type_text("Hello?");
    assert_eq!(h.chat.requests().len(), 2, "no summary call");
    let request = h.chat.last_request();
    assert!(
        request.iter().all(|m| m.content != "Hi"),
        "context is gone too"
    );

    h.clock.advance(FIVE_MINUTES);
    h.chat.will_reply("Summary: Greeting.\nTags: hello");
    h.tick();
    assert_eq!(
        h.store.ended().len(),
        1,
        "later conversations still end normally"
    );
}

#[test]
fn summary_tags_are_deduplicated_and_capped_at_ten() {
    let mut h = Harness::new();
    h.chat.will_reply("Hello.").will_reply(
        "**Summary:** Small talk.\n\
         **Tags:** a, b, B, #c, d, e, f, g, h, i, j, k, l",
    );
    h.type_text("Hi");
    h.clock.advance(FIVE_MINUTES);
    h.tick();

    let tagged = &h.store.ended()[0].1;
    assert_eq!(tagged.summary, "Small talk.");
    assert_eq!(
        tagged.tags,
        tags(&["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"])
    );
}
