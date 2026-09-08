// Windows adapters for vibe-control.
// Enumerates running GUI apps (windowed processes via `tasklist /v`) and
// launches apps / URLs / folders / coding-session terminals via PowerShell.
//
// SECURITY: every untrusted value (app target, URL, folder path, the command a
// session terminal runs, a focus hint) is passed to PowerShell through an
// ENVIRONMENT VARIABLE, never concatenated into a shell command line. PowerShell
// reads it back with `$env:...`, which is a plain string load — not re-parsed by
// a shell — so there is no command-injection surface and values containing
// spaces, `&`, `|`, quotes, etc. are handled correctly. This mirrors the macOS
// adapter's rule of passing arguments, not building shell strings.
//
// Enumeration degrades gracefully: a missing API, a non-zero exit, or a parse
// failure yields an empty result rather than an error, so capture never fails
// hard. Launch/resume return errors (surfaced in the restore report); terminal
// activation is advisory and always returns Ok.

#[cfg(target_os = "windows")]
use std::process::Command;
#[cfg(target_os = "windows")]
use vc_core::{CoreError, Result};

#[cfg(target_os = "windows")]
pub struct WinWindowEnumerator;

#[cfg(target_os = "windows")]
impl WinWindowEnumerator {
    /// Names (without the `.exe` suffix) of running apps that own a window.
    pub fn list_running() -> Result<Vec<String>> {
        Ok(Self::windowed_process_exes()
            .into_iter()
            .map(|exe| display_name(&exe))
            .collect())
    }

    /// Running windowed apps as `(display name, launch target)`. On Windows the
    /// launch target is the executable name (e.g. `chrome.exe`), which PowerShell
    /// `Start-Process` resolves via the App Paths registry / PATH. There is no
    /// stable "bundle identifier" equivalent, so the target doubles as the id.
    pub fn list_running_apps() -> Result<Vec<(String, Option<String>)>> {
        Ok(Self::windowed_process_exes()
            .into_iter()
            .map(|exe| (display_name(&exe), Some(exe)))
            .collect())
    }

    /// Executable names of processes that currently own a top-level window.
    ///
    /// We use `tasklist /v` (verbose) and keep only rows whose Window Title is a
    /// real title (not `N/A` / empty), which approximates macOS's "regular app"
    /// filter and keeps background/service processes (svchost.exe, RuntimeBroker,
    /// …) out of the list. Any failure yields an empty vec (never a hard error).
    fn windowed_process_exes() -> Vec<String> {
        let output = Command::new("tasklist.exe")
            .args(["/v", "/fo", "csv", "/nh"])
            .output();
        let output = match output {
            Ok(o) if o.status.success() => o,
            _ => return Vec::new(),
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut names: Vec<String> = stdout.lines().filter_map(parse_windowed_row).collect();
        names.sort();
        names.dedup();
        names
    }
}

/// Parse one `tasklist /v /fo csv /nh` row, returning the image name only for
/// rows that own a real window. Columns (verbose): Image Name, PID, Session
/// Name, Session#, Mem Usage, Status, User Name, CPU Time, Window Title.
#[cfg(target_os = "windows")]
fn parse_windowed_row(line: &str) -> Option<String> {
    let fields = split_tasklist_csv(line);
    if fields.len() < 9 {
        return None;
    }
    let image = fields[0].trim();
    let title = fields[8].trim();
    if image.is_empty() || image.eq_ignore_ascii_case("Image Name") {
        return None; // header or blank
    }
    // Background/service processes report "N/A" (or empty) as their window title.
    if title.is_empty() || title.eq_ignore_ascii_case("N/A") {
        return None;
    }
    Some(image.to_string())
}

/// Split a `tasklist /fo csv` line whose every field is wrapped in double
/// quotes. Splitting on the literal `","` delimiter (then stripping the leading
/// and trailing quote) preserves commas *inside* a field — critical because the
/// Mem Usage column is formatted like `"12,345 K"`, which a naive `split(',')`
/// would shred, misaligning every later column.
#[cfg(target_os = "windows")]
fn split_tasklist_csv(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    let inner = trimmed
        .strip_prefix('"')
        .unwrap_or(trimmed)
        .strip_suffix('"')
        .unwrap_or(trimmed);
    inner.split("\",\"").map(|s| s.to_string()).collect()
}

/// Drop a trailing `.exe` (case-insensitive) for a friendlier display name;
/// the launch target keeps the full executable name.
#[cfg(target_os = "windows")]
fn display_name(exe: &str) -> String {
    let lower = exe.to_ascii_lowercase();
    if let Some(stripped) = lower.strip_suffix(".exe") {
        exe[..stripped.len()].to_string()
    } else {
        exe.to_string()
    }
}

#[cfg(target_os = "windows")]
pub struct WinBrowserTabReader;

#[cfg(target_os = "windows")]
impl WinBrowserTabReader {
    /// Open browser tabs as `(title, url)`. Not yet implemented on Windows.
    ///
    /// Reliable tab extraction needs either UI Automation (Edge/IE COM) or the
    /// Chrome DevTools protocol (localhost:9222, off by default) — both too heavy
    /// for the MVP. We return empty so capture still succeeds; users add tabs
    /// manually. Returning `Ok` (never launching a browser) is intentional.
    pub fn read_tabs() -> Result<Vec<(String, String)>> {
        Ok(Vec::new())
    }
}

#[cfg(target_os = "windows")]
pub struct WinLauncher;

#[cfg(target_os = "windows")]
impl WinLauncher {
    /// Launch (or focus) an application by executable name or path.
    pub fn open_app(target: &str) -> Result<()> {
        Self::start_process(target)
    }

    /// Open a file or folder in Explorer / its default handler.
    pub fn open_path(path: &str) -> Result<()> {
        Self::start_process(path)
    }

    /// Open a URL in the default browser.
    pub fn open_url(url: &str) -> Result<()> {
        Self::start_process(url)
    }

    /// Launch a target via PowerShell `Start-Process`, passing the (untrusted)
    /// target through an env var so it is never parsed by a shell. One helper
    /// serves apps, URLs and paths: `Start-Process` routes each appropriately
    /// (App Paths for `foo.exe`, default browser for a URL, Explorer for a
    /// folder). `-ErrorAction Stop` makes a launch failure a terminating error
    /// so PowerShell exits non-zero and we can report it.
    fn start_process(target: &str) -> Result<()> {
        let output = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Process -FilePath $env:VC_TARGET -ErrorAction Stop",
            ])
            .env("VC_TARGET", target)
            .output()
            .map_err(|e| CoreError::Internal(format!("powershell Start-Process failed: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(CoreError::Internal(format!(
                "could not launch '{target}': {}",
                stderr.trim()
            )));
        }
        Ok(())
    }

    /// Open a NEW PowerShell window (kept open with `-NoExit`) and run `command`
    /// inside it.
    ///
    /// `command` is passed via an env var and executed in the new window with
    /// `Invoke-Expression` — never placed on a command line, so no injection and
    /// no quote breakage. We launch the window with an OUTER PowerShell whose
    /// `Start-Process` does NOT wait, so this call returns immediately. (The
    /// previous implementation ran `powershell -NoExit` under `.output()`, which
    /// blocked until the user closed the window — an effective deadlock.)
    pub fn run_in_terminal(command: &str) -> Result<()> {
        let output = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                // Start-Process (no -Wait) spawns a visible new window and returns.
                "Start-Process powershell -ArgumentList \
                 '-NoExit','-NoProfile','-Command','Invoke-Expression $env:VC_CMD'",
            ])
            .env("VC_CMD", command)
            .output()
            .map_err(|e| CoreError::Internal(format!("failed to open terminal: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(CoreError::Internal(format!(
                "failed to open terminal and run command: {}",
                stderr.trim()
            )));
        }
        Ok(())
    }

    /// Bring an already-open terminal window hosting a Claude Code session to the
    /// front, best-effort. Matches the window whose title contains `match_hint`
    /// (the session's working-dir name) via `WScript.Shell.AppActivate` — which
    /// is available on every Windows install with no extra assemblies, unlike the
    /// previous `System.Windows.Forms.SendKeys` call (that class isn't loaded by
    /// default, so the old code silently did nothing). True Win32
    /// `SetForegroundWindow` would need a native crate; this is the
    /// no-new-dependency MVP. Always returns Ok — activation is advisory (the
    /// window may not exist).
    pub fn activate_terminal(match_hint: Option<&str>) -> Result<()> {
        let hint = match_hint
            .map(str::trim)
            .filter(|h| !h.is_empty())
            .unwrap_or("PowerShell");
        // Ignore the result: the window may not be open, and that's fine.
        let _ = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "$ws = New-Object -ComObject WScript.Shell; \
                 [void]$ws.AppActivate($env:VC_HINT)",
            ])
            .env("VC_HINT", hint)
            .output();
        Ok(())
    }
}
