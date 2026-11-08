//! The deterministic answer policies of the one stub LLM (ported from
//! `scripts/lib/stub-llm.ts`). Each derives its answer ONLY from the
//! request bytes — never randomness, never the clock — so a pipeline run
//! against the stub is reproducible end to end.

use sha2::{Digest, Sha256};

/// Every identifier a naming prompt asks for, in the order it lists them
/// (the `Identifiers to rename: a, b` line of any message).
pub fn asked_identifiers(request_body: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(request_body) else {
        return Vec::new();
    };
    let messages = v
        .get("messages")
        .and_then(|m| m.as_array())
        .cloned()
        .unwrap_or_default();
    for m in messages {
        let content = m.get("content").and_then(|c| c.as_str()).unwrap_or("");
        if let Some(idx) = content.find("Identifiers to rename: ") {
            let rest = &content[idx + "Identifiers to rename: ".len()..];
            let line = rest.lines().next().unwrap_or("");
            return line
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect();
        }
    }
    Vec::new()
}

/// The user prompt of a request (the last message's content; empty when
/// the body is not a chat request).
pub fn user_prompt(request_body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(request_body)
        .ok()
        .and_then(|v| {
            let last = v.get("messages")?.as_array()?.last()?.clone();
            last.get("content")?.as_str().map(str::to_string)
        })
        .unwrap_or_default()
}

const MODULE_WORDS: [&str; 26] = [
    "amber", "birch", "cedar", "delta", "ember", "fjord", "grove", "harbor", "inlet", "juniper",
    "kelp", "lagoon", "meadow", "nectar", "orchid", "prairie", "quartz", "ridge", "summit",
    "tundra", "upland", "valley", "willow", "yarrow", "zephyr", "basin",
];

/// A JS identifier the stub will rename (`/^[A-Za-z_$][\w$]*$/`).
fn is_identifier(id: &str) -> bool {
    let mut chars = id.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// A module batch (`"m1": "<file-name>"` keys) gets one four-word kebab
/// name per key, chosen by a hash of the key and the WHOLE prompt — a
/// repeat asks the same; the duplicate retry (its prompt lists the taken
/// names) draws a different one.
fn module_answer(prompt: &str) -> Option<serde_json::Map<String, serde_json::Value>> {
    let mut keys = Vec::new();
    let mut rest = prompt;
    while let Some(i) = rest.find("\"m") {
        let after = &rest[i + 2..];
        let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
        if !digits.is_empty() && after[digits.len()..].starts_with("\": \"<file-name>\"") {
            keys.push(format!("m{digits}"));
        }
        rest = &rest[i + 2..];
    }
    if keys.is_empty() {
        return None;
    }
    let mut out = serde_json::Map::new();
    for key in keys {
        let digest = Sha256::digest(format!("{key}\n{prompt}").as_bytes());
        let name: Vec<&str> = (0..4)
            .map(|i| MODULE_WORDS[usize::from(digest[i]) % MODULE_WORDS.len()])
            .collect();
        out.insert(key, serde_json::Value::String(name.join("-")));
    }
    Some(out)
}

fn answer_with(request_body: &str, name: impl Fn(&str) -> String) -> String {
    if let Some(modules) = module_answer(&user_prompt(request_body)) {
        return serde_json::Value::Object(modules).to_string();
    }
    let mut out = serde_json::Map::new();
    for id in asked_identifiers(request_body) {
        if is_identifier(&id) {
            let n = name(&id);
            out.insert(id, serde_json::Value::String(n));
        }
    }
    serde_json::Value::Object(out).to_string()
}

/// The default policy (the e2e gate's): every asked identifier maps to
/// `<id>Renamed` — every rename lands, nothing collides; a module batch
/// gets four-word names.
pub fn renamed(request_body: &str) -> String {
    answer_with(request_body, |id| format!("{id}Renamed"))
}

/// The collision-forcing policy (ask-trace's `--collide <name>`): every
/// asked identifier gets the SAME name.
pub fn collide(name: &str) -> impl Fn(&str) -> String + '_ {
    move |request_body| answer_with(request_body, |_| name.to_string())
}

/// A chat-completions response body whose first choice carries `content`
/// (`None`: a null content).
pub fn completion(content: Option<&str>) -> String {
    let content = content.map_or(serde_json::Value::Null, |c| {
        serde_json::Value::String(c.to_string())
    });
    serde_json::json!({
        "id": "stub",
        "object": "chat.completion",
        "created": 0,
        "model": "stub",
        "choices": [{"index": 0, "finish_reason": "stop", "message": {"role": "assistant", "content": content}}],
        "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
    })
    .to_string()
}
