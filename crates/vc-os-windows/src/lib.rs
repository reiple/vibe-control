// Windows adapters for vibe-control.
// Enumerates every real top-level WINDOW (via Win32 `EnumWindows`), extracts app
// icons, and launches apps / URLs / folders / coding-session terminals via
// PowerShell. Launching an app first tries to FOCUS an already-running
// instance's window (so double-click brings the existing window to the front
// instead of opening a duplicate) and only starts a new process when none is
// running — see `WinLauncher::open_app`.
//
// WHY `EnumWindows` (native FFI), not `tasklist /v` or `Get-Process`:
//   * `tasklist /v` fetches each window title by pumping messages to the window,
//     so one unresponsive app hangs the whole call for minutes — and enumeration
//     runs on the app's 1-second poll, which froze the UI ("not responding").
//   * `Get-Process` / `MainWindowTitle` avoided that hang but exposes only ONE
//     window per process, collapsing every multi-window app (Edge/Chrome
//     windows, KakaoTalk main+chat) into a single row — a real, reproducible
//     defect (see `known-deviations.md#G1`).
// We now walk ALL top-level windows with `EnumWindows` and read their attributes
// (visible, titled, unowned, non-tool, non-cloaked) directly — no message
// pumping (so no hang) and one row PER WINDOW. It is compiled-in FFI to
// user32/dwmapi/kernel32, NOT inline PowerShell `Add-Type`, so the 1-second poll
// pays no `csc` recompile (~150-400 ms each).
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
/// `powershell.exe` we spawn (icon extraction, launch, focus) allocates and
/// briefly shows a console window — and because the app fetches icons and
/// re-checks state around a 1-second poll, that console would flash on screen
/// continuously (stealing focus). Applied to the OUTER helper process only;
/// `run_in_terminal`'s inner `Start-Process powershell` still opens its own
/// intended visible window. (Window enumeration itself no longer spawns any
/// process — it is native FFI.)
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Coarse, machine-level Claude Code liveness probe (U2 FR-2.5 / NFR-3.3).
///
/// Read-only: shells out to `tasklist` (CSV, no header) and checks whether any
/// running image name contains `claude`. Returns `Some(true)`/`Some(false)` on
/// a successful query, and `None` when the probe itself fails so the caller can
/// degrade that session to `Unknown` rather than falsely reporting `Inactive`.
/// Uses `CREATE_NO_WINDOW` so the short-lived query never flashes a console
/// window on the 1s status poll (same rationale as enumeration). Intentionally
/// coarse (any `claude` process, not a per-session PID) — see the macOS
/// counterpart for the design rationale (FD-Q1=A / FD-Q2=A).
#[cfg(target_os = "windows")]
pub fn claude_process_running() -> Option<bool> {
    let output = Command::new("tasklist.exe")
        .args(["/FO", "CSV", "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).to_lowercase();
    Some(stdout.lines().any(|line| line.contains("claude")))
}

#[cfg(target_os = "windows")]
pub struct WinWindowEnumerator;

/// Win32 FFI used by window enumeration. Declared inline (no `windows`/`winapi`
/// crate) — the surface is a handful of long-stable user32/dwmapi/kernel32
/// entry points, and enumeration runs on the app's 1-second poll, so we avoid
/// both a heavy dependency and the per-poll `csc` recompile an inline PowerShell
/// `Add-Type` would pay. All items are `#[cfg(target_os = "windows")]`, so the
/// `#[link]` attributes are never emitted when the crate is built on macOS.
/// See `construction/vc-os-windows/functional-design/window-enumeration.md §2.1`.
#[cfg(target_os = "windows")]
mod winffi {
    use std::os::raw::c_void;

    pub type Hwnd = *mut c_void;
    pub type Handle = *mut c_void;
    /// `EnumWindows` callback: `(hwnd, lparam) -> continue?` (non-zero = keep
    /// going). `lparam` carries a `*mut WinAccum` we thread through.
    pub type EnumProc = unsafe extern "system" fn(Hwnd, isize) -> i32;

    /// `GetWindow(hwnd, GW_OWNER)` → the window's owner (NULL = true top-level).
    pub const GW_OWNER: u32 = 4;
    /// `GetWindowLongW` index for the extended-style word.
    pub const GWL_EXSTYLE: i32 = -20;
    /// Tool windows (floating palettes) never appear in the taskbar / Alt-Tab.
    pub const WS_EX_TOOLWINDOW: i32 = 0x0000_0080;
    /// `DwmGetWindowAttribute(DWMWA_CLOAKED)`: non-zero when the window exists
    /// but is hidden by the shell (suspended UWP app, another virtual desktop).
    pub const DWMWA_CLOAKED: u32 = 14;
    /// `OpenProcess` right that succeeds across integrity levels for the basic
    /// queries `QueryFullProcessImageNameW` needs.
    pub const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x0000_1000;
    /// `CreateToolhelp32Snapshot` flag: snapshot the process list.
    pub const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;

    /// `PROCESSENTRY32W` (tlhelp32.h). Only `th32_process_id` and `sz_exe_file`
    /// are read; the rest exist to match the C ABI layout for `Process32*W`.
    #[repr(C)]
    #[allow(dead_code)]
    pub struct ProcessEntry32W {
        pub dw_size: u32,
        pub cnt_usage: u32,
        pub th32_process_id: u32,
        pub th32_default_heap_id: usize,
        pub th32_module_id: u32,
        pub cnt_threads: u32,
        pub th32_parent_process_id: u32,
        pub pc_pri_class_base: i32,
        pub dw_flags: u32,
        pub sz_exe_file: [u16; 260],
    }

    #[link(name = "user32")]
    extern "system" {
        pub fn EnumWindows(cb: EnumProc, lparam: isize) -> i32;
        pub fn IsWindowVisible(h: Hwnd) -> i32;
        pub fn GetWindowTextLengthW(h: Hwnd) -> i32;
        pub fn GetWindowTextW(h: Hwnd, buf: *mut u16, max: i32) -> i32;
        pub fn GetWindow(h: Hwnd, cmd: u32) -> Hwnd;
        pub fn GetWindowLongW(h: Hwnd, index: i32) -> i32;
        pub fn GetWindowThreadProcessId(h: Hwnd, pid: *mut u32) -> u32;
        pub fn GetForegroundWindow() -> Hwnd;
    }

    #[link(name = "dwmapi")]
    extern "system" {
        pub fn DwmGetWindowAttribute(h: Hwnd, attr: u32, val: *mut c_void, size: u32) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        pub fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
        pub fn CloseHandle(h: Handle) -> i32;
        pub fn QueryFullProcessImageNameW(
            proc_: Handle,
            flags: u32,
            buf: *mut u16,
            size: *mut u32,
        ) -> i32;
        pub fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> Handle;
        pub fn Process32FirstW(snap: Handle, entry: *mut ProcessEntry32W) -> i32;
        pub fn Process32NextW(snap: Handle, entry: *mut ProcessEntry32W) -> i32;
    }
}

/// Accumulator threaded through `EnumWindows` via its `lparam`: the current
/// foreground window (for the focused flag) plus one collected row per window.
#[cfg(target_os = "windows")]
struct WinAccum {
    foreground: winffi::Hwnd,
    /// `(hwnd string, pid, is_focused, title)` — exe path is resolved later.
    rows: Vec<(String, u32, bool, String)>,
}

/// `EnumWindows` callback. Keeps only real, user-facing top-level windows and
/// records each as its OWN row, so several windows of one process (Edge/Chrome
/// windows, KakaoTalk main+chat) are all captured — the whole point of moving
/// off `MainWindowHandle`. The filter (visible + titled + unowned + non-tool +
/// non-cloaked) is the one validated in `diag_windows.ps1` / `#G1`, which
/// correctly excludes e.g. explorer's Program Manager while keeping every real
/// browser window.
///
/// # Safety
/// Called by `EnumWindows`; `lparam` must be the `*mut WinAccum` we pass in.
/// The body uses only non-panicking operations so it never unwinds across the
/// FFI boundary.
#[cfg(target_os = "windows")]
unsafe extern "system" fn enum_windows_cb(hwnd: winffi::Hwnd, lparam: isize) -> i32 {
    let accum = &mut *(lparam as *mut WinAccum);

    if winffi::IsWindowVisible(hwnd) == 0 {
        return 1;
    }
    let len = winffi::GetWindowTextLengthW(hwnd);
    if len <= 0 {
        return 1; // no title → not a user window
    }
    if !winffi::GetWindow(hwnd, winffi::GW_OWNER).is_null() {
        return 1; // owned (dialog/popup) → not a top-level app window
    }
    let ex_style = winffi::GetWindowLongW(hwnd, winffi::GWL_EXSTYLE);
    if ex_style & winffi::WS_EX_TOOLWINDOW != 0 {
        return 1; // tool window (floating palette) → skip
    }
    let mut cloaked: i32 = 0;
    winffi::DwmGetWindowAttribute(
        hwnd,
        winffi::DWMWA_CLOAKED,
        &mut cloaked as *mut i32 as *mut std::os::raw::c_void,
        std::mem::size_of::<i32>() as u32,
    );
    if cloaked != 0 {
        return 1; // cloaked (other desktop / suspended UWP) → skip
    }

    // Read the title (len chars + NUL); GetWindowTextW returns the copied count.
    let mut buf = vec![0u16; len as usize + 1];
    let copied = winffi::GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
    let copied = if copied < 0 {
        0
    } else {
        (copied as usize).min(buf.len())
    };
    let title = String::from_utf16_lossy(&buf[..copied])
        .replace(['\t', '\r', '\n'], " ")
        .trim()
        .to_string();

    let mut pid: u32 = 0;
    winffi::GetWindowThreadProcessId(hwnd, &mut pid as *mut u32);

    let is_focused = hwnd == accum.foreground;
    // HWND as an unsigned decimal so `focus_window`'s all-digits guard accepts
    // it and PowerShell's `[int64]` round-trips it back.
    accum
        .rows
        .push(((hwnd as usize).to_string(), pid, is_focused, title));
    1 // keep enumerating
}

/// Base file name of a path (portion after the last `\` or `/`).
#[cfg(target_os = "windows")]
fn file_name(path: &str) -> &str {
    match path.rfind(['\\', '/']) {
        Some(i) => &path[i + 1..],
        None => path,
    }
}

/// Drop a trailing `.exe` (case-insensitive) from a file name.
#[cfg(target_os = "windows")]
fn strip_exe(name: &str) -> String {
    if name.to_ascii_lowercase().ends_with(".exe") {
        name[..name.len() - 4].to_string()
    } else {
        name.to_string()
    }
}

/// Process name (no extension) from a full exe path, or `None` when the path is
/// empty — mirrors the `ProcessName` the old `Get-Process` scheme produced.
#[cfg(target_os = "windows")]
fn exe_stem(path: &str) -> Option<String> {
    if path.is_empty() {
        None
    } else {
        Some(strip_exe(file_name(path)))
    }
}

/// Full executable path for a PID via `QueryFullProcessImageNameW`, or `None`
/// when the process can't be opened (e.g. a higher-integrity process) — the
/// caller then falls back to the snapshot name / `name.exe`.
#[cfg(target_os = "windows")]
fn process_image_path(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    unsafe {
        let handle = winffi::OpenProcess(winffi::PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut buf = vec![0u16; 1024];
        let mut size = buf.len() as u32;
        let ok = winffi::QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size as *mut u32);
        winffi::CloseHandle(handle);
        if ok == 0 || size == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buf[..size as usize]))
    }
}

/// PID → process name (extension stripped) for every running process, via a
/// Toolhelp snapshot. This never needs elevated rights, so it guarantees a name
/// even for windows whose process `process_image_path` can't open — matching the
/// old scheme's property that every listed window had a name. Empty on failure.
#[cfg(target_os = "windows")]
fn snapshot_process_names() -> std::collections::HashMap<u32, String> {
    let mut map = std::collections::HashMap::new();
    unsafe {
        let snap = winffi::CreateToolhelp32Snapshot(winffi::TH32CS_SNAPPROCESS, 0);
        // CreateToolhelp32Snapshot returns INVALID_HANDLE_VALUE (-1) on failure.
        if snap.is_null() || snap == (-1isize as winffi::Handle) {
            return map;
        }
        let mut entry: winffi::ProcessEntry32W = std::mem::zeroed();
        entry.dw_size = std::mem::size_of::<winffi::ProcessEntry32W>() as u32;
        let mut ok = winffi::Process32FirstW(snap, &mut entry as *mut _);
        while ok != 0 {
            let end = entry
                .sz_exe_file
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.sz_exe_file.len());
            let exe = String::from_utf16_lossy(&entry.sz_exe_file[..end]);
            map.insert(entry.th32_process_id, strip_exe(&exe));
            ok = winffi::Process32NextW(snap, &mut entry as *mut _);
        }
        winffi::CloseHandle(snap);
    }
    map
}

/// One app grouped with its live windows:
/// `(display name, launch target, [(handle, title, is_focused)])`.
#[cfg(target_os = "windows")]
type AppWindows = (String, Option<String>, Vec<(String, String, bool)>);

#[cfg(target_os = "windows")]
impl WinWindowEnumerator {
    /// Names of user-launched apps that currently own a visible window.
    pub fn list_running() -> Result<Vec<String>> {
        Ok(Self::list_running_apps()?
            .into_iter()
            .map(|(name, _)| name)
            .collect())
    }

    /// One entry PER APP (windows de-duplicated by name) as
    /// `(display name, launch target)` — used by capture. The launch target is
    /// the full executable path when readable (making both re-launch via
    /// `Start-Process` and icon extraction reliable), else the bare `name.exe`
    /// (which `Start-Process` resolves via App Paths / PATH). There is no stable
    /// "bundle identifier" on Windows, so the target doubles as the id.
    pub fn list_running_apps() -> Result<Vec<(String, Option<String>)>> {
        let mut seen = std::collections::HashSet::new();
        let mut apps: Vec<(String, Option<String>)> = Vec::new();
        for (name, target, _windows) in Self::list_running_windows()? {
            if seen.insert(name.to_ascii_lowercase()) {
                apps.push((name, target));
            }
        }
        Ok(apps)
    }

    /// Apps grouped WITH their individual windows:
    /// `(display name, launch target, [(hwnd, title, is_focused)])`. Windows of
    /// the same app name are collapsed into one group (icon shown once, FR-2.2)
    /// while each window stays separately activatable by its `HWND` string
    /// (FR-2.8). Groups are sorted case-insensitively by name for a stable list;
    /// windows keep enumeration order. Any failure yields an empty vec so
    /// capture never fails hard.
    pub fn list_running_windows() -> Result<Vec<AppWindows>> {
        let mut order: Vec<String> = Vec::new();
        let mut groups: std::collections::HashMap<String, AppWindows> =
            std::collections::HashMap::new();

        for (hwnd, name, path, focused, title) in Self::raw_windows() {
            let key = name.to_ascii_lowercase();
            let entry = groups.entry(key.clone()).or_insert_with(|| {
                order.push(key.clone());
                (name.clone(), None, Vec::new())
            });
            // First real exe path wins as the group's launch / icon target.
            if entry.1.is_none() && !path.is_empty() {
                entry.1 = Some(path.clone());
            }
            entry.2.push((hwnd, title, focused));
        }

        let mut result: Vec<AppWindows> = order
            .into_iter()
            .map(|k| {
                let (name, target, windows) = groups.remove(&k).unwrap();
                let target = target.unwrap_or_else(|| format!("{name}.exe"));
                (name, Some(target), windows)
            })
            .collect();
        result.sort_by_key(|a| a.0.to_ascii_lowercase());
        Ok(result)
    }

    /// Raw per-window rows `(hwnd, name, path, is_focused, title)` gathered with
    /// `EnumWindows` (one row PER top-level window). Empty on any failure so
    /// enumeration never fails hard. The exe path is best-effort
    /// (`QueryFullProcessImageNameW`, may be empty for higher-integrity
    /// processes); the name is guaranteed by the Toolhelp snapshot, falling back
    /// to the path stem and finally `pid-<n>` so a window is never dropped for
    /// lack of a name. Title falls back to the process name when blank.
    fn raw_windows() -> Vec<(String, String, String, bool, String)> {
        let mut accum = WinAccum {
            foreground: unsafe { winffi::GetForegroundWindow() },
            rows: Vec::new(),
        };
        unsafe {
            winffi::EnumWindows(enum_windows_cb, &mut accum as *mut WinAccum as isize);
        }
        if accum.rows.is_empty() {
            return Vec::new();
        }

        let names = snapshot_process_names();
        // Cache the exe path per PID so multiple windows of one process open it
        // once (and never re-open on the 1s poll within a single call).
        let mut path_by_pid: std::collections::HashMap<u32, String> =
            std::collections::HashMap::new();

        let mut rows = Vec::with_capacity(accum.rows.len());
        for (hwnd, pid, focused, title) in accum.rows {
            let path = path_by_pid
                .entry(pid)
                .or_insert_with(|| process_image_path(pid).unwrap_or_default())
                .clone();
            // Prefer the real exe stem; else the snapshot name; else the PID, so
            // a window is never lost just because its process couldn't be opened.
            let name = exe_stem(&path)
                .or_else(|| names.get(&pid).cloned())
                .unwrap_or_else(|| format!("pid-{pid}"));
            if name.is_empty() {
                continue;
            }
            let title = if title.is_empty() { name.clone() } else { title };
            rows.push((hwnd, name, path, focused, title));
        }
        rows
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

    /// Bring a SPECIFIC window — identified by the `HWND` string that
    /// `list_running_windows` produced — to the front. This is the per-window
    /// counterpart of `open_app` (FR-2.8 / FR-4.1): where `open_app` focuses
    /// *any* window of the app, this targets exactly the window the user picked
    /// from an expanded group, honoring FR-4.2 ("activating the app alone is not
    /// success") by construction — it acts on one handle, not a process.
    ///
    /// It restores (un-minimizes) and foregrounds that handle with the same
    /// Win32 combo `focus_existing_window` uses (`ShowWindowAsync` +
    /// `AppActivate` by the window's owning PID + `SetForegroundWindow`, compiled
    /// inline via `Add-Type`). If the handle is no longer a live window (the user
    /// closed it between the poll and the click), it returns an error so the
    /// caller can drop it / re-enumerate — the poll removes ended windows anyway
    /// (FR-2.8). The handle is validated as a decimal integer and passed via an
    /// env var, never the command line (see module SECURITY).
    pub fn focus_window(hwnd: &str) -> Result<()> {
        // Defense in depth: the value only ever comes from our own enumeration,
        // but reject anything that isn't a bare decimal handle before it reaches
        // PowerShell.
        if hwnd.is_empty() || !hwnd.bytes().all(|b| b.is_ascii_digit()) {
            return Err(CoreError::Internal(format!(
                "invalid window handle: {hwnd:?}"
            )));
        }
        const SCRIPT: &str = "\
$h = [IntPtr][int64]$env:VC_HWND; \
Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public static class VcFocus { [DllImport(\"user32.dll\")] public static extern bool IsWindow(IntPtr h); [DllImport(\"user32.dll\")] public static extern bool SetForegroundWindow(IntPtr h); [DllImport(\"user32.dll\")] public static extern bool ShowWindowAsync(IntPtr h, int n); [DllImport(\"user32.dll\")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid); }'; \
if (-not [VcFocus]::IsWindow($h)) { 'GONE'; exit 0 }; \
[void][VcFocus]::ShowWindowAsync($h, 9); \
$procId = [uint32]0; [void][VcFocus]::GetWindowThreadProcessId($h, [ref]$procId); \
try { $ws = New-Object -ComObject WScript.Shell; if ($procId -ne 0) { [void]$ws.AppActivate([int]$procId) } } catch {}; \
[void][VcFocus]::SetForegroundWindow($h); \
'OK'";
        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
            .env("VC_HWND", hwnd)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| CoreError::Internal(format!("powershell focus_window failed: {e}")))?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.contains("GONE") {
            return Err(CoreError::Internal("window is no longer open".into()));
        }
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(CoreError::Internal(format!(
                "could not focus window: {}",
                stderr.trim()
            )));
        }
        Ok(())
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
