// Coding session providers for vibe-control
// Claude Code (and future Codex/OpenCode) session readers.
// Read-only, robust line-by-line JSONL parsing (never panics — PBT-03).

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use vc_core::{Result, SessionCompletion};

/// Max conversation turns kept in a snapshot (most recent).
const MAX_TURNS: usize = 60;
/// Max characters kept per turn (avoid huge payloads across the Tauri boundary).
const MAX_CONTENT: usize = 4000;

pub trait CodingSessionProvider {
    fn tool_id(&self) -> &str;
    /// Discover available session references (file paths), most recent first.
    fn discover(&self) -> Result<Vec<SessionInfo>>;
    fn read_snapshot(&self, session_ref: &str) -> Result<SessionSnapshot>;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionInfo {
    /// Stable reference (absolute path to the .jsonl file).
    pub session_ref: String,
    /// Human-readable label (project name + short id).
    pub label: String,
    /// Unix mtime (seconds) for ordering, best-effort.
    pub modified: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub conversation: Vec<Turn>,
    pub last_question: Option<String>,
    pub completion: SessionCompletion,
    pub available: bool,
}

impl SessionSnapshot {
    fn unavailable() -> Self {
        Self {
            conversation: vec![],
            last_question: None,
            completion: SessionCompletion::Unknown,
            available: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Turn {
    pub role: String,
    pub content: String,
}

pub struct ClaudeCodeSessionProvider;

impl ClaudeCodeSessionProvider {
    /// `~/.claude/projects` — where Claude Code stores per-project session logs.
    fn projects_dir() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".claude").join("projects"))
    }

    /// Resolve a session reference to a concrete .jsonl path.
    /// Accepts: an absolute/relative path, a bare session id (UUID), or a
    /// value prefixed with `claude-code:`.
    fn resolve_path(session_ref: &str) -> Option<PathBuf> {
        let raw = session_ref
            .strip_prefix("claude-code:")
            .unwrap_or(session_ref)
            .trim();

        // Direct path hit.
        let as_path = Path::new(raw);
        if as_path.is_file() {
            return Some(as_path.to_path_buf());
        }

        // Otherwise treat it as a session id and search the projects tree.
        let stem = raw.trim_end_matches(".jsonl");
        let projects = Self::projects_dir()?;
        let entries = fs::read_dir(&projects).ok()?;
        for entry in entries.flatten() {
            let candidate = entry.path().join(format!("{stem}.jsonl"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    }

    /// The working directory a session was started in, read cheaply by
    /// streaming only the first lines that could carry `cwd` (transcripts can be
    /// many MB, so we never load the whole file just for this). Used to match an
    /// already-open terminal to the session running in it. `None` if the ref
    /// can't be resolved or no cwd line is found near the top.
    pub fn session_cwd(session_ref: &str) -> Option<String> {
        use std::io::{BufRead, BufReader};
        let path = Self::resolve_path(session_ref)?;
        let file = fs::File::open(&path).ok()?;
        for line in BufReader::new(file).lines().take(400).map_while(|l| l.ok()) {
            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if let Some(c) = v.get("cwd").and_then(Value::as_str) {
                return Some(c.to_string());
            }
        }
        None
    }

    /// (cwd, session_id) needed to resume a session with `claude --resume`.
    /// cwd is read from the first log line that carries it.
    pub fn resume_info(session_ref: &str) -> Option<(String, String)> {
        let path = Self::resolve_path(session_ref)?;
        let id = path.file_stem().and_then(|s| s.to_str())?.to_string();
        let content = fs::read_to_string(&path).ok()?;
        let cwd = content.lines().find_map(|line| {
            let v: Value = serde_json::from_str(line).ok()?;
            v.get("cwd").and_then(Value::as_str).map(str::to_string)
        })?;
        Some((cwd, id))
    }
}

impl CodingSessionProvider for ClaudeCodeSessionProvider {
    fn tool_id(&self) -> &str {
        "claude-code"
    }

    fn discover(&self) -> Result<Vec<SessionInfo>> {
        let Some(projects) = Self::projects_dir() else {
            return Ok(vec![]);
        };
        let Ok(project_entries) = fs::read_dir(&projects) else {
            return Ok(vec![]);
        };

        let mut sessions: Vec<SessionInfo> = Vec::new();
        for project in project_entries.flatten() {
            let project_path = project.path();
            if !project_path.is_dir() {
                continue;
            }
            let project_label = project_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            let Ok(files) = fs::read_dir(&project_path) else {
                continue;
            };
            for file in files.flatten() {
                let path = file.path();
                if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }
                let id = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                let modified = file
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let short_id = id.split('-').next().unwrap_or(&id);
                sessions.push(SessionInfo {
                    session_ref: path.to_string_lossy().into_owned(),
                    label: format!("{} · {}", friendly_project(&project_label), short_id),
                    modified,
                });
            }
        }

        // Most recently modified first.
        sessions.sort_by_key(|s| std::cmp::Reverse(s.modified));
        Ok(sessions)
    }

    fn read_snapshot(&self, session_ref: &str) -> Result<SessionSnapshot> {
        let Some(path) = Self::resolve_path(session_ref) else {
            return Ok(SessionSnapshot::unavailable());
        };
        match fs::read(&path) {
            Ok(bytes) => Ok(parse_session_bytes(&bytes)),
            Err(_) => Ok(SessionSnapshot::unavailable()),
        }
    }
}

/// Decode a Claude-Code-style project dir name (`-Users-ezitsu-code-foo`) into
/// something readable (the last path segment).
fn friendly_project(encoded: &str) -> String {
    encoded
        .rsplit('-')
        .find(|s| !s.is_empty())
        .unwrap_or(encoded)
        .to_string()
}

/// Parse raw JSONL bytes into a snapshot. Pure and panic-free: unparseable or
/// unexpected lines are skipped rather than aborting (PBT-03).
pub fn parse_session_bytes(raw: &[u8]) -> SessionSnapshot {
    let text = String::from_utf8_lossy(raw);
    let mut turns: Vec<Turn> = Vec::new();

    // Completion signal derived from the last meaningful event.
    let mut last_completion = SessionCompletion::Unknown;
    // The user's most recent real prompt. This is what we surface as the
    // session's "last question" line — what *you* last asked — independent of
    // whether the agent is currently working or waiting.
    let mut last_user_text: Option<String> = None;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };

        // Skip harness metadata and subagent side-channels.
        if value.get("isMeta").and_then(Value::as_bool) == Some(true)
            || value.get("isSidechain").and_then(Value::as_bool) == Some(true)
        {
            continue;
        }

        let ty = value.get("type").and_then(Value::as_str).unwrap_or("");
        if ty != "user" && ty != "assistant" {
            continue;
        }
        let Some(message) = value.get("message") else {
            continue;
        };
        let content = message.get("content");

        match ty {
            "user" => {
                // Harness-injected pseudo-prompts (a subagent task-notification,
                // other system messages) are logged as `user` turns but are NOT
                // something the user asked — skip them so they never surface as
                // the "last question". Treated like tool output: the agent is
                // working on them.
                if is_injected_prompt(&value) {
                    last_completion = SessionCompletion::NotWaiting;
                    continue;
                }
                let text = extract_text(content);
                // A user line that carries only tool_result (no prose) is
                // machine feedback, not a real user turn.
                if text.trim().is_empty() {
                    // The assistant is still working after tool output.
                    last_completion = SessionCompletion::NotWaiting;
                    continue;
                }
                push_turn(&mut turns, "user", &text);
                last_completion = SessionCompletion::NotWaiting;
                last_user_text = Some(text);
            }
            "assistant" => {
                let text = extract_text(content);
                if ends_with_tool_use(content) && text.trim().is_empty() {
                    // Message is a pure tool call — still executing.
                    last_completion = SessionCompletion::NotWaiting;
                } else if !text.trim().is_empty() {
                    push_turn(&mut turns, "assistant", &text);
                    // Assistant produced a reply → ball is in the user's court.
                    last_completion = SessionCompletion::Waiting;
                }
            }
            _ => {}
        }
    }

    if turns.len() > MAX_TURNS {
        turns = turns.split_off(turns.len() - MAX_TURNS);
    }

    let available = !turns.is_empty();
    // Surface the user's most recent prompt (truncated for the one-line UI),
    // shown whenever the session has one — NOT gated on completion state — so
    // "your last question" stays visible even while the agent is still working.
    let last_question = last_user_text.map(|t| truncate(t.trim(), 500));

    SessionSnapshot {
        conversation: turns,
        last_question,
        completion: if available {
            last_completion
        } else {
            SessionCompletion::Unknown
        },
        available,
    }
}

/// Whether a `user`-type line is a harness-injected pseudo-prompt rather than
/// something the user actually typed. Claude Code marks injected turns (subagent
/// task-notifications, other system messages) with `promptSource: "system"` or
/// an `origin.kind` other than `"human"`. A deny-list: lines lacking these
/// fields (older transcripts, other tools) still count as real prompts.
fn is_injected_prompt(value: &Value) -> bool {
    if value.get("promptSource").and_then(Value::as_str) == Some("system") {
        return true;
    }
    match value.get("origin").and_then(|o| o.get("kind")).and_then(Value::as_str) {
        Some(kind) => kind != "human",
        None => false,
    }
}

/// Concatenate all human-readable text from a message `content` value.
/// Handles a plain string, or an array of blocks (keeping only `text` blocks,
/// dropping `thinking` / `tool_use` / `tool_result`).
fn extract_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(blocks)) => {
            let mut parts: Vec<String> = Vec::new();
            for block in blocks {
                if block.get("type").and_then(Value::as_str) == Some("text") {
                    if let Some(t) = block.get("text").and_then(Value::as_str) {
                        parts.push(t.to_string());
                    }
                }
            }
            parts.join("\n")
        }
        _ => String::new(),
    }
}

/// True if the last block of an array content is a `tool_use`.
fn ends_with_tool_use(content: Option<&Value>) -> bool {
    matches!(content, Some(Value::Array(blocks))
        if blocks.last().and_then(|b| b.get("type")).and_then(Value::as_str) == Some("tool_use"))
}

fn push_turn(turns: &mut Vec<Turn>, role: &str, content: &str) {
    turns.push(Turn {
        role: role.to_string(),
        content: truncate(content.trim(), MAX_CONTENT),
    });
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

pub struct SessionProviderRegistry {
    providers: Vec<Box<dyn CodingSessionProvider>>,
}

impl Default for SessionProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: vec![Box::new(ClaudeCodeSessionProvider)],
        }
    }

    pub fn register(&mut self, provider: Box<dyn CodingSessionProvider>) {
        self.providers.push(provider);
    }

    pub fn provider(&self, tool_id: &str) -> Option<&dyn CodingSessionProvider> {
        self.providers
            .iter()
            .find(|p| p.tool_id() == tool_id)
            .map(|b| &**b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry() {
        let registry = SessionProviderRegistry::new();
        assert!(registry.provider("claude-code").is_some());
    }

    #[test]
    fn parses_basic_conversation() {
        let jsonl = concat!(
            r#"{"type":"user","message":{"role":"user","content":"안녕 이거 고쳐줘"}}"#,
            "\n",
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"thinking","thinking":"hmm"},{"type":"text","text":"네 고쳤습니다. 더 필요하신 게 있나요?"}]}}"#,
            "\n",
        );
        let snap = parse_session_bytes(jsonl.as_bytes());
        assert!(snap.available);
        assert_eq!(snap.conversation.len(), 2);
        assert_eq!(snap.conversation[0].role, "user");
        assert_eq!(snap.conversation[1].role, "assistant");
        // Ends on an assistant reply → waiting for the user.
        assert_eq!(snap.completion, SessionCompletion::Waiting);
        // last_question surfaces the USER's most recent prompt (not the
        // assistant's question).
        assert_eq!(snap.last_question.as_deref(), Some("안녕 이거 고쳐줘"));
    }

    #[test]
    fn tool_result_only_user_line_is_not_a_turn() {
        let jsonl = concat!(
            r#"{"type":"user","message":{"role":"user","content":"start"}}"#,
            "\n",
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"1","name":"Bash","input":{}}]}}"#,
            "\n",
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"1","content":"ok"}]}}"#,
            "\n",
        );
        let snap = parse_session_bytes(jsonl.as_bytes());
        // Only the real user prompt becomes a turn.
        assert_eq!(snap.conversation.len(), 1);
        assert_eq!(snap.conversation[0].role, "user");
        // Last event is a tool_result feeding the assistant → still working.
        assert_eq!(snap.completion, SessionCompletion::NotWaiting);
        // …yet your last prompt is still surfaced (the bug: it used to blank
        // out whenever the session wasn't in the Waiting state).
        assert_eq!(snap.last_question.as_deref(), Some("start"));
    }

    #[test]
    fn skips_injected_system_prompts() {
        // A real typed prompt, then a harness-injected task-notification logged
        // as a `user` turn. The notification must NOT become the last_question.
        let jsonl = concat!(
            r#"{"type":"user","origin":{"kind":"human"},"promptSource":"typed","message":{"role":"user","content":"이미지 약간 더 크게"}}"#,
            "\n",
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"키웠습니다."}]}}"#,
            "\n",
            r#"{"type":"user","origin":{"kind":"task-notification"},"promptSource":"system","message":{"role":"user","content":"<task-notification><task-id>abc</task-id></task-notification>"}}"#,
            "\n",
        );
        let snap = parse_session_bytes(jsonl.as_bytes());
        // The injected notification is neither a turn nor the last question.
        assert_eq!(snap.last_question.as_deref(), Some("이미지 약간 더 크게"));
        assert!(snap.conversation.iter().all(|t| !t.content.contains("task-notification")));
        // It arrived after the assistant replied → agent is working on it.
        assert_eq!(snap.completion, SessionCompletion::NotWaiting);
    }

    #[test]
    fn skips_meta_and_sidechain_and_garbage() {
        let jsonl = concat!(
            "not json at all\n",
            r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"meta"}}"#,
            "\n",
            r#"{"type":"assistant","isSidechain":true,"message":{"role":"assistant","content":[{"type":"text","text":"sub"}]}}"#,
            "\n",
            r#"{"type":"summary"}"#,
            "\n",
            r#"{"type":"user","message":{"role":"user","content":"real"}}"#,
            "\n",
        );
        let snap = parse_session_bytes(jsonl.as_bytes());
        assert_eq!(snap.conversation.len(), 1);
        assert_eq!(snap.conversation[0].content, "real");
    }

    #[test]
    fn resume_info_reads_cwd_and_id() {
        let dir = std::env::temp_dir().join(format!("vc-sessions-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("abc123-def.jsonl");
        let jsonl = concat!(
            r#"{"type":"summary"}"#,
            "\n",
            r#"{"type":"user","cwd":"/Users/me/proj","message":{"role":"user","content":"hi"}}"#,
            "\n",
        );
        fs::write(&path, jsonl).unwrap();

        let (cwd, id) =
            ClaudeCodeSessionProvider::resume_info(&path.to_string_lossy()).expect("resume info");
        assert_eq!(cwd, "/Users/me/proj");
        assert_eq!(id, "abc123-def");

        // Missing session → None (never panics).
        assert!(ClaudeCodeSessionProvider::resume_info("no-such-session-xyz").is_none());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_and_unavailable() {
        let snap = parse_session_bytes(b"");
        assert!(!snap.available);
        assert_eq!(snap.completion, SessionCompletion::Unknown);

        let missing = ClaudeCodeSessionProvider
            .read_snapshot("claude-code:definitely-not-a-real-session-id-xyz")
            .unwrap();
        assert!(!missing.available);
    }
}

#[cfg(test)]
mod pbt {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        // PBT-03: parser never panics on arbitrary bytes.
        #[test]
        fn prop_parser_robust(raw in prop::collection::vec(any::<u8>(), 0..1024)) {
            let _ = parse_session_bytes(&raw);
        }

        // Arbitrary newline-joined arbitrary strings also never panic.
        #[test]
        fn prop_lines_robust(lines in prop::collection::vec("\\PC*", 0..20)) {
            let joined = lines.join("\n");
            let _ = parse_session_bytes(joined.as_bytes());
        }
    }
}
