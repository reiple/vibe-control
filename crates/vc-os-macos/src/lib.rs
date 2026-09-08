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
    pub fn list_running_apps() -> Result<Vec<(String, Option<String>)>> {
        // tab-separated `name<TAB>bundleid` per line; bundle id may be empty.
        let script = "tell application \"System Events\"\n\
             set out to \"\"\n\
             repeat with p in (every process whose background only is false)\n\
             set bid to \"\"\n\
             try\n\
             set bid to bundle identifier of p\n\
             end try\n\
             set out to out & (name of p) & tab & bid & linefeed\n\
             end repeat\n\
             return out\n\
             end tell";
        let Some(out) = run_osascript(script) else {
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
        let script = format!(
            "tell application \"{app}\"\n\
             set out to \"\"\n\
             repeat with w in windows\n\
             repeat with t in tabs of w\n\
             set out to out & ({title_prop} of t) & tab & (URL of t) & linefeed\n\
             end repeat\n\
             end repeat\n\
             return out\n\
             end tell"
        );
        let Some(out) = run_osascript(&script) else {
            return vec![];
        };
        out.lines()
            .filter_map(|line| {
                let (title, url) = line.split_once('\t')?;
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
