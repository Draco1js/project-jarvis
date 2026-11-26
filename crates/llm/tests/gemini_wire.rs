//! Seam 2: the chat client against a local mock HTTP server, checking the
//! request on the wire and how each response is reported.

use std::time::Duration;

use jarvis_core::{ChatError, ChatProvider, Message, Reply, Role, ToolCall, ToolSpec};
use jarvis_llm::{OpenAiChat, gemini_key};
use mockito::Matcher;
use serde_json::json;

fn conversation() -> Vec<Message> {
    vec![
        Message::new(Role::System, "Be brief."),
        Message::new(Role::User, "Hi"),
        Message::new(Role::Assistant, "Hello."),
        Message::new(Role::User, "Two plus two?"),
    ]
}

fn client(server: &mockito::Server) -> OpenAiChat {
    OpenAiChat::new(
        &format!("{}/v1beta/openai/", server.url()),
        Some("test-key".into()),
        "gemini-3.5-flash-lite",
    )
}

#[test]
fn sends_the_conversation_and_returns_the_reply() {
    let mut server = mockito::Server::new();
    let mock = server
        .mock("POST", "/v1beta/openai/chat/completions")
        .match_header("authorization", "Bearer test-key")
        .match_header("content-type", Matcher::Regex("application/json".into()))
        .match_body(Matcher::PartialJson(json!({
            "model": "gemini-3.5-flash-lite",
            "messages": [
                {"role": "system", "content": "Be brief."},
                {"role": "user", "content": "Hi"},
                {"role": "assistant", "content": "Hello."},
                {"role": "user", "content": "Two plus two?"},
            ],
        })))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "id": "x",
                "object": "chat.completion",
                "model": "gemini-3.5-flash-lite",
                "choices": [{
                    "index": 0,
                    "finish_reason": "stop",
                    "message": {"role": "assistant", "content": "Four."},
                }],
                "usage": {"prompt_tokens": 9, "completion_tokens": 1, "total_tokens": 10},
            })
            .to_string(),
        )
        .create();

    let reply = client(&server).chat(&conversation(), &[]);

    mock.assert();
    assert_eq!(reply, Ok(Reply::Text("Four.".to_string())));
}

/// Serve one canned response to the chat endpoint and return what the client made of it.
fn reply_to(status: usize, headers: &[(&str, &str)], body: &str) -> Result<Reply, ChatError> {
    let mut server = mockito::Server::new();
    let mut mock = server
        .mock("POST", "/v1beta/openai/chat/completions")
        .with_status(status)
        .with_header("content-type", "application/json")
        .with_body(body);
    for (name, value) in headers {
        mock = mock.with_header(*name, value);
    }
    let _mock = mock.create();
    client(&server).chat(&conversation(), &[])
}

#[test]
fn unauthorized_means_the_key_is_invalid() {
    let body = json!({"error": {"code": 401, "message": "Unauthorized"}}).to_string();
    assert_eq!(reply_to(401, &[], &body), Err(ChatError::InvalidKey));
}

#[test]
fn gemini_bad_key_400_means_the_key_is_invalid() {
    // Gemini's OpenAI endpoint answers a bad key with 400, wrapped in an array.
    let body = json!([{"error": {
        "code": 400,
        "message": "API key not valid. Please pass a valid API key.",
        "status": "INVALID_ARGUMENT",
    }}])
    .to_string();
    assert_eq!(reply_to(400, &[], &body), Err(ChatError::InvalidKey));
}

#[test]
fn rate_limit_reports_the_retry_after_header() {
    let body = json!({"error": {"code": 429, "message": "Resource exhausted"}}).to_string();
    assert_eq!(
        reply_to(429, &[("retry-after", "42")], &body),
        Err(ChatError::RateLimited {
            retry_after: Some(Duration::from_secs(42))
        })
    );
}

#[test]
fn rate_limit_reports_the_google_retry_delay() {
    let body = json!([{"error": {
        "code": 429,
        "message": "You exceeded your current quota.",
        "status": "RESOURCE_EXHAUSTED",
        "details": [
            {"@type": "type.googleapis.com/google.rpc.QuotaFailure", "violations": []},
            {"@type": "type.googleapis.com/google.rpc.RetryInfo", "retryDelay": "17s"},
        ],
    }}])
    .to_string();
    assert_eq!(
        reply_to(429, &[], &body),
        Err(ChatError::RateLimited {
            retry_after: Some(Duration::from_secs(17))
        })
    );
}

#[test]
fn rate_limit_without_reset_information() {
    assert_eq!(
        reply_to(429, &[], "{}"),
        Err(ChatError::RateLimited { retry_after: None })
    );
}

#[test]
fn server_errors_report_their_status() {
    for status in [500, 503, 524] {
        assert_eq!(
            reply_to(status, &[], "upstream trouble"),
            Err(ChatError::Server {
                status: status as u16
            }),
            "status {status}"
        );
    }
}

#[test]
fn unreadable_success_is_an_error_not_a_reply() {
    assert!(matches!(
        reply_to(200, &[], "<html>oops</html>"),
        Err(ChatError::Other(_))
    ));
}

#[test]
fn slow_answer_times_out() {
    let mut server = mockito::Server::new();
    let _mock = server
        .mock("POST", "/v1beta/openai/chat/completions")
        .with_status(200)
        .with_chunked_body(|w| {
            std::thread::sleep(Duration::from_millis(1500));
            std::io::Write::write_all(w, b"{}")
        })
        .create();

    let reply = client(&server)
        .with_timeout(Duration::from_millis(300))
        .chat(&conversation(), &[]);

    assert_eq!(reply, Err(ChatError::Timeout));
}

#[test]
fn closed_port_is_unreachable() {
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let mut chat = OpenAiChat::new(
        &format!("http://127.0.0.1:{port}/v1beta/openai/"),
        Some("test-key".into()),
        "gemini-3.5-flash-lite",
    );
    assert_eq!(chat.chat(&conversation(), &[]), Err(ChatError::Unreachable));
}

#[test]
fn missing_key_sends_nothing() {
    let mut server = mockito::Server::new();
    let mock = server.mock("POST", Matcher::Any).expect(0).create();
    let mut chat = OpenAiChat::new(&server.url(), None, "gemini-3.5-flash-lite");

    assert_eq!(chat.chat(&conversation(), &[]), Err(ChatError::MissingKey));
    mock.assert();
}

#[test]
fn gemini_key_comes_from_the_environment_variable() {
    let env = |value: &'static str| {
        move |name: &str| (name == "JARVIS_GEMINI_API_KEY").then(|| value.to_string())
    };
    assert_eq!(gemini_key(env(" abc123 ")), Some("abc123".into()));
    assert_eq!(gemini_key(env("  ")), None);
    assert_eq!(gemini_key(|_| None), None);
}

#[test]
fn gemini_defaults() {
    assert_eq!(
        jarvis_llm::GEMINI_BASE_URL,
        "https://generativelanguage.googleapis.com/v1beta/openai/"
    );
    assert_eq!(jarvis_llm::DEFAULT_GEMINI_MODEL, "gemini-3.5-flash-lite");
}

fn search_tool() -> ToolSpec {
    ToolSpec {
        name: "web_search".into(),
        description: "Search the web.".into(),
        parameters: json!({
            "type": "object",
            "properties": {"query": {"type": "string"}},
            "required": ["query"],
        }),
    }
}

#[test]
fn offers_the_tools_and_returns_the_calls_the_model_asks_for() {
    let mut server = mockito::Server::new();
    let mock = server
        .mock("POST", "/v1beta/openai/chat/completions")
        .match_body(Matcher::PartialJson(json!({
            "tools": [{
                "type": "function",
                "function": {
                    "name": "web_search",
                    "description": "Search the web.",
                    "parameters": {
                        "type": "object",
                        "properties": {"query": {"type": "string"}},
                        "required": ["query"],
                    },
                },
            }],
        })))
        .with_status(200)
        .with_body(
            json!({
                "choices": [{
                    "index": 0,
                    "finish_reason": "tool_calls",
                    "message": {
                        "role": "assistant",
                        "tool_calls": [{
                            "id": "call-1",
                            "type": "function",
                            "function": {"name": "web_search", "arguments": "{\"query\":\"cats\"}"},
                            "extra_content": {"google": {"thought_signature": "sig=="}},
                        }],
                    },
                }],
            })
            .to_string(),
        )
        .create();

    let reply = client(&server).chat(&conversation(), &[search_tool()]);

    mock.assert();
    let Ok(Reply::ToolCalls(calls)) = reply else {
        panic!("expected tool calls, got {reply:?}");
    };
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].id, "call-1");
    assert_eq!(calls[0].name, "web_search");
    let args: serde_json::Value = serde_json::from_str(&calls[0].arguments).unwrap();
    assert_eq!(args, json!({"query": "cats"}));
    let data: serde_json::Value =
        serde_json::from_str(calls[0].provider_data.as_deref().unwrap()).unwrap();
    assert_eq!(data, json!({"google": {"thought_signature": "sig=="}}));
}

#[test]
fn no_tools_means_no_tools_field() {
    let mut server = mockito::Server::new();
    let mock = server
        .mock("POST", "/v1beta/openai/chat/completions")
        .match_request(|request| {
            let body: serde_json::Value = serde_json::from_slice(request.body().unwrap()).unwrap();
            body.get("tools").is_none()
        })
        .with_status(200)
        .with_body(r#"{"choices":[{"message":{"role":"assistant","content":"Hi."}}]}"#)
        .create();

    let reply = client(&server).chat(&conversation(), &[]);

    mock.assert();
    assert_eq!(reply, Ok(Reply::Text("Hi.".into())));
}

#[test]
fn sends_back_the_calls_with_their_signature_and_the_results() {
    let call = ToolCall {
        id: "call-1".into(),
        name: "web_search".into(),
        arguments: r#"{"query":"cats"}"#.into(),
        provider_data: Some(r#"{"google":{"thought_signature":"sig=="}}"#.into()),
    };
    let messages = vec![
        Message::new(Role::System, "Be brief."),
        Message::new(Role::User, "Search cats"),
        Message::calls(vec![call]),
        Message::tool_result("call-1", "Opened."),
    ];
    let mut server = mockito::Server::new();
    let mock = server
        .mock("POST", "/v1beta/openai/chat/completions")
        .match_body(Matcher::PartialJson(json!({
            "messages": [
                {"role": "system", "content": "Be brief."},
                {"role": "user", "content": "Search cats"},
                {
                    "role": "assistant",
                    "tool_calls": [{
                        "id": "call-1",
                        "type": "function",
                        "function": {"name": "web_search", "arguments": "{\"query\":\"cats\"}"},
                        "extra_content": {"google": {"thought_signature": "sig=="}},
                    }],
                },
                {"role": "tool", "tool_call_id": "call-1", "content": "Opened."},
            ],
        })))
        .with_status(200)
        .with_body(r#"{"choices":[{"message":{"role":"assistant","content":"Done."}}]}"#)
        .create();

    let reply = client(&server).chat(&messages, &[search_tool()]);

    mock.assert();
    assert_eq!(reply, Ok(Reply::Text("Done.".into())));
}
