// Claude client for vibe-control's prompt console — via the Amazon Bedrock
// runtime (this is how Claude Code itself reaches the model here:
// CLAUDE_CODE_USE_BEDROCK + AWS_BEARER_TOKEN_BEDROCK).
//
// Security posture: the bearer token is NEVER hardcoded, logged, or returned to
// the UI. It is resolved at call time (local settings → AWS_BEARER_TOKEN_BEDROCK
// env) by the caller and passed in here; only the assistant's reply text crosses
// back to the webview. Requests go to bedrock-runtime.<region>.amazonaws.com over
// HTTPS, authenticated with `Authorization: Bearer <token>` (a Bedrock API key —
// no SigV4 signing needed).

use serde::{Deserialize, Serialize};

/// Bedrock InvokeModel body version for Anthropic models.
const ANTHROPIC_BEDROCK_VERSION: &str = "bedrock-2023-05-31";
/// App default model (a Bedrock inference-profile id) when none is picked.
pub const DEFAULT_MODEL: &str = "global.anthropic.claude-opus-4-8";
/// Default region when neither settings nor AWS_REGION provide one.
pub const DEFAULT_REGION: &str = "ap-northeast-2";
const MAX_TOKENS: u32 = 4096;
const TIMEOUT_SECS: u64 = 120;

/// One turn of the console conversation, shared with the frontend.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatMsg {
    /// "user" or "assistant".
    pub role: String,
    pub content: String,
}

#[derive(Serialize)]
struct RequestBody<'a> {
    /// Bedrock takes the model in the URL, not the body; it needs the
    /// anthropic_version instead.
    anthropic_version: &'a str,
    max_tokens: u32,
    messages: &'a [ChatMsg],
}

#[derive(Deserialize)]
struct ResponseBody {
    #[serde(default)]
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    ty: String,
    #[serde(default)]
    text: String,
}

/// Encode a Bedrock model id for use as a URL path segment. The ids are
/// otherwise path-safe (`a-z 0-9 . -`); only the `:0` version suffix needs
/// escaping.
fn encode_model(model: &str) -> String {
    model.replace(':', "%3A")
}

/// POST a conversation to the Bedrock runtime and return the assistant's reply
/// text. Errors are human-readable strings and never contain the bearer token.
pub async fn send_message(
    token: &str,
    region: &str,
    model: &str,
    messages: &[ChatMsg],
) -> Result<String, String> {
    if token.trim().is_empty() {
        return Err("No Bedrock API key configured.".into());
    }
    if messages.is_empty() {
        return Err("No message to send.".into());
    }
    let region = if region.trim().is_empty() {
        DEFAULT_REGION
    } else {
        region.trim()
    };

    let url = format!(
        "https://bedrock-runtime.{}.amazonaws.com/model/{}/invoke",
        region,
        encode_model(model)
    );

    let body = RequestBody {
        anthropic_version: ANTHROPIC_BEDROCK_VERSION,
        max_tokens: MAX_TOKENS,
        messages,
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let resp = client
        .post(&url)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .header("accept", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("Reading response failed: {e}"))?;

    if !status.is_success() {
        // Surface Bedrock's error message (never the token we sent). Bedrock
        // uses `message`/`Message`; Anthropic-style bodies use `error.message`.
        let msg = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| {
                v.get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .or_else(|| v.get("message").and_then(|m| m.as_str()))
                    .or_else(|| v.get("Message").and_then(|m| m.as_str()))
                    .map(str::to_string)
            })
            .unwrap_or(text);
        return Err(format!("Bedrock error {}: {}", status.as_u16(), msg));
    }

    let parsed: ResponseBody =
        serde_json::from_str(&text).map_err(|e| format!("Parsing reply failed: {e}"))?;

    let reply = parsed
        .content
        .iter()
        .filter(|b| b.ty == "text")
        .map(|b| b.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();

    if reply.is_empty() {
        Err("Claude returned an empty reply.".into())
    } else {
        Ok(reply)
    }
}
