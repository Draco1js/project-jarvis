//! Seam 1: timers, alarms and reminders, driven through the assistant with
//! fake ports and a fake clock.

mod fakes;

use std::time::Duration;

use fakes::{Harness, start_time};
use jarvis_core::{Output, ReminderKind};
use serde_json::json;

#[test]
fn timer_set_by_voice_is_saved_and_confirmed() {
    let mut h = Harness::new();
    h.chat
        .will_call("set_timer", json!({"seconds": 300, "label": "tea"}))
        .will_reply("Tea timer set for five minutes.");

    let outputs = h.type_text("Set a tea timer for five minutes");

    let saved = h.reminders.all();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].kind, ReminderKind::Timer);
    assert_eq!(saved[0].text, "tea");
    assert_eq!(saved[0].due, start_time() + Duration::from_secs(300));
    assert!(h.chat.last_tools().contains(&"set_timer".to_string()));
    assert_eq!(
        outputs.last(),
        Some(&Output::Reply("Tea timer set for five minutes.".into()))
    );
    assert_eq!(h.speaker.spoken(), vec!["Tea timer set for five minutes."]);
}

/// The summary written when a conversation goes idle (memory, #11).
const IDLE_SUMMARY: &str = "Summary: Timers.\nTags: timer, tea";

fn set_tea_timer(h: &mut Harness) {
    h.chat
        .will_call("set_timer", json!({"seconds": 300, "label": "tea"}))
        .will_reply("Done.");
    h.type_text("Tea timer, five minutes");
}

#[test]
fn timer_goes_off_once_when_due_as_a_notification_and_aloud() {
    let mut h = Harness::new();
    set_tea_timer(&mut h);

    h.clock.advance(Duration::from_secs(299));
    assert_eq!(h.tick(), vec![], "not due yet");
    assert!(h.notifier.shown().is_empty());

    h.clock.advance(Duration::from_secs(1));
    h.chat.will_reply(IDLE_SUMMARY);
    let outputs = h.tick();

    assert_eq!(
        outputs,
        vec![
            Output::ConversationReset,
            Output::ReminderDue {
                title: "Timer done".into(),
                body: "tea".into(),
                late: false,
            }
        ],
        "the conversation also went idle"
    );
    assert_eq!(
        h.notifier.shown(),
        vec![("Timer done".to_string(), "tea".to_string())]
    );
    assert_eq!(h.speaker.spoken().last().unwrap(), "Timer done: tea.");
    assert!(h.reminders.all().is_empty(), "gone once it went off");

    h.clock.advance(Duration::from_secs(5));
    assert_eq!(h.tick(), vec![], "goes off only once");
}

#[test]
fn timer_missed_while_jarvis_was_off_goes_off_at_next_start_marked_late() {
    let mut h = Harness::new();
    h.clock.set_utc_offset(5 * 3600 + 1800); // 12:00 UTC is 17:30 local
    set_tea_timer(&mut h); // due 17:35 local

    // Quit, and start again an hour later.
    h.clock.advance(Duration::from_secs(3600));
    h.restart();
    let outputs = h.tick();

    let late_body = "tea (late: was due at 17:35)";
    assert_eq!(
        outputs,
        vec![Output::ReminderDue {
            title: "Timer done".into(),
            body: late_body.into(),
            late: true,
        }]
    );
    assert_eq!(
        h.notifier.shown(),
        vec![("Timer done".to_string(), late_body.to_string())]
    );
    assert!(h.reminders.all().is_empty());
}

#[test]
fn model_is_told_the_local_time_so_it_can_place_alarms() {
    let mut h = Harness::new();
    h.clock.set_utc_offset(5 * 3600 + 1800);
    h.chat.will_reply("Hi.");

    h.type_text("Hi");

    let prompt = &h.chat.last_request()[0].content;
    assert!(
        prompt.contains("Thursday 26 November 2026, 17:30"),
        "{prompt}"
    );
    assert!(prompt.contains("2026-11-26T17:30:00+05:30"), "{prompt}");
}

#[test]
fn alarm_at_a_local_time_goes_off_then() {
    let mut h = Harness::new();
    h.clock.set_utc_offset(5 * 3600 + 1800); // now 17:30 local
    h.chat
        .will_call("set_alarm", json!({"at": "2026-11-27T07:00:00+05:30"}))
        .will_reply("Alarm set for 7 tomorrow.");

    h.type_text("Wake me at 7");

    assert_eq!(
        h.chat.last_tool_results(),
        vec!["Alarm 1 set, going off at 07:00 on Fri 27 Nov."]
    );
    h.clock.advance(Duration::from_secs(13 * 3600 + 1800 - 1));
    h.chat.will_reply(IDLE_SUMMARY);
    assert_eq!(
        h.tick(),
        vec![Output::ConversationReset],
        "idle conversation ends"
    );
    h.clock.advance(Duration::from_secs(1));
    assert_eq!(
        h.tick(),
        vec![Output::ReminderDue {
            title: "Alarm".into(),
            body: "".into(),
            late: false,
        }]
    );
    assert_eq!(h.speaker.spoken().last().unwrap(), "Alarm.");
}

#[test]
fn time_without_an_offset_is_taken_as_local() {
    let mut h = Harness::new();
    h.clock.set_utc_offset(3600); // now 13:00 local
    h.chat
        .will_call(
            "set_reminder",
            json!({"at": "2026-11-26T14:00", "text": "stretch"}),
        )
        .will_reply("OK.");

    h.type_text("Remind me to stretch at 2");

    assert_eq!(
        h.reminders.all()[0].due,
        start_time() + Duration::from_secs(3600)
    );
}

#[test]
fn a_time_already_passed_is_refused_back_to_the_model() {
    let mut h = Harness::new();
    h.chat
        .will_call(
            "set_reminder",
            json!({"at": "2026-11-26T11:00:00+00:00", "text": "lunch"}),
        )
        .will_reply("That time has passed.");

    let outputs = h.type_text("Remind me about lunch at 11");

    assert!(h.reminders.all().is_empty());
    let results = h.chat.last_tool_results();
    assert!(results[0].starts_with("Error:"), "{results:?}");
    assert!(results[0].contains("already passed"), "{results:?}");
    assert_eq!(
        outputs.last(),
        Some(&Output::Reply("That time has passed.".into()))
    );
}

#[test]
fn reminders_can_be_listed_and_cancelled() {
    let mut h = Harness::new();
    set_tea_timer(&mut h);
    h.chat
        .will_call("list_reminders", json!({}))
        .will_call("cancel_reminder", json!({"id": 1}))
        .will_reply("Cancelled the tea timer.");

    h.type_text("Cancel my tea timer");

    let results = h.chat.last_tool_results();
    assert_eq!(
        results[results.len() - 2..],
        vec!["1: Timer \"tea\" at 12:05", "Cancelled 1."],
        "both results go back, in order"
    );
    assert!(h.reminders.all().is_empty());
    h.clock.advance(Duration::from_secs(600));
    h.chat.will_reply(IDLE_SUMMARY);
    assert_eq!(
        h.tick(),
        vec![Output::ConversationReset],
        "a cancelled timer never goes off"
    );
}

#[test]
fn tool_results_answer_their_calls_and_only_speech_is_saved() {
    let mut h = Harness::new();
    set_tea_timer(&mut h);

    let request = h.chat.last_request();
    let call = request
        .iter()
        .find(|m| !m.tool_calls.is_empty())
        .expect("the tool call is sent back");
    let result = request
        .iter()
        .find(|m| m.role == jarvis_core::Role::Tool)
        .unwrap();
    assert_eq!(
        result.tool_call_id.as_deref(),
        Some(call.tool_calls[0].id.as_str())
    );

    let saved: Vec<String> = h
        .store
        .saved()
        .into_iter()
        .map(|(_, _, text)| text)
        .collect();
    assert_eq!(saved, vec!["Tea timer, five minutes", "Done."]);
}

#[test]
fn several_due_at_once_all_go_off_soonest_first() {
    let mut h = Harness::new();
    h.chat
        .will_call("set_timer", json!({"seconds": 120, "label": "eggs"}))
        .will_call("set_timer", json!({"seconds": 60, "label": "toast"}))
        .will_reply("Both set.");
    h.type_text("Timers for eggs and toast");

    h.clock.advance(Duration::from_secs(120));
    let bodies: Vec<String> = h
        .tick()
        .into_iter()
        .map(|o| match o {
            Output::ReminderDue { body, .. } => body,
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(bodies, vec!["toast (late: was due at 12:01)", "eggs"]);
}

#[test]
fn weather_for_a_named_place_or_for_here() {
    let mut h = Harness::new();
    h.weather.will_report(Ok("Paris: light rain, 9°C."));
    h.chat
        .will_call("get_weather", json!({"place": "Paris"}))
        .will_reply("Light rain in Paris.")
        .will_call("get_weather", json!({}))
        .will_reply("Sunny here.");

    h.type_text("Weather in Paris?");
    assert_eq!(h.chat.last_tool_results(), vec!["Paris: light rain, 9°C."]);
    h.type_text("And here?");

    assert_eq!(h.weather.asked(), vec![Some("Paris".to_string()), None]);
}

#[test]
fn weather_failure_goes_back_to_the_model() {
    let mut h = Harness::new();
    h.weather.will_report(Err("wttr.in did not answer"));
    h.chat
        .will_call("get_weather", json!({}))
        .will_reply("I couldn't get the weather.");

    h.type_text("Weather?");

    assert_eq!(
        h.chat.last_tool_results(),
        vec!["Error: no weather report: wttr.in did not answer."]
    );
}

#[test]
fn reminder_missed_on_an_earlier_day_says_which_day() {
    let mut h = Harness::new();
    h.chat
        .will_call(
            "set_reminder",
            json!({"at": "2026-11-26T12:30:00+00:00", "text": "call mum"}),
        )
        .will_reply("I'll remind you.");
    h.type_text("Remind me to call mum at half twelve");

    h.clock.advance(Duration::from_secs(2 * 24 * 3600));
    h.restart();
    let outputs = h.tick();

    assert_eq!(
        outputs,
        vec![Output::ReminderDue {
            title: "Reminder".into(),
            body: "call mum (late: was due at 12:30 on Thu 26 Nov)".into(),
            late: true,
        }]
    );
}

#[test]
fn an_absurdly_long_timer_is_refused() {
    let mut h = Harness::new();
    h.chat
        .will_call("set_timer", json!({"seconds": u64::MAX}))
        .will_reply("That is too long.");

    h.type_text("Set a timer for ever");

    assert!(h.reminders.all().is_empty());
}
