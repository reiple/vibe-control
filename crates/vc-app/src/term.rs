// Session-coupled EXTERNAL terminals (session == terminal).
//
// Each Claude Code session the app shows is bound 1:1 to a real, VISIBLE
// PowerShell window running interactive `claude` (the terminal the user sees on
// "resume"). The user types directly in that window; the app ALSO injects input
// into the SAME window (focus + SendKeys), so both surfaces drive one shared
// `claude` session and both see the result (the app reflects it by reading the
// session `.jsonl`). We track each window by PID so the lifecycle is coupled:
//   * remove the session in the app  -> kill the terminal window,
//   * the user closes the terminal   -> the session disappears from the app.
//
// All local: the window runs the user's own `claude` CLI; no Bedrock token and
// no external send are involved (§12 preserved).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Process-lifetime registry: session ref -> the PID of its terminal window.
fn registry() -> &'static Mutex<HashMap<String, u32>> {
    static REG: OnceLock<Mutex<HashMap<String, u32>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Open (or reuse+focus) the terminal for `session_ref`, running `command` in a
/// fresh PowerShell window. Reuse: if a tracked window is still alive we focus it
/// and return its PID instead of spawning a duplicate. Returns the window PID.
pub fn open(session_ref: &str, command: &str) -> Result<u32, String> {
    // Reuse a still-open window rather than spawning a duplicate.
    if let Some(pid) = pid_of(session_ref) {
        if is_alive(pid) {
            let _ = focus(pid);
            return Ok(pid);
        }
        // Stale entry (window was closed) — drop it and open a new one.
        forget(session_ref);
    }

    let pid = spawn(command)?;
    registry()
        .lock()
        .map_err(|_| "터미널 레지스트리 잠금 실패".to_string())?
        .insert(session_ref.to_string(), pid);
    Ok(pid)
}

/// Send a line of text into the session's terminal and submit it (Enter), so an
/// app-side send lands in the same interactive `claude` the user drives.
pub fn send_line(session_ref: &str, text: &str) -> Result<(), String> {
    let pid = pid_of(session_ref).ok_or_else(|| "열린 터미널이 없습니다".to_string())?;
    if !is_alive(pid) {
        forget(session_ref);
        return Err("터미널이 닫혀 있습니다".to_string());
    }
    let sendkeys = format!("{}{{ENTER}}", escape_sendkeys(text));
    send_keys(pid, &sendkeys)
}

/// Close (kill) the session's terminal window and forget it. Called when the
/// session is removed in the app.
pub fn close(session_ref: &str) {
    if let Some(pid) = pid_of(session_ref) {
        let _ = kill(pid);
    }
    forget(session_ref);
}

/// Whether this session currently has an open (alive) terminal.
pub fn is_open(session_ref: &str) -> bool {
    match pid_of(session_ref) {
        Some(pid) => is_alive(pid),
        None => false,
    }
}

/// The session refs whose terminals are still alive, pruning any that the user
/// has closed (so the caller can drop those sessions). Returns the survivors.
pub fn open_refs() -> Vec<String> {
    let snapshot: Vec<(String, u32)> = match registry().lock() {
        Ok(reg) => reg.iter().map(|(k, v)| (k.clone(), *v)).collect(),
        Err(_) => return Vec::new(),
    };
    let mut alive = Vec::new();
    for (ref_, pid) in snapshot {
        if is_alive(pid) {
            alive.push(ref_);
        } else {
            forget(&ref_);
        }
    }
    alive
}

/// Kill every tracked terminal and clear the registry. Called on app exit so a
/// session terminal never orphans across a restart.
pub fn stop_all() {
    let pids: Vec<u32> = match registry().lock() {
        Ok(mut reg) => reg.drain().map(|(_, v)| v).collect(),
        Err(_) => return,
    };
    for pid in pids {
        let _ = kill(pid);
    }
}

// ── internals ─────────────────────────────────────────────────────────────

fn pid_of(session_ref: &str) -> Option<u32> {
    registry().lock().ok().and_then(|r| r.get(session_ref).copied())
}

fn forget(session_ref: &str) {
    if let Ok(mut reg) = registry().lock() {
        reg.remove(session_ref);
    }
}

/// Escape SendKeys metacharacters (`+ ^ % ~ ( ) { } [ ]`) so the text is typed
/// literally, and flatten newlines to spaces (Enter submits the whole line).
fn escape_sendkeys(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '{' | '}' | '(' | ')' | '[' | ']' | '+' | '^' | '%' | '~' => {
                out.push('{');
                out.push(c);
                out.push('}');
            }
            '\r' | '\n' => out.push(' '),
            other => out.push(other),
        }
    }
    out
}

#[cfg(target_os = "windows")]
fn spawn(command: &str) -> Result<u32, String> {
    vc_os_windows::WinLauncher::run_in_terminal(command).map_err(|e| e.to_string())
}
#[cfg(target_os = "windows")]
fn send_keys(pid: u32, sendkeys: &str) -> Result<(), String> {
    vc_os_windows::WinLauncher::send_keys_to_pid(pid, sendkeys).map_err(|e| e.to_string())
}
#[cfg(target_os = "windows")]
fn is_alive(pid: u32) -> bool {
    vc_os_windows::WinLauncher::is_pid_alive(pid)
}
#[cfg(target_os = "windows")]
fn kill(pid: u32) -> Result<(), String> {
    vc_os_windows::WinLauncher::kill_pid_tree(pid).map_err(|e| e.to_string())
}
#[cfg(target_os = "windows")]
fn focus(pid: u32) -> Result<(), String> {
    vc_os_windows::WinLauncher::focus_pid(pid).map_err(|e| e.to_string())
}

// Non-Windows fallback so the crate still builds elsewhere. The app ships on
// Windows; a macOS terminal-coupling adapter would mirror these against
// `vc_os_macos` (Terminal.app / iTerm) but is out of scope here.
#[cfg(not(target_os = "windows"))]
fn spawn(_command: &str) -> Result<u32, String> {
    Err("session terminals are only supported on Windows".to_string())
}
#[cfg(not(target_os = "windows"))]
fn send_keys(_pid: u32, _sendkeys: &str) -> Result<(), String> {
    Err("session terminals are only supported on Windows".to_string())
}
#[cfg(not(target_os = "windows"))]
fn is_alive(_pid: u32) -> bool {
    false
}
#[cfg(not(target_os = "windows"))]
fn kill(_pid: u32) -> Result<(), String> {
    Ok(())
}
#[cfg(not(target_os = "windows"))]
fn focus(_pid: u32) -> Result<(), String> {
    Ok(())
}
