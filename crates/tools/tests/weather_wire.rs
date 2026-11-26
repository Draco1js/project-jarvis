//! Seam 2: the wttr.in client against a local mock HTTP server.

use jarvis_core::Weather;
use jarvis_tools::weather::Wttr;
use mockito::Matcher;
use serde_json::json;

/// A trimmed `?format=j1` answer from wttr.in.
fn j1(area: &str) -> String {
    let day = |date: &str, min: &str, max: &str, desc: &str| {
        json!({
            "date": date,
            "mintempC": min,
            "maxtempC": max,
            "hourly": [
                {"time": "0", "weatherDesc": [{"value": "Clear"}], "chanceofrain": "0"},
                {"time": "1200", "weatherDesc": [{"value": desc}], "chanceofrain": "80"},
            ],
        })
    };
    json!({
        "current_condition": [{
            "temp_C": "8",
            "FeelsLikeC": "5",
            "weatherDesc": [{"value": "Partly cloudy"}],
            "windspeedKmph": "15",
            "humidity": "81",
        }],
        "nearest_area": [{
            "areaName": [{"value": area}],
            "country": [{"value": "United Kingdom"}],
        }],
        "weather": [
            day("2026-11-26", "4", "10", "Light rain"),
            day("2026-11-27", "3", "9", "Sunny"),
            day("2026-11-28", "1", "7", "Overcast"),
        ],
    })
    .to_string()
}

fn mock_j1(server: &mut mockito::Server, path: &str, area: &str) -> mockito::Mock {
    server
        .mock("GET", path)
        .match_query(Matcher::UrlEncoded("format".into(), "j1".into()))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(j1(area))
        .create()
}

#[test]
fn a_named_place_is_looked_up_and_summarised() {
    let mut server = mockito::Server::new();
    let mock = mock_j1(&mut server, "/London", "London");
    let mut weather = Wttr::new(&server.url(), None);

    let report = weather.report(Some("London")).unwrap();

    mock.assert();
    assert_eq!(
        report,
        "Weather for London, United Kingdom. Now 8°C, feels like 5°C, partly cloudy, \
wind 15 km/h, humidity 81%. \
Thu 26 Nov: 4 to 10°C, light rain, 80% chance of rain. \
Fri 27 Nov: 3 to 9°C, sunny, 80% chance of rain. \
Sat 28 Nov: 1 to 7°C, overcast, 80% chance of rain."
    );
}

#[test]
fn a_named_place_wins_over_the_city_setting() {
    let mut server = mockito::Server::new();
    let mock = mock_j1(&mut server, "/Paris", "Paris");
    let mut weather = Wttr::new(&server.url(), Some("Leeds".into()));

    assert!(weather.report(Some("Paris")).unwrap().contains("Paris"));
    mock.assert();
}

#[test]
fn without_a_place_the_city_setting_is_used() {
    let mut server = mockito::Server::new();
    let mock = mock_j1(&mut server, "/Leeds", "Leeds");
    let mut weather = Wttr::new(&server.url(), Some("Leeds".into()));

    assert!(weather.report(None).unwrap().contains("Leeds"));
    mock.assert();
}

#[test]
fn without_a_place_or_city_setting_wttr_uses_the_ip_location() {
    let mut server = mockito::Server::new();
    let mock = mock_j1(&mut server, "/", "Manchester");
    let mut weather = Wttr::new(&server.url(), Some("  ".into()));

    assert!(weather.report(None).unwrap().contains("Manchester"));
    mock.assert();
}

#[test]
fn places_with_spaces_are_encoded() {
    let mut server = mockito::Server::new();
    let mock = mock_j1(&mut server, "/New%20York", "New York");
    let mut weather = Wttr::new(&server.url(), None);

    assert!(
        weather
            .report(Some("New York"))
            .unwrap()
            .contains("New York")
    );
    mock.assert();
}

#[test]
fn an_unknown_place_is_an_error() {
    let mut server = mockito::Server::new();
    server
        .mock("GET", "/Nowhereville")
        .match_query(Matcher::Any)
        .with_status(404)
        .with_body("Unknown location")
        .create();
    let mut weather = Wttr::new(&server.url(), None);

    let error = weather.report(Some("Nowhereville")).unwrap_err();
    assert!(error.contains("404"), "{error}");
}

#[test]
fn an_unreadable_answer_is_an_error() {
    let mut server = mockito::Server::new();
    server
        .mock("GET", "/London")
        .match_query(Matcher::Any)
        .with_status(200)
        .with_body("<html>busy</html>")
        .create();
    let mut weather = Wttr::new(&server.url(), None);

    assert!(weather.report(Some("London")).is_err());
}
