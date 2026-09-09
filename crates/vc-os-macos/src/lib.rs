// macOS adapters for vibe-control.
// Enumerates running GUI apps and open browser tabs via `osascript`.
// All calls degrade gracefully: missing permissions or a closed app yield an
// empty result rather than an error, so capture never fails hard.

#[cfg(target_os = "macos")]
use std::process::Command;
#[cfg(target_os = "macos")]
use vc_core::{CoreError, Result};

/// Run an AppleScript snippet, returning trimmed stdout on success.
#[cfg(target_os = "macos")]
fn run_osascript(script: &str) -> Option<String> {
    let output = Command::new("osascript").arg("-e").arg(script).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Run a JavaScript-for-Automation (JXA) snippet via `osascript -l JavaScript`,
/// returning trimmed stdout on success. Trailing `args` are passed as `run()`'s
/// argv (never string-interpolated into the script), matching the ObjC-bridge
/// approach `MacIconReader` uses. Unlike `System Events` automation, the AppKit
/// APIs reached this way (e.g. `NSWorkspace`) need no Automation permission, so
/// they don't prompt or stall on first launch.
#[cfg(target_os = "macos")]
fn run_jxa(script: &str, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new("osascript");
    cmd.arg("-l").arg("JavaScript").arg("-e").arg(script);
    for a in args {
        cmd.arg(a);
    }
    let output = cmd.output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Whether this process holds macOS **Accessibility** permission — required for
/// the `System Events` pass behind per-instance window listing (VS Code /
/// Terminal / any non-browser, non-Finder app). Window *titles* on modern macOS
/// are permission-gated, so this cannot be worked around in code.
///
/// When `prompt` is true and permission is missing, macOS shows the one-time
/// "allow Accessibility" dialog for THIS app; the user must click Allow (there
/// is no way to grant it programmatically). Because the per-window scan runs via
/// an `osascript` child whose *responsible process* is this app, granting the
/// app authorizes those System Events calls too — the same attribution that lets
/// Safari/Chrome tab reads work once Automation is allowed.
#[cfg(target_os = "macos")]
pub fn accessibility_trusted(prompt: bool) -> bool {
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::string::{CFString, CFStringRef};

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
        static kAXTrustedCheckOptionPrompt: CFStringRef;
    }

    // SAFETY: `kAXTrustedCheckOptionPrompt` is a framework-owned constant string
    // (get-rule); the options dict is a well-formed {CFString: CFBoolean} that
    // `AXIsProcessTrustedWithOptions` only reads.
    unsafe {
        let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
        let val = if prompt {
            CFBoolean::true_value()
        } else {
            CFBoolean::false_value()
        };
        let opts = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), val.as_CFType())]);
        AXIsProcessTrustedWithOptions(opts.as_concrete_TypeRef())
    }
}

/// Non-macOS builds have no Accessibility gate; treat as always trusted.
#[cfg(not(target_os = "macos"))]
pub fn accessibility_trusted(_prompt: bool) -> bool {
    true
}

/// One app grouped with its live windows:
/// `(display name, bundle id, [(handle, title, is_focused)])`.
#[cfg(target_os = "macos")]
type AppWindows = (String, Option<String>, Vec<(String, String, bool)>);

/// One expandable child of an app in the running-apps list:
/// `(kind, handle, title, target, is_focused)` where `kind` is
/// `"tab"` | `"folder"` | `"window"`. `target` is the value registered into a
/// group (a URL, a POSIX folder path, or a window handle); `handle` activates
/// the child (`app\u{1f}title\u{1f}idx` for windows, `Finder\u{1f}name` for
/// folders, `app\u{1f}url` for tabs).
#[cfg(target_os = "macos")]
pub type AppChild = (String, String, String, String, bool);

#[cfg(target_os = "macos")]
pub struct MacWindowEnumerator;

#[cfg(target_os = "macos")]
impl MacWindowEnumerator {
    /// Names of currently running foreground (non-background) applications.
    pub fn list_running() -> Result<Vec<String>> {
        let script = "tell application \"System Events\" to get name of every process whose background only is false";
        let Some(out) = run_osascript(script) else {
            return Ok(vec![]);
        };
        let mut names: Vec<String> = out
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        names.sort();
        names.dedup();
        Ok(names)
    }

    /// Running foreground apps as (display name, bundle identifier). The bundle
    /// id is the stable restore target; it's `None` for the rare process that
    /// doesn't expose one. Name and id come from a single query so they stay
    /// aligned even if the process list changes between calls.
    ///
    /// Backed by `NSWorkspace.runningApplications` (via JXA) rather than a
    /// `System Events` process loop: it's a single fast AppKit call, needs no
    /// Automation permission (so no cold-start prompt/stall — this runs on app
    /// launch), and reports the LaunchServices `localizedName`/`bundleIdentifier`
    /// directly. Only regular (Dock) apps are kept — `activationPolicy == 0`,
    /// which JXA surfaces as the string "0".
    pub fn list_running_apps() -> Result<Vec<(String, Option<String>)>> {
        // Emits `name<TAB>bundleid` per line; bundle id may be empty.
        const SCRIPT: &str = r#"
ObjC.import('AppKit');
function run() {
  var apps = $.NSWorkspace.sharedWorkspace.runningApplications;
  var out = '';
  var n = apps.count;
  for (var i = 0; i < n; i++) {
    var a = apps.objectAtIndex(i);
    if (String(a.activationPolicy) !== '0') { continue; }
    var name = a.localizedName;
    if (name.isNil()) { continue; }
    var nameStr = ObjC.unwrap(name);
    if (nameStr === '') { continue; }
    var bid = a.bundleIdentifier;
    var bidStr = bid.isNil() ? '' : ObjC.unwrap(bid);
    out += nameStr + '\t' + bidStr + '\n';
  }
  return out;
}
"#;
        let Some(out) = run_jxa(SCRIPT, &[]) else {
            return Ok(vec![]);
        };
        let mut apps: Vec<(String, Option<String>)> = out
            .lines()
            .filter_map(|line| {
                let (name, bid) = line.split_once('\t').unwrap_or((line, ""));
                let name = name.trim();
                if name.is_empty() {
                    return None;
                }
                let bid = bid.trim();
                let bundle_id = if bid.is_empty() {
                    None
                } else {
                    Some(bid.to_string())
                };
                Some((name.to_string(), bundle_id))
            })
            .collect();
        apps.sort();
        apps.dedup();
        Ok(apps)
    }

    /// Apps grouped WITH their individual windows:
    /// `(display name, bundle id, [(handle, title, is_focused)])` (FR-2.8/AC-20).
    ///
    /// The app list + bundle ids come from `NSWorkspace` (no permission, never
    /// regresses the current app-level list). Per-window titles are OVERLAID
    /// from a single `System Events` pass — which needs Accessibility permission,
    /// as any per-window enumeration on macOS inherently does. When that pass
    /// yields nothing (permission not granted) or a process name doesn't line up
    /// with the LaunchServices name, the app simply carries no window detail and
    /// the UI treats it as one entry that activates the app (single-window
    /// consistency). The per-window `handle` is `display_name\u{1f}title` so
    /// `MacLauncher::focus_window` can raise exactly that window by title.
    pub fn list_running_windows() -> Result<Vec<AppWindows>> {
        let apps = Self::list_running_apps()?;
        let windows_by_proc = Self::windows_by_process();
        let out = apps
            .into_iter()
            .map(|(name, bundle_id)| {
                let windows = windows_by_proc
                    .get(&name.to_lowercase())
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(title, focused)| {
                        let handle = format!("{name}\u{1f}{title}");
                        (handle, title, focused)
                    })
                    .collect();
                (name, bundle_id, windows)
            })
            .collect();
        Ok(out)
    }

    /// process-name(lowercased) → its windows `(title, is_focused)`, via a single
    /// `System Events` pass. Empty when Accessibility permission isn't granted —
    /// callers fall back to app-level activation. The frontmost process's front
    /// window (index 1) is flagged focused (FR-2.4).
    fn windows_by_process() -> std::collections::HashMap<String, Vec<(String, bool)>> {
        const SCRIPT: &str = "tell application \"System Events\"\n\
            set out to \"\"\n\
            set frontApp to \"\"\n\
            try\n\
            set frontApp to name of first process whose frontmost is true\n\
            end try\n\
            repeat with p in (processes whose background only is false)\n\
            set pn to name of p\n\
            set idx to 0\n\
            try\n\
            repeat with w in windows of p\n\
            set idx to idx + 1\n\
            set wt to \"\"\n\
            try\n\
            set wt to name of w\n\
            end try\n\
            set foc to \"0\"\n\
            if (pn is frontApp) and (idx is 1) then set foc to \"1\"\n\
            set out to out & pn & tab & foc & tab & wt & linefeed\n\
            end repeat\n\
            end try\n\
            end repeat\n\
            return out\n\
            end tell";
        let mut map: std::collections::HashMap<String, Vec<(String, bool)>> =
            std::collections::HashMap::new();
        let Some(out) = run_osascript(SCRIPT) else {
            return map;
        };
        for line in out.lines() {
            let mut parts = line.splitn(3, '\t');
            let pn = parts.next().unwrap_or("").trim();
            let focused = parts.next().unwrap_or("0").trim() == "1";
            let title = parts.next().unwrap_or("").trim();
            if pn.is_empty() {
                continue;
            }
            let title = if title.is_empty() {
                pn.to_string()
            } else {
                title.to_string()
            };
            map.entry(pn.to_lowercase())
                .or_default()
                .push((title, focused));
        }
        map
    }

    /// Lazily enumerate one app's expandable children when the user expands it in
    /// the running-apps list. Kept off the fast `NSWorkspace` app-list path so the
    /// left panel never stalls on the slow, permission-gated per-window scan. Each
    /// query is bounded by an AppleScript `with timeout`, so one unresponsive app
    /// fails fast instead of hanging the UI. Dispatch by app:
    /// - Safari / Google Chrome → per-tab children (target = URL)
    /// - Finder → per-window folder children (target = POSIX path)
    /// - everything else → per-window children (target = window handle)
    pub fn list_app_children(app_name: &str, _bundle_id: Option<&str>) -> Vec<AppChild> {
        match app_name {
            "Safari" => Self::browser_tabs("Safari", "name", "current tab"),
            "Google Chrome" => Self::browser_tabs("Google Chrome", "title", "active tab"),
            "Finder" => Self::finder_folders(),
            other => Self::app_windows(other),
        }
    }

    /// Browser tabs (title + URL) across all windows; the active tab of each
    /// window is flagged focused. `title_prop`/`curtab_prop` differ per browser
    /// (Safari: `name`/`current tab`; Chrome: `title`/`active tab`).
    fn browser_tabs(app: &str, title_prop: &str, curtab_prop: &str) -> Vec<AppChild> {
        let esc_app = app.replace('\\', "\\\\").replace('"', "\\\"");
        // IMPORTANT: `tab` is a terminology word inside `tell application
        // "Safari"/"Google Chrome"` — both dictionaries define a `tab` class — so the
        // bare AppleScript `tab` constant coerces to the literal text "tab" instead of
        // an ASCII tab, corrupting the delimiter. We therefore compute a unit-separator
        // delimiter (U+001F, which never occurs in titles/URLs) OUTSIDE the tell block,
        // where no app terminology can shadow it, and split on it in Rust.
        let script = format!(
            "set fs to (character id 31)\n\
             set out to \"\"\n\
             tell application \"{esc_app}\"\n\
             try\n\
             with timeout of 3 seconds\n\
             repeat with w in windows\n\
             set curT to missing value\n\
             try\n\
             set curT to {curtab_prop} of w\n\
             end try\n\
             repeat with t in tabs of w\n\
             set foc to \"0\"\n\
             try\n\
             if (t is curT) then set foc to \"1\"\n\
             end try\n\
             set out to out & foc & fs & ({title_prop} of t) & fs & (URL of t) & linefeed\n\
             end repeat\n\
             end repeat\n\
             end timeout\n\
             end try\n\
             end tell\n\
             return out"
        );
        let Some(out) = run_osascript(&script) else {
            return vec![];
        };
        out.lines()
            .filter_map(|line| {
                let mut parts = line.splitn(3, '\u{1f}');
                let foc = parts.next().unwrap_or("0").trim() == "1";
                let title = parts.next().unwrap_or("").trim();
                let url = parts.next().unwrap_or("").trim();
                if url.is_empty() {
                    return None;
                }
                let title = if title.is_empty() { url } else { title };
                let handle = format!("{app}\u{1f}{url}");
                Some((
                    "tab".to_string(),
                    handle,
                    title.to_string(),
                    url.to_string(),
                    foc,
                ))
            })
            .collect()
    }

    /// Finder windows as folder children: window name + the POSIX path it shows.
    fn finder_folders() -> Vec<AppChild> {
        // IMPORTANT: iterate `Finder window i` BY INDEX, not `repeat with w in
        // Finder windows`. The `repeat with w in ...` form makes Finder resolve
        // `name of w` against the window's *selection/contents* — it yields the
        // enclosed file names with empty targets, so every row got dropped and the
        // group showed nothing. Indexing returns one row per actual window.
        // `tab` is Finder terminology, so the U+001F delimiter is computed outside
        // the tell block where no app terminology can shadow it.
        const SCRIPT: &str = "set fs to (character id 31)\n\
             set out to \"\"\n\
             tell application \"Finder\"\n\
             try\n\
             with timeout of 2 seconds\n\
             set n to (count of Finder windows)\n\
             repeat with i from 1 to n\n\
             set w to Finder window i\n\
             set nm to \"\"\n\
             try\n\
             set nm to name of w\n\
             end try\n\
             set p to \"\"\n\
             try\n\
             set p to POSIX path of (target of w as alias)\n\
             end try\n\
             set out to out & nm & fs & p & linefeed\n\
             end repeat\n\
             end timeout\n\
             end try\n\
             end tell\n\
             return out";
        let Some(out) = run_osascript(SCRIPT) else {
            return vec![];
        };
        // Basename of a POSIX path or a name that happens to be a path: drop a
        // trailing slash, then take the last segment. Empty → the input.
        fn basename(s: &str) -> &str {
            let s = s.trim_end_matches('/');
            s.rsplit('/').next().filter(|b| !b.is_empty()).unwrap_or(s)
        }
        out.lines()
            .filter_map(|line| {
                let (name, path) = line.split_once('\u{1f}')?;
                let name = name.trim();
                let path = path.trim();
                // Only a truly blank row (the trailing linefeed) is dropped. A
                // window WITHOUT a filesystem target (Recents / saved search /
                // AirDrop / network) is still a real instance the user wants to
                // see, so it is kept and activated by window name instead of path.
                if name.is_empty() && path.is_empty() {
                    return None;
                }
                let title = if !path.is_empty() {
                    basename(path).to_string()
                } else {
                    // Saved-search / smart-folder windows: show a clean name.
                    basename(name)
                        .trim_end_matches(".cannedSearch")
                        .trim_end_matches(".savedSearch")
                        .to_string()
                };
                let title = if title.is_empty() {
                    name.to_string()
                } else {
                    title
                };
                // Handle carries the raw window name so a path-less window can be
                // raised by name (see `activate_child`).
                let handle = format!("Finder\u{1f}{name}");
                Some((
                    "folder".to_string(),
                    handle,
                    title,
                    path.to_string(),
                    false,
                ))
            })
            .collect()
    }

    /// Raise an already-open Finder window by its name (title). Used for windows
    /// that have no filesystem path (Recents / saved searches) and therefore
    /// cannot be re-opened via `open <path>`. Best-effort: brings Finder frontmost
    /// even if no window name matches.
    pub fn activate_finder_window(name: &str) -> Result<()> {
        let esc = name.replace('\\', "\\\\").replace('"', "\\\"");
        let script = format!(
            "tell application \"Finder\"\n\
             activate\n\
             try\n\
             with timeout of 2 seconds\n\
             set n to (count of Finder windows)\n\
             repeat with i from 1 to n\n\
             set w to Finder window i\n\
             if (name of w) is \"{esc}\" then\n\
             set index of w to 1\n\
             exit repeat\n\
             end if\n\
             end repeat\n\
             end timeout\n\
             end try\n\
             end tell"
        );
        run_osascript(&script).map(|_| ()).ok_or_else(|| {
            CoreError::Internal("Finder window activation failed (grant Automation)".into())
        })
    }

    /// Windows of a SINGLE process via `System Events`, bounded by a 2s timeout so
    /// an unresponsive app fails fast instead of stalling. `System Events` returns
    /// a process's windows front-to-back, so window index 1 is that app's own
    /// front window — flagged focused, the "active window in this group" marker
    /// (FR-2.4). (It is keyed on the process's own z-order, NOT on whether the app
    /// is system-frontmost — vibe-control itself is frontmost while the user reads
    /// this list, so a system-frontmost gate would never light up.)
    fn app_windows(app_name: &str) -> Vec<AppChild> {
        let esc = app_name.replace('\\', "\\\\").replace('"', "\\\"");
        // Delimiter computed outside the tell block (see `browser_tabs`) so no
        // System Events terminology can shadow it. Each row is
        // `foc <fs> idx <fs> title`; the 1-based window index disambiguates two
        // windows that share a title (common for Terminal/VS Code) so each is
        // independently registerable and focusable.
        let script = format!(
            "set fs to (character id 31)\n\
             set out to \"\"\n\
             tell application \"System Events\"\n\
             try\n\
             with timeout of 2 seconds\n\
             set p to first process whose name is \"{esc}\"\n\
             set idx to 0\n\
             repeat with w in windows of p\n\
             set idx to idx + 1\n\
             set wt to \"\"\n\
             try\n\
             set wt to name of w\n\
             end try\n\
             set foc to \"0\"\n\
             if (idx is 1) then set foc to \"1\"\n\
             set out to out & foc & fs & idx & fs & wt & linefeed\n\
             end repeat\n\
             end timeout\n\
             end try\n\
             end tell\n\
             return out"
        );
        let Some(out) = run_osascript(&script) else {
            return vec![];
        };
        out.lines()
            .filter_map(|line| {
                let mut parts = line.splitn(3, '\u{1f}');
                let foc = parts.next().unwrap_or("0").trim() == "1";
                let idx = parts.next().unwrap_or("").trim();
                let title = parts.next().unwrap_or("").trim();
                if title.is_empty() {
                    return None;
                }
                // handle = `app\u{1f}title\u{1f}idx`; `focus_window` raises the
                // window at this exact position, falling back to a title match.
                let handle = format!("{app_name}\u{1f}{title}\u{1f}{idx}");
                Some((
                    "window".to_string(),
                    handle.clone(),
                    title.to_string(),
                    handle,
                    foc,
                ))
            })
            .collect()
    }
}

#[cfg(target_os = "macos")]
pub struct MacBrowserTabReader;

#[cfg(target_os = "macos")]
impl MacBrowserTabReader {
    /// Open (title, url) tabs across Safari and Chrome, for whichever is running.
    /// Browsers that aren't running are skipped (never launched).
    pub fn read_tabs() -> Result<Vec<(String, String)>> {
        let running = MacWindowEnumerator::list_running().unwrap_or_default();
        let mut tabs = Vec::new();

        if running.iter().any(|n| n == "Safari") {
            tabs.extend(Self::read_app_tabs("Safari", "name"));
        }
        if running.iter().any(|n| n == "Google Chrome") {
            tabs.extend(Self::read_app_tabs("Google Chrome", "title"));
        }
        Ok(tabs)
    }

    /// `title_prop` differs: Safari exposes `name of t`, Chrome exposes `title of t`.
    fn read_app_tabs(app: &str, title_prop: &str) -> Vec<(String, String)> {
        // `tab` collides with browser `tab` terminology inside the tell block, so use
        // a unit-separator (U+001F) delimiter computed outside it.
        let script = format!(
            "set fs to (character id 31)\n\
             set out to \"\"\n\
             tell application \"{app}\"\n\
             repeat with w in windows\n\
             repeat with t in tabs of w\n\
             set out to out & ({title_prop} of t) & fs & (URL of t) & linefeed\n\
             end repeat\n\
             end repeat\n\
             end tell\n\
             return out"
        );
        let Some(out) = run_osascript(&script) else {
            return vec![];
        };
        out.lines()
            .filter_map(|line| {
                let (title, url) = line.split_once('\u{1f}')?;
                let url = url.trim();
                if url.is_empty() {
                    return None;
                }
                let title = if title.trim().is_empty() { url } else { title.trim() };
                Some((title.to_string(), url.to_string()))
            })
            .collect()
    }
}

/// Reopens things captured in a bundle: apps, folders, URLs, and coding
/// sessions (resumed in a fresh Terminal window).
#[cfg(target_os = "macos")]
pub struct MacLauncher;

#[cfg(target_os = "macos")]
impl MacLauncher {
    /// Launch (or focus) an application. `target` may be a bundle identifier
    /// (e.g. `com.microsoft.VSCode`) or an application name.
    ///
    /// Application *names* are tricky: a process name reported by System Events
    /// (e.g. `Code`) is not always what LaunchServices knows the app as
    /// (`Visual Studio Code`), so `open -a Code` fails. We therefore try, in
    /// order: `open -b` for bundle-id-looking targets, `open -a` for names, and
    /// finally resolve the name to a bundle id via `id of app` and `open -b`.
    pub fn open_app(target: &str) -> Result<()> {
        if is_bundle_id(target) {
            return Self::run_open(&["-b", target]);
        }
        if Self::run_open(&["-a", target]).is_ok() {
            return Ok(());
        }
        if let Some(bundle_id) = resolve_bundle_id(target) {
            return Self::run_open(&["-b", &bundle_id]);
        }
        Err(CoreError::Internal(format!(
            "could not open application '{target}' (no matching app name or bundle id)"
        )))
    }

    /// Bring a SPECIFIC window to the front (FR-2.8 / FR-4.1). `handle` is
    /// `display_name\u{1f}title` or `display_name\u{1f}title\u{1f}idx` (the 1-based
    /// window index, when known) as produced by `app_windows`/`list_running_windows`.
    /// Activates the app first (works without Accessibility), then best-effort
    /// raises the exact window via `System Events` `AXRaise`: by window index first
    /// (unambiguous when two windows share a title), falling back to a title match
    /// for older, index-less handles. Raising the exact window needs Accessibility
    /// permission; failure is non-fatal because the app is already frontmost. When
    /// the handle carries neither title nor index, this degrades to plain app
    /// activation (single-window consistency). Also used to restore a WindowRef
    /// resource, so a closed app is relaunched by the leading `open_app`.
    pub fn focus_window(handle: &str) -> Result<()> {
        let mut parts = handle.splitn(3, '\u{1f}');
        let app = parts.next().unwrap_or(handle);
        let title = parts.next().unwrap_or("");
        let idx: Option<u32> = parts.next().and_then(|s| s.trim().parse().ok());
        // Always bring the app forward first (relaunches it if it was closed).
        Self::open_app(app)?;
        if title.is_empty() && idx.is_none() {
            return Ok(());
        }
        let esc_app = app.replace('\\', "\\\\").replace('"', "\\\"");
        let esc_title = title.replace('\\', "\\\\").replace('"', "\\\"");
        // Raise the exact window position first; if that index is now out of range
        // (a window closed), fall through to a title match.
        let index_raise = match idx {
            Some(i) => format!(
                "try\n\
                 perform action \"AXRaise\" of (window {i} of p)\n\
                 set frontmost of p to true\n\
                 return\n\
                 end try\n"
            ),
            None => String::new(),
        };
        let title_raise = if title.is_empty() {
            String::new()
        } else {
            format!(
                "repeat with w in windows of p\n\
                 if (name of w) is \"{esc_title}\" then\n\
                 perform action \"AXRaise\" of w\n\
                 set frontmost of p to true\n\
                 return\n\
                 end if\n\
                 end repeat\n"
            )
        };
        let script = format!(
            "tell application \"System Events\"\n\
             try\n\
             set p to first process whose name is \"{esc_app}\"\n\
             {index_raise}\
             {title_raise}\
             end try\n\
             end tell"
        );
        // Advisory: the app is already frontmost even if the raise fails.
        let _ = run_osascript(&script);
        Ok(())
    }

    /// Focus a specific browser tab by URL (FR-2.2 for per-tab items). Activates
    /// the browser, then selects the first tab whose URL matches and raises its
    /// window. Selecting the active tab differs per browser (Chrome sets
    /// `active tab index`; Safari sets `current tab`). If NO tab matches (the tab
    /// was closed since it was registered), the URL is reopened in a new tab of
    /// that same browser — otherwise the browser would just come frontmost on
    /// whatever unrelated tab happened to be active, which looks like the wrong
    /// tab got activated.
    pub fn activate_browser_tab(app: &str, url: &str) -> Result<()> {
        Self::open_app(app)?;
        let esc_app = app.replace('\\', "\\\\").replace('"', "\\\"");
        let esc_url = url.replace('\\', "\\\\").replace('"', "\\\"");
        let select = if app == "Google Chrome" {
            "set active tab index of w to i\nset index of w to 1"
        } else {
            "set current tab of w to t\nset index of w to 1"
        };
        // Reopen path when the tab is gone. Chrome has no `open location`; make a
        // tab explicitly (spawning a window first if none is open). Safari's
        // `open location` reliably opens the URL in a new tab of the front window.
        let reopen = if app == "Google Chrome" {
            "if (count of windows) is 0 then\n\
             make new window\n\
             end if\n\
             tell window 1 to make new tab with properties {URL:\"URL_PLACEHOLDER\"}\n\
             set index of window 1 to 1"
        } else {
            "open location \"URL_PLACEHOLDER\""
        }
        .replace("URL_PLACEHOLDER", &esc_url);
        let script = format!(
            "tell application \"{esc_app}\"\n\
             activate\n\
             set didFind to false\n\
             try\n\
             repeat with w in windows\n\
             set i to 0\n\
             repeat with t in tabs of w\n\
             set i to i + 1\n\
             if (URL of t) is \"{esc_url}\" then\n\
             {select}\n\
             set didFind to true\n\
             exit repeat\n\
             end if\n\
             end repeat\n\
             if didFind then exit repeat\n\
             end repeat\n\
             end try\n\
             if not didFind then\n\
             {reopen}\n\
             end if\n\
             end tell"
        );
        // Advisory: the browser is already frontmost even if selection fails.
        let _ = run_osascript(&script);
        Ok(())
    }

    /// Open a file or folder in its default handler / Finder.
    pub fn open_path(path: &str) -> Result<()> {
        Self::run_open(&[path])
    }

    /// Open a URL in the default browser.
    pub fn open_url(url: &str) -> Result<()> {
        Self::run_open(&[url])
    }

    fn run_open(args: &[&str]) -> Result<()> {
        let output = Command::new("open")
            .args(args)
            .output()
            .map_err(|e| CoreError::Internal(format!("failed to spawn `open`: {e}")))?;
        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(CoreError::Internal(format!(
                "`open {}` failed: {}",
                args.join(" "),
                stderr.trim()
            )))
        }
    }

    /// Open a new Terminal window and run `command` in it (used to resume a
    /// coding session with `claude --resume`). Requires Automation permission
    /// for Terminal; degrades to an error otherwise.
    pub fn run_in_terminal(command: &str) -> Result<()> {
        let escaped = command.replace('\\', "\\\\").replace('"', "\\\"");
        let script = format!(
            "tell application \"Terminal\"\n\
             activate\n\
             do script \"{escaped}\"\n\
             end tell"
        );
        run_osascript(&script).map(|_| ()).ok_or_else(|| {
            CoreError::Internal(
                "Terminal automation failed (grant Automation permission for Terminal)".into(),
            )
        })
    }

    /// Bring the already-open Terminal (hosting a Claude Code session) to the
    /// front WITHOUT spawning anything new. `match_hint` is an optional string
    /// (e.g. the session's working-directory name) used for best-effort focus
    /// of the specific window/tab whose title contains it; if no tab matches,
    /// the plain `activate` still brings Terminal forward. Requires Automation
    /// permission for Terminal, like `run_in_terminal`.
    pub fn activate_terminal(match_hint: Option<&str>) -> Result<()> {
        let selector = match match_hint {
            Some(hint) if !hint.is_empty() => {
                let escaped = hint.replace('\\', "\\\\").replace('"', "\\\"");
                format!(
                    "try\n\
                     repeat with w in windows\n\
                     repeat with t in tabs of w\n\
                     if (custom title of t) contains \"{escaped}\" then\n\
                     set index of w to 1\n\
                     set selected of t to true\n\
                     return\n\
                     end if\n\
                     end repeat\n\
                     end repeat\n\
                     end try\n"
                )
            }
            _ => String::new(),
        };
        let script = format!(
            "tell application \"Terminal\"\n\
             activate\n\
             {selector}\
             end tell"
        );
        run_osascript(&script).map(|_| ()).ok_or_else(|| {
            CoreError::Internal(
                "Terminal automation failed (grant Automation permission for Terminal)".into(),
            )
        })
    }
}

/// Heuristic: reverse-DNS-looking strings (has a dot, no spaces, ≥2 segments)
/// are treated as bundle identifiers rather than application names.
#[cfg(target_os = "macos")]
fn is_bundle_id(s: &str) -> bool {
    s.contains('.') && !s.contains(' ') && s.split('.').filter(|p| !p.is_empty()).count() >= 2
}

/// Resolve an application name to its bundle identifier via `id of app`.
/// This uses a fuzzier lookup than `open -a`, so it resolves process names
/// like `Code` that `open -a` rejects.
#[cfg(target_os = "macos")]
fn resolve_bundle_id(name: &str) -> Option<String> {
    let escaped = name.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!("id of app \"{escaped}\"");
    run_osascript(&script).filter(|s| !s.is_empty())
}

/// Reads application icons at high resolution (§13.5) via AppKit's
/// `NSWorkspace`, driven through JXA's ObjC bridge in `osascript` — the same
/// shell-out spirit as the rest of this module, with zero native deps. A
/// missing app or an un-renderable icon yields `None`, never an error,
/// matching the graceful-degradation convention used throughout.
#[cfg(target_os = "macos")]
pub struct MacIconReader;

#[cfg(target_os = "macos")]
impl MacIconReader {
    /// Pixel size of the extracted icon. 128px stays crisp on Retina when
    /// shown in a ~20px box (far above the 2× physical-pixel requirement)
    /// while remaining small enough to cache many icons in memory.
    const ICON_PX: u32 = 128;

    /// An app's icon as a ready-to-use `data:image/png;base64,…` URI, or
    /// `None` when the app can't be found or its icon can't be rendered.
    /// `target` may be a bundle identifier (`com.microsoft.VSCode`) or an app
    /// name — names are resolved to a bundle id via the same `id of app`
    /// lookup `open_app` uses.
    pub fn icon_data_uri(target: &str) -> Option<String> {
        let b64 = Self::icon_png_base64(target, Self::ICON_PX)?;
        Some(format!("data:image/png;base64,{b64}"))
    }

    /// Render the app icon into a `size`×`size` PNG and return raw base64.
    ///
    /// Drives `NSWorkspace.iconForFile` (the composited icon the OS actually
    /// shows, backed by the app's 512/1024px representations) and draws it
    /// into an exact NxN ARGB bitmap so it's sharp on high-DPI displays.
    ///
    /// NOTE: zero-argument ObjC methods (`lockFocus`, `saveGraphicsState`) are
    /// NOT bridged by `osascript`, so this uses the `NSBitmapImageRep` +
    /// `graphicsContextWithBitmapImageRep` drawing path (all take args).
    /// `run()`'s return value is what lands on stdout; `bundle_id`/`size` are
    /// passed as trailing argv (not interpolated) to avoid any injection.
    fn icon_png_base64(target: &str, size: u32) -> Option<String> {
        const SCRIPT: &str = r#"
ObjC.import('AppKit');
function run(argv) {
  var bundleId = argv[0];
  var size = parseFloat(argv[1]);
  var ws = $.NSWorkspace.sharedWorkspace;
  var path = ws.absolutePathForAppBundleWithIdentifier(bundleId);
  if (path.isNil()) { return ''; }
  var icon = ws.iconForFile(path);
  if (icon.isNil()) { return ''; }
  var rep = $.NSBitmapImageRep.alloc.initWithBitmapDataPlanesPixelsWidePixelsHighBitsPerSampleSamplesPerPixelHasAlphaIsPlanarColorSpaceNameBytesPerRowBitsPerPixel(
    $(), size, size, 8, 4, true, false, $.NSCalibratedRGBColorSpace, 0, 0);
  rep.setSize($.NSMakeSize(size, size));
  var ctx = $.NSGraphicsContext.graphicsContextWithBitmapImageRep(rep);
  $.NSGraphicsContext.setCurrentContext(ctx);
  icon.drawInRectFromRectOperationFraction(
    $.NSMakeRect(0, 0, size, size), $.NSZeroRect, $.NSCompositingOperationSourceOver, 1.0);
  var png = rep.representationUsingTypeProperties($.NSBitmapImageFileTypePNG, $());
  return ObjC.unwrap(png.base64EncodedStringWithOptions(0));
}
"#;

        let bundle_id = if is_bundle_id(target) {
            target.to_string()
        } else {
            // Resolve process/app names (e.g. "Code") to a bundle id first.
            resolve_bundle_id(target)?
        };

        let output = Command::new("osascript")
            .arg("-l")
            .arg("JavaScript")
            .arg("-e")
            .arg(SCRIPT)
            .arg(&bundle_id) // argv[0] in run()
            .arg(size.to_string()) // argv[1]
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

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn bundle_id_heuristic() {
        assert!(is_bundle_id("com.microsoft.VSCode"));
        assert!(is_bundle_id("com.apple.Safari"));
        assert!(!is_bundle_id("Code")); // process name
        assert!(!is_bundle_id("Visual Studio Code")); // name with spaces
        assert!(!is_bundle_id("")); // empty
        assert!(!is_bundle_id("foo.")); // trailing dot, one real segment
    }
}
