// Windows adapters for vibe-control.
// Enumerates user-launched GUI apps (processes owning a visible top-level
// window, via `Get-Process`), extracts their icons, and launches apps / URLs /
// folders / coding-session terminals via PowerShell.
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
/// `powershell.exe` we spawn allocates and briefly shows a
/// console window — and because app enumeration polls once a second, that
/// console flashes on screen continuously. Applied to the OUTER helper process
/// only; `run_in_terminal`'s inner `Start-Process powershell` still opens its
/// own intended visible window.
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(target_os = "windows")]
pub struct WinWindowEnumerator;

/// PowerShell one-liner that lists only the apps the *user* launched: processes
/// that currently own a VISIBLE top-level window. `MainWindowHandle != 0`
/// (paired with a non-empty `MainWindowTitle`) is the signal that a real,
/// on-screen window exists — which excludes background services and the
/// invisible helper windows (message-only / notification sinks like
/// `OleMainThreadWndName`, `ATKOSD2`, `AMD:DVR-CapturingWindow`, …) that made
/// the old `tasklist /v` window-title heuristic leak dozens of background
/// agents into the list. Each match is emitted as `ProcessName<TAB>FullPath`
/// (the path is empty when it can't be read, e.g. an elevated process).
#[cfg(target_os = "windows")]
const LIST_APPS_SCRIPT: &str = "$ErrorActionPreference='SilentlyContinue'; \
Get-Process | Where-Object { $_.MainWindowHandle -ne 0 -and $_.MainWindowTitle } | \
ForEach-Object { $p=''; try { $p=$_.Path } catch {}; ($_.ProcessName + [char]9 + $p) }";

#[cfg(target_os = "windows")]
impl WinWindowEnumerator {
    /// Names of user-launched apps that currently own a visible window.
    pub fn list_running() -> Result<Vec<String>> {
        Ok(Self::visible_window_apps()
            .into_iter()
            .map(|(name, _)| name)
            .collect())
    }

    /// User-launched windowed apps as `(display name, launch target)`. The
    /// launch target is the full executable path when we can read it (which
    /// makes both re-launch via `Start-Process` and icon extraction reliable),
    /// falling back to the bare `name.exe` — which `Start-Process` resolves via
    /// the App Paths registry / PATH — when the path is unavailable. There is no
    /// stable "bundle identifier" on Windows, so the target doubles as the id.
    pub fn list_running_apps() -> Result<Vec<(String, Option<String>)>> {
        Ok(Self::visible_window_apps()
            .into_iter()
            .map(|(name, target)| (name, Some(target)))
            .collect())
    }

    /// Enumerate visible-window processes as `(display name, launch target)`,
    /// de-duplicated by name (an app with several windowed processes — e.g.
    /// Chrome — appears once) and sorted case-insensitively for a stable list.
    /// Any failure (missing PowerShell, non-zero exit) yields an empty vec so
    /// capture never fails hard.
    fn visible_window_apps() -> Vec<(String, String)> {
        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", LIST_APPS_SCRIPT])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        let output = match output {
            Ok(o) if o.status.success() => o,
            _ => return Vec::new(),
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut seen = std::collections::HashSet::new();
        let mut apps: Vec<(String, String)> = Vec::new();
        for line in stdout.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let mut parts = line.splitn(2, '\t');
            let name = parts.next().unwrap_or("").trim();
            let path = parts.next().unwrap_or("").trim();
            // Skip blanks and collapse repeats (dedup on lowercased name).
            if name.is_empty() || !seen.insert(name.to_ascii_lowercase()) {
                continue;
            }
            let target = if path.is_empty() {
                format!("{name}.exe")
            } else {
                path.to_string()
            };
            apps.push((name.to_string(), target));
        }
        apps.sort_by(|a, b| a.0.to_ascii_lowercase().cmp(&b.0.to_ascii_lowercase()));
        apps
    }
}

#[cfg(target_os = "windows")]
pub struct WinIconReader;

/// PowerShell + a small C# helper (compiled on demand via `Add-Type`) that
/// extracts an executable's shell icon and returns it as base64 PNG. The
/// untrusted target is read from `$env:VC_ICON_TARGET` — never the command line
/// (see the module SECURITY note). It first resolves the target to a real file
/// path (using it directly when it already is one, else looking up a running
/// process of that name), then pulls the 256px "jumbo" shell icon, degrading to
/// the 48px "extra-large" list icon and finally the 32px associated icon so
/// every app yields *something*. Emits nothing (and exits non-zero) on failure.
#[cfg(target_os = "windows")]
const ICON_SCRIPT: &str = r#"$ErrorActionPreference='SilentlyContinue'
$t = $env:VC_ICON_TARGET
if (-not (Test-Path -LiteralPath $t -PathType Leaf)) {
  $base = [System.IO.Path]::GetFileNameWithoutExtension($t)
  $proc = Get-Process -Name $base -ErrorAction SilentlyContinue | Where-Object { $_.Path } | Select-Object -First 1
  if ($proc) { $t = $proc.Path }
}
if (-not (Test-Path -LiteralPath $t -PathType Leaf)) { exit 1 }
Add-Type -ReferencedAssemblies System.Drawing @"
using System;
using System.Drawing;
using System.Runtime.InteropServices;
public static class IconX {
  [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Auto)]
  struct SHFILEINFO { public IntPtr hIcon; public int iIcon; public uint dwAttributes;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst=260)] public string szDisplayName;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst=80)]  public string szTypeName; }
  [DllImport("shell32.dll", CharSet=CharSet.Auto)]
  static extern IntPtr SHGetFileInfo(string pszPath, uint attrs, ref SHFILEINFO psfi, uint cb, uint flags);
  [DllImport("shell32.dll")]
  static extern int SHGetImageList(int iImageList, ref Guid riid, out IImageList ppv);
  [DllImport("user32.dll")]
  static extern bool DestroyIcon(IntPtr hIcon);
  [ComImport, Guid("46EB5926-582E-4017-9FDF-E8998DAA0950"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
  interface IImageList {
    int Add(IntPtr i, IntPtr m, ref int pi);
    int ReplaceIcon(int i, IntPtr icon, ref int pi);
    int SetOverlayImage(int iImage, int iOverlay);
    int Replace(int i, IntPtr image, IntPtr mask);
    int AddMasked(IntPtr image, int mask, ref int pi);
    int Draw(IntPtr pimldp);
    int Remove(int i);
    int GetIcon(int i, int flags, ref IntPtr picon);
  }
  const uint SHGFI_SYSICONINDEX = 0x4000;
  const int ILD_TRANSPARENT = 0x1;
  static Bitmap FromList(string path, int shil) {
    var fi = new SHFILEINFO();
    SHGetFileInfo(path, 0, ref fi, (uint)Marshal.SizeOf(fi), SHGFI_SYSICONINDEX);
    var guid = new Guid("46EB5926-582E-4017-9FDF-E8998DAA0950");
    IImageList list;
    if (SHGetImageList(shil, ref guid, out list) != 0 || list == null) return null;
    IntPtr hicon = IntPtr.Zero;
    if (list.GetIcon(fi.iIcon, ILD_TRANSPARENT, ref hicon) != 0 || hicon == IntPtr.Zero) return null;
    try { using (var ico = Icon.FromHandle(hicon)) { return ico.ToBitmap(); } }
    finally { DestroyIcon(hicon); }
  }
  // Crop fully-transparent margins. The jumbo (256px) image list parks a small
  // glyph in the TOP-LEFT of an otherwise-empty 256 canvas for apps that ship
  // no 256px icon (many WinUI3 apps, e.g. PowerToys); trimming makes that glyph
  // fill the UI box instead of hugging the corner. A real 256px icon has no
  // transparent margin and is returned unchanged. Null if wholly transparent.
  static Bitmap Trim(Bitmap b) {
    int minX=b.Width, minY=b.Height, maxX=-1, maxY=-1;
    for (int y=0; y<b.Height; y++)
      for (int x=0; x<b.Width; x++)
        if (b.GetPixel(x,y).A > 0) {
          if (x<minX) minX=x; if (x>maxX) maxX=x;
          if (y<minY) minY=y; if (y>maxY) maxY=y;
        }
    if (maxX < minX) return null;
    var rect = new Rectangle(minX, minY, maxX-minX+1, maxY-minY+1);
    if (rect.Width==b.Width && rect.Height==b.Height) return b;
    var outb = b.Clone(rect, b.PixelFormat);
    b.Dispose();
    return outb;
  }
  public static string PngBase64(string path) {
    Bitmap bmp = null;
    try { bmp = FromList(path, 0x4); } catch {}          // SHIL_JUMBO (256px)
    if (bmp == null) { try { bmp = FromList(path, 0x2); } catch {} } // SHIL_EXTRALARGE (48px)
    if (bmp == null) {
      var ico = Icon.ExtractAssociatedIcon(path);
      if (ico == null) return null;
      bmp = ico.ToBitmap();
    }
    bmp = Trim(bmp);
    if (bmp == null) return null;
    using (var ms = new System.IO.MemoryStream()) {
      bmp.Save(ms, System.Drawing.Imaging.ImageFormat.Png);
      bmp.Dispose();
      return Convert.ToBase64String(ms.ToArray());
    }
  }
}
"@
$b64 = [IconX]::PngBase64($t)
if ([string]::IsNullOrEmpty($b64)) { exit 1 }
Write-Output $b64"#;

#[cfg(target_os = "windows")]
impl WinIconReader {
    /// An app's icon as a ready-to-use `data:image/png;base64,…` URI, or `None`
    /// when the executable / its icon can't be found. `target` is the value
    /// `list_running_apps` produced: a full exe path (used directly) or a bare
    /// `name.exe` (resolved to a running process's path).
    pub fn icon_data_uri(target: &str) -> Option<String> {
        let b64 = Self::icon_png_base64(target)?;
        Some(format!("data:image/png;base64,{b64}"))
    }

    /// Run `ICON_SCRIPT` in Windows PowerShell (`powershell.exe` ships
    /// System.Drawing via .NET Framework) and return the raw base64 PNG, or
    /// `None` on any failure. The target is passed through an env var.
    fn icon_png_base64(target: &str) -> Option<String> {
        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", ICON_SCRIPT])
            .env("VC_ICON_TARGET", target)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let b64 = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if b64.is_empty() {
            None
        } else {
            Some(b64)
        }
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
