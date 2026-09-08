// Windows adapters for vibe-control.
// Enumerates running GUI apps (processes owning a visible main window, via
// PowerShell `Get-Process`) and launches apps / URLs / folders / coding-session
// terminals via PowerShell. Launching an app first tries to FOCUS an
// already-running instance's window (so double-click brings the existing window
// to the front instead of opening a duplicate) and only starts a new process
// when none is running — see `WinLauncher::open_app`.
//
// WHY NOT `tasklist /v`: verbose `tasklist` fetches each process's window title
// by pumping messages to the window, so a single unresponsive app makes the
// whole call hang for minutes. Because enumeration runs on the app's 1s poll
// loop, that froze the UI ("not responding") and left the running-apps list
// empty. `Get-Process` reads `MainWindowTitle` from the process table without
// messaging any window, so it never hangs and returns in ~1s.
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
use std::os::windows::process::CommandExt;
#[cfg(target_os = "windows")]
use std::process::Command;
#[cfg(target_os = "windows")]
use vc_core::{CoreError, Result};

/// `CREATE_NO_WINDOW` process-creation flag. Without it, every short-lived
/// `powershell.exe` we spawn allocates and briefly shows a console window — and
/// because app enumeration polls once a second, that console would flash on
/// screen continuously (stealing focus). Applied to the OUTER helper process
/// only; `run_in_terminal`'s inner `Start-Process powershell` still opens its
/// own intended visible window.
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(target_os = "windows")]
pub struct WinWindowEnumerator;

#[cfg(target_os = "windows")]
impl WinWindowEnumerator {
    /// Names of running apps that own a visible main window.
    pub fn list_running() -> Result<Vec<String>> {
        Ok(Self::windowed_processes()
            .into_iter()
            .map(|(name, _target)| name)
            .collect())
    }

    /// Running windowed apps as `(display name, launch target)`. The launch
    /// target is the full executable path when readable, else the process name
    /// plus `.exe` (which PowerShell `Start-Process` resolves via the App Paths
    /// registry / PATH). There is no stable "bundle identifier" equivalent on
    /// Windows, so the target doubles as the id.
    pub fn list_running_apps() -> Result<Vec<(String, Option<String>)>> {
        Ok(Self::windowed_processes()
            .into_iter()
            .map(|(name, target)| (name, Some(target)))
            .collect())
    }

    /// Processes that currently own a visible main window, as `(process name,
    /// launch target)`.
    ///
    /// Uses PowerShell `Get-Process`, keeping only processes whose
    /// `MainWindowTitle` is non-empty — the "has a real window" signal, read
    /// straight from the process table. Unlike `tasklist /v` this never messages
    /// the windows, so an unresponsive app can't hang the call. The query emits
    /// one `name|path` line per process (`path` is empty when the executable
    /// path isn't readable); it takes no untrusted input, so it is a fixed
    /// literal. Any failure yields an empty vec (never a hard error).
    fn windowed_processes() -> Vec<(String, String)> {
        const QUERY: &str = "Get-Process | Where-Object { $_.MainWindowTitle -ne '' } | \
             Select-Object ProcessName,Path -Unique | \
             ForEach-Object { \"$($_.ProcessName)|$($_.Path)\" }";
        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", QUERY])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        let output = match output {
            Ok(o) if o.status.success() => o,
            _ => return Vec::new(),
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut apps: Vec<(String, String)> = stdout.lines().filter_map(parse_process_line).collect();
        apps.sort();
        apps.dedup();
        apps
    }
}

/// Parse one `name|path` enumeration line into `(display name, launch target)`.
/// The target is the executable path when present, else `name.exe` (App Paths /
/// PATH resolves it at launch). Blank/nameless lines are skipped.
#[cfg(target_os = "windows")]
fn parse_process_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let (name, path) = line.split_once('|').unwrap_or((line, ""));
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let path = path.trim();
    let target = if path.is_empty() {
        format!("{name}.exe")
    } else {
        path.to_string()
    };
    Some((name.to_string(), target))
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
    /// Launch an application, OR — if a windowed instance is already running —
    /// bring that existing window to the front instead of starting a second
    /// copy (FR-2.6 / §13.4: "bring to front, launching it if closed").
    ///
    /// WHY THIS EXISTS: `Start-Process` unconditionally spawns a NEW process, so
    /// double-clicking a running app used to open a duplicate window. Unlike
    /// macOS `open` (which naturally re-activates a running app), Windows has no
    /// single call for "focus if running, else launch", so we do it in two
    /// steps: try to focus an existing window first, and only launch when none
    /// is found.
    pub fn open_app(target: &str) -> Result<()> {
        if Self::focus_existing_window(target) {
            return Ok(());
        }
        Self::start_process(target)
    }

    /// Focus an already-running instance's main window. Returns `true` when a
    /// matching windowed process was found (so the caller must NOT launch a
    /// duplicate), `false` when none is running.
    ///
    /// A process matches by executable path (what enumeration hands back as the
    /// launch target) or, as a fallback, by process name, and must own a real
    /// main window (`MainWindowHandle != 0`). We restore (un-minimize) and
    /// foreground it via Win32 `ShowWindowAsync` + `SetForegroundWindow` —
    /// compiled inline with `Add-Type`, present on every Windows install, so no
    /// native crate is needed — reinforced by `WScript.Shell.AppActivate`
    /// (which Windows treats permissively for activation). The focus calls are
    /// best-effort and wrapped in a `try`: once a windowed instance is found we
    /// report `ACTIVATED` regardless, because "an instance exists" is what
    /// decides not to launch a duplicate; a failed focus just leaves the
    /// existing window where it was rather than spawning a new one. The target
    /// is passed via an env var, never the command line (see module SECURITY).
    fn focus_existing_window(target: &str) -> bool {
        const SCRIPT: &str = "\
$target = $env:VC_TARGET; \
$base = [System.IO.Path]::GetFileNameWithoutExtension($target); \
$proc = Get-Process | Where-Object { $_.MainWindowHandle -ne 0 -and ($_.ProcessName -eq $base -or ($(try { $_.Path } catch { $null }) -eq $target)) } | Select-Object -First 1; \
if ($proc) { \
    try { \
        Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public static class VcWin { [DllImport(\"user32.dll\")] public static extern bool SetForegroundWindow(IntPtr h); [DllImport(\"user32.dll\")] public static extern bool ShowWindowAsync(IntPtr h, int n); }'; \
        [void][VcWin]::ShowWindowAsync($proc.MainWindowHandle, 9); \
        $ws = New-Object -ComObject WScript.Shell; \
        [void]$ws.AppActivate($proc.Id); \
        [void][VcWin]::SetForegroundWindow($proc.MainWindowHandle); \
    } catch {} \
    'ACTIVATED'; \
} else { 'NOTFOUND'; }";
        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
            .env("VC_TARGET", target)
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        match output {
            Ok(o) => String::from_utf8_lossy(&o.stdout).contains("ACTIVATED"),
            // Couldn't even run PowerShell — fall back to launching.
            Err(_) => false,
        }
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
            .creation_flags(CREATE_NO_WINDOW)
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
            .creation_flags(CREATE_NO_WINDOW)
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
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        Ok(())
    }
}
