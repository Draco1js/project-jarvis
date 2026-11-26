//! Chat providers: Gemini (OpenAI-compatible endpoint) first, local llama-server fallback with cooldown.
//!
//! One OpenAI-compatible client, [`OpenAiChat`], serves both Gemini and (later)
//! llama-server. It implements the core's [`ChatProvider`] port.

use std::time::Duration;

use jarvis_core::{ChatError, ChatProvider, Message, Reply, Role, ToolCall, ToolSpec};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Gemini's OpenAI-compatible endpoint.
pub const GEMINI_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta/openai/";
/// Default Gemini model. Never a `gemini-2.5-*` model (redirected from 2026-12-01).
pub const DEFAULT_GEMINI_MODEL: &str = "gemini-3.5-flash-lite";
/// Environment variable holding the Gemini key until keyring storage lands.
pub const GEMINI_KEY_ENV: &str = "JARVIS_GEMINI_API_KEY";
/// How long to wait for an answer before giving up (and, later, falling back).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(8);

/// Reads the Gemini key through `lookup` (normally `std::env::var`).
/// Blank values count as missing.
pub fn gemini_key(lookup: impl Fn(&str) -> Option<String>) -> Option<String> {
    lookup(GEMINI_KEY_ENV)
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
}

/// Reads the Gemini key from the process environment.
pub fn gemini_key_from_env() -> Option<String> {
    gemini_key(|name| std::env::var(name).ok())
}

/// A chat client for any OpenAI-compatible `chat/completions` endpoint.
pub struct OpenAiChat {
    url: String,
    api_key: Option<String>,
    model: String,
    agent: ureq::Agent,
}

impl OpenAiChat {
    /// `base_url` is the API root, e.g. [`GEMINI_BASE_URL`].
    pub fn new(base_url: &str, api_key: Option<String>, model: &str) -> Self {
        let mut url = base_url.to_string();
        if !url.ends_with('/') {
            url.push('/');
        }
        url.push_str("chat/completions");
        Self {
            url,
            api_key,
            model: model.to_string(),
            agent: agent(DEFAULT_TIMEOUT),
        }
    }

    /// Gemini with the default model.
    pub fn gemini(api_key: Option<String>) -> Self {
        Self::new(GEMINI_BASE_URL, api_key, DEFAULT_GEMINI_MODEL)
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.agent = agent(timeout);
        self
    }
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .build()
        .into()
}

#[derive(Serialize)]
struct Request<'a> {
    model: &'a str,
    messages: Vec<WireMessage<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<WireTool<'a>>,
}

#[derive(Serialize)]
struct WireMessage<'a> {
    role: &'static str,
    /// Left out of an assistant turn that only calls tools.
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<WireCall<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<&'a str>,
}

#[derive(Serialize)]
struct WireTool<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    function: WireFunction<'a>,
}

#[derive(Serialize)]
struct WireFunction<'a> {
    name: &'a str,
    description: &'a str,
    parameters: &'a Value,
}

#[derive(Serialize)]
struct WireCall<'a> {
    id: &'a str,
    #[serde(rename = "type")]
    kind: &'static str,
    function: WireCallFunction<'a>,
    /// Gemini 3 rejects a follow-up whose calls lost their thought signature.
    #[serde(skip_serializing_if = "Option::is_none")]
    extra_content: Option<Value>,
}

#[derive(Serialize)]
struct WireCallFunction<'a> {
    name: &'a str,
    arguments: &'a str,
}

#[derive(Deserialize)]
struct Completion {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ReplyMessage,
}

#[derive(Deserialize)]
struct ReplyMessage {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ReplyCall>,
}

#[derive(Deserialize)]
struct ReplyCall {
    #[serde(default)]
    id: String,
    function: ReplyFunction,
    extra_content: Option<Value>,
}

#[derive(Deserialize)]
struct ReplyFunction {
    name: String,
    #[serde(default)]
    arguments: Option<String>,
}

fn wire_message(m: &Message) -> WireMessage<'_> {
    let only_calls = m.role == Role::Assistant && !m.tool_calls.is_empty() && m.content.is_empty();
    WireMessage {
        role: match m.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        },
        content: (!only_calls).then_some(m.content.as_str()),
        tool_calls: m
            .tool_calls
            .iter()
            .map(|call| WireCall {
                id: &call.id,
                kind: "function",
                function: WireCallFunction {
                    name: &call.name,
                    arguments: &call.arguments,
                },
                extra_content: call
                    .provider_data
                    .as_deref()
                    .and_then(|data| serde_json::from_str(data).ok()),
            })
            .collect(),
        tool_call_id: m.tool_call_id.as_deref(),
    }
}

impl ChatProvider for OpenAiChat {
    fn chat(&mut self, messages: &[Message], tools: &[ToolSpec]) -> Result<Reply, ChatError> {
        let key = self.api_key.as_deref().ok_or(ChatError::MissingKey)?;
        let request = Request {
            model: &self.model,
            messages: messages.iter().map(wire_message).collect(),
            tools: tools
                .iter()
                .map(|tool| WireTool {
                    kind: "function",
                    function: WireFunction {
                        name: &tool.name,
                        description: &tool.description,
                        parameters: &tool.parameters,
                    },
                })
                .collect(),
        };

        let mut response = self
            .agent
            .post(&self.url)
            .header("Authorization", &format!("Bearer {key}"))
            .send_json(&request)
            .map_err(transport_error)?;

        let status = response.status().as_u16();
        let retry_after_header = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(Duration::from_secs);
        let body = response
            .body_mut()
            .read_to_string()
            .map_err(transport_error)?;

        match status {
            200..=299 => parse_reply(&body),
            401 | 403 => Err(ChatError::InvalidKey),
            400 if body.contains("API key not valid") || body.contains("API_KEY_INVALID") => {
                Err(ChatError::InvalidKey)
            }
            429 => Err(ChatError::RateLimited {
                retry_after: retry_after_header.or_else(|| retry_delay(&body)),
            }),
            500..=599 => Err(ChatError::Server { status }),
            _ => Err(ChatError::Other(format!(
                "HTTP {status}: {}",
                error_message(&body).unwrap_or_default()
            ))),
        }
    }
}

fn transport_error(error: ureq::Error) -> ChatError {
    match error {
        ureq::Error::Timeout(_) => ChatError::Timeout,
        ureq::Error::Io(e) if e.kind() == std::io::ErrorKind::TimedOut => ChatError::Timeout,
        ureq::Error::Io(_) | ureq::Error::ConnectionFailed | ureq::Error::HostNotFound => {
            ChatError::Unreachable
        }
        other => ChatError::Other(other.to_string()),
    }
}

fn parse_reply(body: &str) -> Result<Reply, ChatError> {
    let completion: Completion = serde_json::from_str(body)
        .map_err(|e| ChatError::Other(format!("unreadable reply: {e}")))?;
    let message = completion
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| ChatError::Other("empty reply".into()))?
        .message;
    if !message.tool_calls.is_empty() {
        let calls = message
            .tool_calls
            .into_iter()
            .enumerate()
            .map(|(n, call)| ToolCall {
                // Some servers leave ids out; the follow-up still needs one.
                id: if call.id.is_empty() {
                    format!("call-{n}")
                } else {
                    call.id
                },
                name: call.function.name,
                arguments: call
                    .function
                    .arguments
                    .filter(|a| !a.trim().is_empty())
                    .unwrap_or_else(|| "{}".into()),
                provider_data: call.extra_content.map(|data| data.to_string()),
            })
            .collect();
        return Ok(Reply::ToolCalls(calls));
    }
    message
        .content
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
        .map(Reply::Text)
        .ok_or_else(|| ChatError::Other("empty reply".into()))
}

/// Gemini errors sometimes come wrapped in a one-element array.
fn error_object(body: &str) -> Option<Value> {
    let value: Value = serde_json::from_str(body).ok()?;
    let value = match value {
        Value::Array(mut items) if !items.is_empty() => items.swap_remove(0),
        other => other,
    };
    value.get("error").cloned()
}

fn error_message(body: &str) -> Option<String> {
    error_object(body)?
        .get("message")?
        .as_str()
        .map(str::to_string)
}

/// The `retryDelay` ("17s") from a Google `RetryInfo` error detail.
fn retry_delay(body: &str) -> Option<Duration> {
    let error = error_object(body)?;
    error
        .get("details")?
        .as_array()?
        .iter()
        .find_map(|detail| detail.get("retryDelay")?.as_str())
        .and_then(|delay| delay.strip_suffix('s')?.parse::<f64>().ok())
        .filter(|secs| secs.is_finite() && *secs >= 0.0)
        .map(Duration::from_secs_f64)
}
