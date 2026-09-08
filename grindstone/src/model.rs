//! Permissive serde models for the transcript JSONL format.
//!
//! The format evolves with every CLI release, so every field is optional and
//! unknown fields are ignored. Entry types we don't model are *counted*, never
//! fatal (spec §2 / design rule 4).

use serde::Deserialize;
use serde_json::Value;

/// Entry types grindstone models. Anything else lands in `unknown_types`.
pub const KNOWN_TYPES: &[&str] = &[
    "user",
    "assistant",
    "system",
    "attachment",
    "permission-mode",
    "file-history-delta",
    "file-history-snapshot",
    "queue-operation",
];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawEntry {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub uuid: Option<String>,
    // NB: no sessionId field — sessions are keyed by file stem, and some
    // entries carry BOTH `sessionId` and `session_id`, which a serde alias
    // would reject as a duplicate field.
    pub timestamp: Option<String>,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub version: Option<String>,
    pub entrypoint: Option<String>,
    #[serde(default)]
    pub is_sidechain: bool,
    #[serde(default)]
    pub is_meta: bool,
    // user
    pub prompt_id: Option<String>,
    pub tool_denial_kind: Option<String>,
    // assistant
    pub request_id: Option<String>,
    pub effort: Option<String>,
    // user + assistant
    pub message: Option<Value>,
    // queue-operation
    pub operation: Option<String>,
    // file-history-delta
    pub tracking_path: Option<String>,
    // file-history-snapshot
    pub snapshot: Option<Value>,
}

impl RawEntry {
    pub fn ts_ms(&self) -> Option<i64> {
        self.timestamp.as_deref().and_then(crate::util::parse_ts_ms)
    }
}

/// Extracted view of a `user` entry's `message.content`.
pub struct UserContent<'a> {
    /// Concatenated text of the string form or all `text` blocks.
    pub text: String,
    pub tool_results: Vec<ToolResultBlock<'a>>,
}

pub struct ToolResultBlock<'a> {
    pub tool_use_id: &'a str,
    pub is_error: bool,
    /// Serialized JSON length of the `content` field (content-safe size proxy).
    pub content_bytes: i64,
    pub content_text: String,
}

pub fn extract_user_content(message: Option<&Value>) -> UserContent<'_> {
    let mut out = UserContent {
        text: String::new(),
        tool_results: Vec::new(),
    };
    let Some(content) = message.and_then(|m| m.get("content")) else {
        return out;
    };
    match content {
        Value::String(s) => out.text = s.clone(),
        Value::Array(blocks) => {
            for b in blocks {
                match b.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        if let Some(t) = b.get("text").and_then(Value::as_str) {
                            if !out.text.is_empty() {
                                out.text.push('\n');
                            }
                            out.text.push_str(t);
                        }
                    }
                    Some("tool_result") => {
                        let Some(id) = b.get("tool_use_id").and_then(Value::as_str) else {
                            continue;
                        };
                        let content = b.get("content");
                        let content_text = match content {
                            Some(Value::String(s)) => s.clone(),
                            Some(v @ Value::Array(_)) => collect_text_blocks(v),
                            _ => String::new(),
                        };
                        out.tool_results.push(ToolResultBlock {
                            tool_use_id: id,
                            is_error: b.get("is_error").and_then(Value::as_bool).unwrap_or(false),
                            content_bytes: content
                                .map(|c| serde_json::to_string(c).map(|s| s.len()).unwrap_or(0))
                                .unwrap_or(0) as i64,
                            content_text,
                        });
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
    out
}

fn collect_text_blocks(v: &Value) -> String {
    let mut s = String::new();
    if let Value::Array(items) = v {
        for it in items {
            if it.get("type").and_then(Value::as_str) == Some("text") {
                if let Some(t) = it.get("text").and_then(Value::as_str) {
                    if !s.is_empty() {
                        s.push('\n');
                    }
                    s.push_str(t);
                }
            }
        }
    }
    s
}

/// Extracted view of an `assistant` entry's `message`.
pub struct AssistantMessage<'a> {
    pub model: Option<&'a str>,
    pub stop_reason: Option<&'a str>,
    pub usage: Option<&'a Value>,
    pub tool_uses: Vec<ToolUseBlock<'a>>,
    pub n_text_blocks: i32,
    pub texts: Vec<(&'static str, String)>, // (kind, text) for --include-text
}

pub struct ToolUseBlock<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub input: Option<&'a Value>,
}

pub fn extract_assistant_message(message: Option<&Value>, want_text: bool) -> AssistantMessage<'_> {
    let mut out = AssistantMessage {
        model: None,
        stop_reason: None,
        usage: None,
        tool_uses: Vec::new(),
        n_text_blocks: 0,
        texts: Vec::new(),
    };
    let Some(m) = message else { return out };
    out.model = m.get("model").and_then(Value::as_str);
    out.stop_reason = m.get("stop_reason").and_then(Value::as_str);
    out.usage = m.get("usage");
    if let Some(Value::Array(blocks)) = m.get("content") {
        for b in blocks {
            match b.get("type").and_then(Value::as_str) {
                Some("tool_use") => {
                    if let Some(id) = b.get("id").and_then(Value::as_str) {
                        out.tool_uses.push(ToolUseBlock {
                            id,
                            name: b.get("name").and_then(Value::as_str).unwrap_or(""),
                            input: b.get("input"),
                        });
                    }
                }
                Some("text") => {
                    out.n_text_blocks += 1;
                    if want_text {
                        if let Some(t) = b.get("text").and_then(Value::as_str) {
                            out.texts.push(("assistant_text", t.to_string()));
                        }
                    }
                }
                Some("thinking") if want_text => {
                    if let Some(t) = b.get("thinking").and_then(Value::as_str) {
                        out.texts.push(("thinking", t.to_string()));
                    }
                }
                _ => {}
            }
        }
    }
    out
}

pub fn usage_i64(usage: Option<&Value>, key: &str) -> i64 {
    usage
        .and_then(|u| u.get(key))
        .and_then(Value::as_i64)
        .unwrap_or(0)
}

pub fn usage_nested_i64(usage: Option<&Value>, outer: &str, key: &str) -> i64 {
    usage
        .and_then(|u| u.get(outer))
        .and_then(|o| o.get(key))
        .and_then(Value::as_i64)
        .unwrap_or(0)
}

pub fn usage_str<'a>(usage: Option<&'a Value>, key: &str) -> Option<&'a str> {
    usage.and_then(|u| u.get(key)).and_then(Value::as_str)
}
