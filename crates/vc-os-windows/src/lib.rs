// Windows adapters for vibe-control
// WindowEnumerator, WindowActivator, BrowserTabReader via UI Automation

#[cfg(target_os = "windows")]
use vc_core::Result;

#[cfg(target_os = "windows")]
pub struct WinWindowEnumerator;

#[cfg(target_os = "windows")]
impl WinWindowEnumerator {
    pub fn list_running() -> Result<Vec<String>> {
        // Placeholder: Use Windows UI Automation or EnumWindows
        Ok(vec![])
    }
}

#[cfg(target_os = "windows")]
pub struct WinBrowserTabReader;

#[cfg(target_os = "windows")]
impl WinBrowserTabReader {
    pub fn read_tabs() -> Result<Vec<(String, String)>> {
        // Placeholder: Edge + Chrome tab enumeration
        Ok(vec![])
    }
}
