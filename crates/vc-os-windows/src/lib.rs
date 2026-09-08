// Windows adapters for vibe-control.
// Enumerates running GUI apps and open browser tabs via Win32 / WMI / Registry.
// All calls degrade gracefully: missing APIs or closed processes yield an
// empty result rather than an error, so capture never fails hard.

#[cfg(target_os = "windows")]
use std::process::Command;
#[cfg(target_os = "windows")]
use vc_core::{CoreError, Result};

#[cfg(target_os = "windows")]
pub struct WinWindowEnumerator;

#[cfg(target_os = "windows")]
impl WinWindowEnumerator {
    /// Names of currently running foreground applications via tasklist.
    pub fn list_running() -> Result<Vec<String>> {
        let output = Command::new("tasklist.exe")
            .arg("/fo")
            .arg("csv")
            .arg("/nh")
            .output()
            .map_err(|e| CoreError::Internal(format!("tasklist failed: {e}")))?;
        if !output.status.success() {
            return Ok(vec![]);
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut names: Vec<String> = stdout
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split(',').map(|s| s.trim().trim_matches('"')).collect();
                if parts.is_empty() {
                    return None;
                }
                let name = parts[0].trim();
                if name.is_empty() || name.eq_ignore_ascii_case("name") {
                    return None;
                }
                Some(name.to_string())
            })
            .collect();
        names.sort();
        names.dedup();
        Ok(names)
    }

    /// Running foreground apps as (display name, process name). On Windows,
    /// we return (executable name, executable name) since there's no stable
    /// "bundle identifier" equivalent (AppUserModelId exists but is optional).
    pub fn list_running_apps() -> Result<Vec<(String, Option<String>)>> {
        let output = Command::new("tasklist.exe")
            .arg("/fo")
            .arg("csv")
            .arg("/nh")
            .output()
            .map_err(|e| CoreError::Internal(format!("tasklist failed: {e}")))?;
        if !output.status.success() {
            return Ok(vec![]);
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut apps: Vec<(String, Option<String>)> = stdout
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split(',').map(|s| s.trim().trim_matches('"')).collect();
                if parts.is_empty() {
                    return None;
                }
                let name = parts[0].trim();
                if name.is_empty() || name.eq_ignore_ascii_case("name") {
                    return None;
                }
                // On Windows, we use the process name (e.g. "chrome.exe") as both display
                // and "bundle id" since there's no unified app identifier like macOS.
                // Frontend will display it nicely; restore uses the same name.
                Some((name.to_string(), Some(name.to_string())))
            })
            .collect();
        apps.sort();
        apps.dedup();
        Ok(apps)
    }
}

#[cfg(target_os = "windows")]
pub struct WinBrowserTabReader;

#[cfg(target_os = "windows")]
impl WinBrowserTabReader {
    /// Open (title, url) tabs across Edge and Chrome, for whichever is running.
    /// Browsers that aren't running are skipped (never launched).
    /// NOTE: This is a best-effort implementation. Exact tab extraction on Windows
    /// requires COM (Edge/IE) or Chrome's debugging protocol (overkill for now).
    /// For MVP, we return empty — the user can manually add browser tabs if needed.
    pub fn read_tabs() -> Result<Vec<(String, String)>> {
        // Placeholder: Full implementation requires either:
        // - Edge COM Interop (IUIAutomationElement)
        // - Chrome debugging protocol (localhost:9222)
        // - Registry scraping of last-opened URLs
        // For now, return empty; tabs can be added manually in the UI or via later enhancement.
        Ok(vec![])
    }
}

#[cfg(target_os = "windows")]
pub struct WinLauncher;

#[cfg(target_os = "windows")]
impl WinLauncher {
    /// Launch (or focus) an application by name or executable.
    pub fn open_app(target: &str) -> Result<()> {
        // Try to launch via "start" command (Windows equivalent of macOS "open").
        // "start" with no args just focuses if running; with program name it launches.
        let output = Command::new("cmd.exe")
            .arg("/c")
            .arg(format!("start {}", target))
            .output()
            .map_err(|e| CoreError::Internal(format!("cmd.exe start failed: {e}")))?;
        if !output.status.success() {
            return Err(CoreError::Internal(
                format!("could not launch application '{target}'"),
            ));
        }
        Ok(())
    }

    /// Open a file or folder in its default handler / Explorer.
    pub fn open_path(path: &str) -> Result<()> {
        let output = Command::new("explorer.exe")
            .arg(path)
            .output()
            .map_err(|e| CoreError::Internal(format!("explorer.exe failed: {e}")))?;
        if !output.status.success() {
            return Err(CoreError::Internal(format!(
                "could not open path '{path}'"
            )));
        }
        Ok(())
    }

    /// Open a URL in the default browser.
    pub fn open_url(url: &str) -> Result<()> {
        let output = Command::new("cmd.exe")
            .arg("/c")
            .arg(format!("start {}", url))
            .output()
            .map_err(|e| CoreError::Internal(format!("cmd.exe start failed: {e}")))?;
        if !output.status.success() {
            return Err(CoreError::Internal(format!(
                "could not open URL '{url}'"
            )));
        }
        Ok(())
    }

    /// Open a new PowerShell / Command Prompt window and run a command in it.
    pub fn run_in_terminal(command: &str) -> Result<()> {
        // Use PowerShell to open a new window and execute the command.
        // -Command executes the command; -NoExit keeps the window open.
        let escaped = command.replace('"', "\\\"");
        let ps_cmd = format!(
            "powershell.exe -NoExit -Command \"{}\"",
            escaped
        );
        let output = Command::new("cmd.exe")
            .arg("/c")
            .arg(&ps_cmd)
            .output()
            .map_err(|e| CoreError::Internal(format!("powershell launch failed: {e}")))?;
        if !output.status.success() {
            return Err(CoreError::Internal(
                "failed to open terminal and run command".into(),
            ));
        }
        Ok(())
    }

    /// Bring the already-open PowerShell (hosting a Claude Code session) to the front.
    /// match_hint is currently unused on Windows; focus is best-effort via tasklist.
    pub fn activate_terminal(_match_hint: Option<&str>) -> Result<()> {
        // Best-effort: try to bring PowerShell to foreground.
        // This is more complex on Windows and would require Win32 SetForegroundWindow.
        // For now, attempt via PowerShell:
        let output = Command::new("powershell.exe")
            .arg("-Command")
            .arg("[System.Windows.Forms.SendKeys]::SendWait('%^~')")
            .output()
            .map_err(|e| CoreError::Internal(format!("activate_terminal failed: {e}")))?;
        if !output.status.success() {
            // Silently degrade — terminal may or may not activate, but command still ran.
            return Ok(());
        }
        Ok(())
    }
}
