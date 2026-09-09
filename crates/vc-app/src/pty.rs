// Interactive Claude-CLI sessions over a PTY (the "A" feature — 스마트 프롬프트 감지).
//
// We run `claude` in a real pseudo-terminal so its INTERACTIVE prompts
// (permission menus, selection lists) actually render — headless `-p` mode never
// shows them. A background reader thread feeds the PTY output through a vt100
// terminal emulator, giving us a clean text "screen". From that screen we DETECT
// selection prompts (a question + numbered/pointer options + which one is
// selected) and hand the frontend a structured `DetectedPrompt`, so it can render
// native EP-133 buttons; clicking a button writes the matching keystroke back
// into the PTY. The raw rendered screen is always exposed too, so even when
// detection misses the user still sees the terminal and can drive it with the
// arrow / enter / digit key buttons.
//
// All local: the PTY talks to the user's own `claude` CLI (its own local auth);
// no Bedrock token and no external send are involved (§12 preserved).

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use vc_sessions::ClaudeCodeSessionProvider;

/// Emulated terminal size. Tall/wide enough that Claude's boxed prompts and
/// option lists render without wrapping in most cases.
const ROWS: u16 = 45;
const COLS: u16 = 120;

/// One live interactive PTY-backed `claude` process. Every field is behind a
/// `Mutex`/`Arc` so the whole struct is `Send + Sync` and can live in the shared
/// registry while a background thread feeds the parser.
struct PtySession {
    /// Kept alive for the lifetime of the session so the cloned reader stays
    /// valid; never touched after setup except to drop on stop.
    _master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    parser: Arc<Mutex<vt100::Parser>>,
    /// Flipped to false when the reader hits EOF (the `claude` process exited).
    alive: Arc<AtomicBool>,
}

impl Drop for PtySession {
    /// Kill the `claude` child when the session leaves the registry (stop / reuse
    /// replacement / app teardown) so a PTY-backed process never orphans.
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
        }
    }
}

/// Process-lifetime registry of interactive sessions, keyed by session ref.
fn registry() -> &'static Mutex<HashMap<String, Arc<PtySession>>> {
    static REG: OnceLock<Mutex<HashMap<String, Arc<PtySession>>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The Tauri app handle, set once at startup so PTY reader threads can emit raw
/// output chunks to the frontend (xterm.js). `None` until `set_app_handle` runs.
fn app_handle() -> &'static OnceLock<AppHandle> {
    static H: OnceLock<AppHandle> = OnceLock::new();
    &H
}

/// Register the app handle so reader threads can emit PTY output. Called once
/// from `run()` setup; a second call is a no-op.
pub fn set_app_handle(handle: AppHandle) {
    let _ = app_handle().set(handle);
}

/// Event name the frontend listens on for raw PTY output chunks.
const PTY_OUTPUT_EVENT: &str = "pty://output";

/// One chunk of raw PTY output streamed to the frontend terminal. `bytes` is the
/// verbatim PTY output (ANSI escapes included) — the frontend writes it straight
/// into the matching group's xterm.js instance, so rendering is faithful (colors,
/// cursor, scrollback) rather than a scraped text approximation.
#[derive(Serialize, Clone)]
struct PtyChunk {
    /// Session ref (registry key) this output belongs to, so the frontend routes
    /// it to the right group's terminal.
    id: String,
    /// Raw output bytes (crosses the bridge as a JSON number array).
    bytes: Vec<u8>,
}

fn get_arc(id: &str) -> Result<Arc<PtySession>, String> {
    let reg = registry().lock().map_err(|_| "pty 레지스트리 잠금 실패".to_string())?;
    reg.get(id)
        .cloned()
        .ok_or_else(|| "실행 중인 대화형 세션이 없습니다".to_string())
}

/// A selection prompt detected in the rendered screen: the question text, the
/// option labels in order, and which one is currently highlighted.
#[derive(Serialize, Clone)]
pub struct DetectedPrompt {
    pub question: String,
    pub options: Vec<String>,
    pub selected: usize,
}

/// A snapshot handed to the frontend each poll: liveness, the clean screen lines,
/// and the detected selection prompt (if any).
#[derive(Serialize, Clone)]
pub struct InteractiveScreen {
    pub alive: bool,
    pub lines: Vec<String>,
    pub prompt: Option<DetectedPrompt>,
}

/// Start (or reuse) an interactive `claude --resume <id>` PTY for `session_ref`.
/// Optionally writes an initial prompt + Enter once the REPL is up. Returns the
/// registry key (the session ref) the frontend then polls / sends keys to.
pub fn start(session_ref: &str, initial_prompt: Option<&str>) -> Result<String, String> {
    // Reuse a still-running session rather than spawning a duplicate. Checked
    // FIRST (before reading the session file) so a live session — including a
    // brand-new one registered by `start_new` whose `.jsonl` may not exist yet —
    // is reused without a spurious "path not found".
    {
        let reg = registry().lock().map_err(|_| "pty 레지스트리 잠금 실패".to_string())?;
        if let Some(s) = reg.get(session_ref) {
            if s.alive.load(Ordering::SeqCst) {
                return Ok(session_ref.to_string());
            }
        }
    }

    let (cwd, id) = ClaudeCodeSessionProvider::resume_info(session_ref)
        .ok_or_else(|| "세션 경로/ID를 확인할 수 없습니다".to_string())?;

    // Resume the existing session log in its recorded working directory. The
    // reader thread + registration + optional initial prompt are handled by the
    // shared spawn helper (same path as a brand-new session).
    let mut builder = CommandBuilder::new("claude");
    builder.arg("--resume");
    builder.arg(&id);
    spawn_into_registry(session_ref, &cwd, builder, initial_prompt)
}

/// Start a BRAND-NEW interactive `claude --session-id <id>` PTY for `session_ref`.
/// Unlike [`start`], the session log does not exist yet, so cwd + id are passed
/// directly (we can't read them back from a file). `claude` creates the `.jsonl`
/// on first turn; the caller attaches `session_id` as the resource descriptor
/// (which `resolve_path` then finds once the file lands). Reuses a live session
/// under the same ref rather than spawning a duplicate.
pub fn start_new(
    session_ref: &str,
    cwd: &str,
    session_id: &str,
    initial_prompt: Option<&str>,
) -> Result<String, String> {
    {
        let reg = registry().lock().map_err(|_| "pty 레지스트리 잠금 실패".to_string())?;
        if let Some(s) = reg.get(session_ref) {
            if s.alive.load(Ordering::SeqCst) {
                return Ok(session_ref.to_string());
            }
        }
    }
    let mut builder = CommandBuilder::new("claude");
    builder.arg("--session-id");
    builder.arg(session_id);
    spawn_into_registry(session_ref, cwd, builder, initial_prompt)
}

/// Shared PTY spawn: open a pty, run `builder` in `cwd`, wire a reader thread into
/// a vt100 parser, register the session under `session_ref`, and optionally submit
/// an initial prompt. Used by both [`start`] (resume) and [`start_new`].
fn spawn_into_registry(
    session_ref: &str,
    cwd: &str,
    mut builder: CommandBuilder,
    initial_prompt: Option<&str>,
) -> Result<String, String> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: ROWS,
            cols: COLS,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("PTY 생성 실패: {e}"))?;

    builder.cwd(cwd);
    for (k, v) in std::env::vars() {
        builder.env(k, v);
    }
    builder.env("TERM", "xterm-256color");

    let child = pair
        .slave
        .spawn_command(builder)
        .map_err(|e| format!("claude 실행 실패: {e}"))?;
    drop(pair.slave);

    let writer = pair
        .master
        .take_writer()
        .map_err(|e| format!("PTY 입력 스트림 실패: {e}"))?;
    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("PTY 출력 스트림 실패: {e}"))?;

    let parser = Arc::new(Mutex::new(vt100::Parser::new(ROWS, COLS, 2000)));
    let alive = Arc::new(AtomicBool::new(true));
    {
        let parser_bg = parser.clone();
        let alive_bg = alive.clone();
        let sref = session_ref.to_string();
        let mut reader = reader;
        thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        // Feed the vt100 parser (drives `screen()`/summary) AND
                        // stream the raw bytes to the frontend terminal (xterm.js).
                        if let Ok(mut p) = parser_bg.lock() {
                            p.process(&buf[..n]);
                        }
                        if let Some(h) = app_handle().get() {
                            let _ = h.emit(
                                PTY_OUTPUT_EVENT,
                                PtyChunk {
                                    id: sref.clone(),
                                    bytes: buf[..n].to_vec(),
                                },
                            );
                        }
                    }
                    Err(_) => break,
                }
            }
            alive_bg.store(false, Ordering::SeqCst);
        });
    }

    let session = Arc::new(PtySession {
        _master: Mutex::new(pair.master),
        writer: Mutex::new(writer),
        child: Mutex::new(child),
        parser,
        alive,
    });
    registry()
        .lock()
        .map_err(|_| "pty 레지스트리 잠금 실패".to_string())?
        .insert(session_ref.to_string(), session);

    if let Some(p) = initial_prompt {
        let trimmed = p.trim();
        if !trimmed.is_empty() {
            write_input(session_ref, trimmed.as_bytes())?;
            write_input(session_ref, b"\r")?;
        }
    }
    Ok(session_ref.to_string())
}

/// Type a full line into the REPL and submit it (text + Enter) in one call, so the
/// UI's send box reliably lands a turn without two racing round-trips.
pub fn submit_line(id: &str, text: &str) -> Result<(), String> {
    write_input(id, text.as_bytes())?;
    write_input(id, b"\r")
}

/// Kill every live interactive session and clear the registry. Called on app exit
/// so PTY-backed `claude` children never orphan (they otherwise survive an app
/// restart because the in-memory registry is wiped but the OS processes are not).
pub fn stop_all() {
    let sessions: Vec<Arc<PtySession>> = {
        match registry().lock() {
            Ok(mut reg) => reg.drain().map(|(_, s)| s).collect(),
            Err(_) => return,
        }
    };
    for s in sessions {
        if let Ok(mut child) = s.child.lock() {
            let _ = child.kill();
        }
        s.alive.store(false, Ordering::SeqCst);
    }
}

/// Resize the PTY (and its vt100 parser) to `rows`x`cols` so `claude`'s TUI
/// repaints to fit the frontend terminal — keeps boxes/wrapping aligned with the
/// actual on-screen width instead of the fixed startup size. No-op-safe: a zero
/// dimension is ignored (xterm reports 0 before its first layout).
pub fn resize(id: &str, rows: u16, cols: u16) -> Result<(), String> {
    if rows == 0 || cols == 0 {
        return Ok(());
    }
    let session = get_arc(id)?;
    {
        let master = session._master.lock().map_err(|_| "master 잠금 실패".to_string())?;
        master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("PTY 크기 변경 실패: {e}"))?;
    }
    if let Ok(mut p) = session.parser.lock() {
        p.screen_mut().set_size(rows, cols);
    }
    Ok(())
}

/// Write raw bytes to the PTY (typing into the REPL / answering a prompt).
pub fn write_input(id: &str, bytes: &[u8]) -> Result<(), String> {
    let session = get_arc(id)?;
    let mut w = session.writer.lock().map_err(|_| "입력 스트림 잠금 실패".to_string())?;
    w.write_all(bytes).map_err(|e| format!("입력 전송 실패: {e}"))?;
    w.flush().map_err(|e| format!("입력 flush 실패: {e}"))
}

/// Translate a named key from the UI into the terminal escape/byte sequence and
/// send it. Unknown names are written verbatim (covers digit keys "1".."9").
pub fn send_key(id: &str, key: &str) -> Result<(), String> {
    let bytes: &[u8] = match key {
        "up" => b"\x1b[A",
        "down" => b"\x1b[B",
        "left" => b"\x1b[D",
        "right" => b"\x1b[C",
        "enter" => b"\r",
        "esc" => b"\x1b",
        "tab" => b"\t",
        "backspace" => b"\x7f",
        "space" => b" ",
        other => return write_input(id, other.as_bytes()),
    };
    write_input(id, bytes)
}

/// Capture the current screen and detect any selection prompt on it.
pub fn screen(id: &str) -> Result<InteractiveScreen, String> {
    let session = get_arc(id)?;
    let alive = session.alive.load(Ordering::SeqCst);
    let parser = session.parser.lock().map_err(|_| "parser 잠금 실패".to_string())?;
    let scr = parser.screen();
    // Use the parser's CURRENT width (it changes on resize), not the startup const.
    let cols = scr.size().1;
    let lines: Vec<String> = scr.rows(0, cols).collect();
    let prompt = detect_prompt(&lines);
    Ok(InteractiveScreen {
        alive,
        lines,
        prompt,
    })
}

/// Kill the interactive `claude` process and drop it from the registry.
pub fn stop(id: &str) -> Result<(), String> {
    let removed = {
        let mut reg = registry().lock().map_err(|_| "pty 레지스트리 잠금 실패".to_string())?;
        reg.remove(id)
    };
    if let Some(s) = removed {
        if let Ok(mut child) = s.child.lock() {
            let _ = child.kill();
        }
        s.alive.store(false, Ordering::SeqCst);
    }
    Ok(())
}

// ── Selection-prompt detection ────────────────────────────────────────────

const POINTERS: [char; 3] = ['\u{276F}', '>', '\u{25CF}']; // ❯  >  ●

fn is_box_char(c: char) -> bool {
    matches!(
        c,
        '│' | '─'
            | '╭'
            | '╮'
            | '╰'
            | '╯'
            | '┌'
            | '┐'
            | '└'
            | '┘'
            | '├'
            | '┤'
            | '┬'
            | '┴'
            | '┼'
            | '║'
            | '═'
            | '╔'
            | '╗'
            | '╚'
            | '╝'
            | '╠'
            | '╣'
            | '╦'
            | '╩'
            | '╬'
            | '▌'
            | '▐'
            | '█'
            | '▏'
            | '▕'
    )
}

/// Replace box-drawing decoration with spaces and trim the ends, leaving the
/// logical text (pointer + label) for detection. Interior spacing is preserved.
fn strip_box(s: &str) -> String {
    let out: String = s
        .chars()
        .map(|c| if is_box_char(c) { ' ' } else { c })
        .collect();
    out.trim().to_string()
}

fn starts_with_pointer(s: &str) -> bool {
    POINTERS.iter().any(|p| s.starts_with(*p))
}

fn strip_pointer(s: &str) -> &str {
    let s = s.trim_start_matches(|c| POINTERS.contains(&c));
    s.trim_start()
}

/// If `s` begins with "N." or "N)", return the label after it.
fn strip_number(s: &str) -> Option<&str> {
    let digits_end = s.find(|c: char| !c.is_ascii_digit())?;
    if digits_end == 0 {
        return None;
    }
    let rest = &s[digits_end..];
    let rest = rest
        .strip_prefix('.')
        .or_else(|| rest.strip_prefix(')'))?;
    Some(rest.trim_start())
}

/// A short, single-phrase line that plausibly belongs to an option list (used to
/// bound a pointer-based, un-numbered menu without swallowing the question).
fn is_optionish(line: &str) -> bool {
    let t = line.trim();
    !t.is_empty() && t.chars().count() <= 60 && !t.ends_with('?') && !t.ends_with(':')
}

/// Gather up to a few non-empty lines directly above the first option as the
/// prompt's question.
fn question_above(cleaned: &[String], first_option: usize) -> String {
    let mut collected: Vec<String> = Vec::new();
    let mut i = first_option;
    while i > 0 {
        i -= 1;
        let t = cleaned[i].trim();
        if t.is_empty() {
            if collected.is_empty() {
                continue;
            }
            break;
        }
        collected.push(t.to_string());
        if collected.len() >= 3 {
            break;
        }
    }
    collected.reverse();
    collected.join(" ").trim().to_string()
}

/// Detect a selection prompt from the rendered screen lines. Two passes:
/// numbered options first (Claude's permission menus), then a pointer-based
/// fallback for un-numbered Yes/No style prompts. Returns `None` when nothing
/// menu-like is on screen (the frontend then shows only the raw terminal).
fn detect_prompt(lines: &[String]) -> Option<DetectedPrompt> {
    let cleaned: Vec<String> = lines.iter().map(|l| strip_box(l)).collect();

    // Pass 1 — numbered options: "1. Yes", "❯ 2. No", "3) Cancel".
    let mut numbered: Vec<(usize, String, bool)> = Vec::new();
    for (i, line) in cleaned.iter().enumerate() {
        let marked = starts_with_pointer(line);
        let body = strip_pointer(line);
        if let Some(label) = strip_number(body) {
            if !label.is_empty() {
                numbered.push((i, label.to_string(), marked));
            }
        }
    }
    if numbered.len() >= 2 {
        let first_line = numbered[0].0;
        let selected = numbered.iter().position(|(_, _, m)| *m).unwrap_or(0);
        return Some(DetectedPrompt {
            question: question_above(&cleaned, first_line),
            options: numbered.into_iter().map(|(_, l, _)| l).collect(),
            selected,
        });
    }

    // Pass 2 — pointer-based, un-numbered: find the ❯ line and the run of
    // adjacent option-ish lines around it.
    let ptr = cleaned.iter().position(|l| starts_with_pointer(l))?;
    let mut start = ptr;
    while start > 0 && is_optionish(&cleaned[start - 1]) && !starts_with_pointer(&cleaned[start - 1])
    {
        start -= 1;
    }
    let mut end = ptr;
    while end + 1 < cleaned.len() && is_optionish(&cleaned[end + 1]) {
        end += 1;
    }
    let mut options = Vec::new();
    let mut selected = 0;
    for (k, i) in (start..=end).enumerate() {
        if starts_with_pointer(&cleaned[i]) {
            selected = k;
        }
        options.push(strip_pointer(&cleaned[i]).to_string());
    }
    if options.len() >= 2 {
        Some(DetectedPrompt {
            question: question_above(&cleaned, start),
            options,
            selected,
        })
    } else {
        None
    }
}
