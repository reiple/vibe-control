// Application services + Tauri bridge for vibe-control
// Orchestrates domain + adapters, exposes Tauri commands.

mod claude;
mod usage_cw;

use std::collections::HashMap;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{Manager, State};

use claude::{ChatMsg, DEFAULT_MODEL};
use vc_core::{AppSettings, Resource, ResourceIdentity, ResourceKind, Result, WorkBundle};
use vc_sessions::{
    ClaudeCodeSessionProvider, CodingSessionProvider, SessionInfo, SessionProviderRegistry,
    SessionSnapshot,
};
use vc_store::{BundleStore, JsonBundleStore};

/// Cap on coding sessions pulled into a capture (most recent first).
const MAX_CAPTURED_SESSIONS: usize = 8;

pub struct AppState {
    store: Box<dyn BundleStore + Send>,
    bundles: Vec<WorkBundle>,
    /// Persisted app settings (layout + Claude connection). The API key lives
    /// here, in the OS config dir — never in the repo, never sent to the UI.
    settings: AppSettings,
    /// bundle id (or app name) → app icon as a `data:image/png;base64,…` URI.
    /// `None` is cached too, so a failed/absent extraction isn't retried on
    /// every request (NFR §9). Process-lifetime only — a restart re-extracts,
    /// which also self-heals icons that changed since (e.g. an app update).
    icon_cache: HashMap<String, Option<String>>,
    /// The most-recent Bedrock call's token counts, for the "last call" gauge.
    /// Process-lifetime only; the cumulative *daily* totals live in `settings`
    /// (persisted, keyed by local date). Never any prompt/response content.
    usage: UsageAccum,
}

/// Most-recent Bedrock call's token counts (transient), driving the KO-II
/// "last call" gauge. Holds only integers — never any content.
#[derive(Default, Clone, Copy)]
struct UsageAccum {
    last_input: u64,
    last_output: u64,
}

impl AppState {
    pub fn new() -> Result<Self> {
        let store = Box::new(JsonBundleStore::new()?);
        let bundles = store.load()?;
        // Corrupt settings must not brick the app — fall back to defaults.
        let settings = store.load_settings().unwrap_or_default();

        Ok(Self {
            store,
            bundles,
            settings,
            icon_cache: HashMap::new(),
            usage: UsageAccum::default(),
        })
    }

    /// Zero today's persisted counters when `today` differs from the stored
    /// date (a new local day). Returns whether anything changed (so the caller
    /// can persist). Does NOT save on its own.
    fn roll_usage_day(&mut self, today: &str) -> bool {
        if self.settings.usage_date.as_deref() == Some(today) {
            return false;
        }
        self.settings.usage_date = Some(today.to_string());
        self.settings.usage_input = 0;
        self.settings.usage_output = 0;
        self.settings.usage_requests = 0;
        true
    }

    /// Fold one Bedrock call's token counts into TODAY's persisted totals
    /// (rolling the day over first) and remember it as the most-recent call.
    pub fn add_usage(&mut self, input: u64, output: u64, today: &str) {
        self.roll_usage_day(today);
        self.settings.usage_input = self.settings.usage_input.saturating_add(input);
        self.settings.usage_output = self.settings.usage_output.saturating_add(output);
        self.settings.usage_requests = self.settings.usage_requests.saturating_add(1);
        self.usage.last_input = input;
        self.usage.last_output = output;
        // Best-effort persist: a save failure must not fail the reply.
        let _ = self.store.save_settings(&self.settings);
    }

    pub fn get_bundles(&self) -> &[WorkBundle] {
        &self.bundles
    }

    pub fn set_bundles(&mut self, bundles: Vec<WorkBundle>) -> Result<()> {
        self.bundles = bundles;
        self.store.save(&self.bundles)
    }

    /// Resolve the Bedrock bearer token: an explicitly-saved key wins, else the
    /// `AWS_BEARER_TOKEN_BEDROCK` env var (what Claude Code itself uses), else
    /// `ANTHROPIC_API_KEY`. GUI launches via LaunchServices don't inherit the
    /// shell env, so a saved key is the reliable path there. `None` if nothing
    /// yields a non-blank value.
    pub fn claude_key(&self) -> Option<String> {
        self.settings
            .claude_api_key
            .clone()
            .filter(|k| !k.trim().is_empty())
            .or_else(|| Self::env_nonblank("AWS_BEARER_TOKEN_BEDROCK"))
            .or_else(|| Self::env_nonblank("ANTHROPIC_API_KEY"))
    }

    /// The AWS region for the Bedrock endpoint: saved setting → `AWS_REGION`
    /// env → the app default.
    pub fn claude_region(&self) -> String {
        self.settings
            .claude_region
            .clone()
            .filter(|r| !r.trim().is_empty())
            .or_else(|| Self::env_nonblank("AWS_REGION"))
            .unwrap_or_else(|| claude::DEFAULT_REGION.to_string())
    }

    fn env_nonblank(name: &str) -> Option<String> {
        std::env::var(name).ok().filter(|v| !v.trim().is_empty())
    }

    /// Where the key came from, for the UI: "settings" | "env" | "none".
    pub fn key_source(&self) -> &'static str {
        if self
            .settings
            .claude_api_key
            .as_deref()
            .is_some_and(|k| !k.trim().is_empty())
        {
            "settings"
        } else if Self::env_nonblank("AWS_BEARER_TOKEN_BEDROCK").is_some()
            || Self::env_nonblank("ANTHROPIC_API_KEY").is_some()
        {
            "env"
        } else {
            "none"
        }
    }

    pub fn claude_model(&self) -> String {
        self.settings
            .claude_model
            .clone()
            .filter(|m| !m.trim().is_empty())
            // Self-heal legacy Anthropic-public ids (e.g. "claude-opus-5")
            // saved before the Bedrock pivot: they aren't valid Bedrock model
            // paths. Every Bedrock Anthropic inference-profile id contains
            // "anthropic.", so anything without it falls back to the default.
            .filter(|m| m.contains("anthropic."))
            .unwrap_or_else(|| DEFAULT_MODEL.to_string())
    }

    /// Save (or, with a blank value, clear) the Claude API key.
    pub fn set_claude_key(&mut self, key: Option<String>) -> Result<()> {
        self.settings.claude_api_key = key.filter(|k| !k.trim().is_empty());
        self.store.save_settings(&self.settings)
    }

    pub fn set_claude_model(&mut self, model: String) -> Result<()> {
        self.settings.claude_model = Some(model).filter(|m| !m.trim().is_empty());
        self.store.save_settings(&self.settings)
    }
}

type SharedState = Mutex<AppState>;

#[derive(Serialize)]
pub struct CommandError {
    message: String,
}

impl<E: std::fmt::Display> From<E> for CommandError {
    fn from(e: E) -> Self {
        CommandError {
            message: e.to_string(),
        }
    }
}

#[tauri::command]
fn get_bundles(state: State<SharedState>) -> std::result::Result<Vec<WorkBundle>, CommandError> {
    let app = state.lock().map_err(|_| CommandError {
        message: "state lock poisoned".into(),
    })?;
    Ok(app.get_bundles().to_vec())
}

#[tauri::command]
fn save_bundles(
    state: State<SharedState>,
    bundles: Vec<WorkBundle>,
) -> std::result::Result<(), CommandError> {
    let mut app = state.lock().map_err(|_| CommandError {
        message: "state lock poisoned".into(),
    })?;
    app.set_bundles(bundles)?;
    Ok(())
}

#[tauri::command]
async fn capture_current(name: String) -> std::result::Result<WorkBundle, CommandError> {
    let mut bundle = WorkBundle::new(name);

    // Running applications. The bundle id (when available) is a stable restore
    // target; the process name is only a fallback since it isn't always what
    // LaunchServices accepts (e.g. "Code" vs "Visual Studio Code").
    for (name, bundle_id) in enumerate_running_apps_detailed() {
        let target = bundle_id.as_deref().unwrap_or(name.as_str());
        bundle.add_resource(make_resource(&name, ResourceKind::AppLaunch, target, Some(target)));
    }

    // Open browser tabs.
    for (title, url) in enumerate_browser_tabs() {
        bundle.add_resource(make_resource(
            &title,
            ResourceKind::BrowserTab,
            &url,
            Some(&url),
        ));
    }

    // Active coding sessions (Claude Code).
    for session in discover_coding_sessions() {
        bundle.add_resource(make_resource(
            &session.label,
            ResourceKind::CodingSession,
            &session.session_ref,
            Some(&session.session_ref),
        ));
    }

    Ok(bundle)
}

fn make_resource(
    display_name: &str,
    kind: ResourceKind,
    descriptor: &str,
    reopen_info: Option<&str>,
) -> Resource {
    Resource::new(
        display_name.to_string(),
        kind,
        ResourceIdentity {
            kind,
            descriptor: descriptor.to_string(),
            hint: None,
            reopen_info: reopen_info.map(str::to_string),
        },
    )
}

fn enumerate_browser_tabs() -> Vec<(String, String)> {
    #[cfg(target_os = "macos")]
    {
        vc_os_macos::MacBrowserTabReader::read_tabs().unwrap_or_default()
    }
    #[cfg(target_os = "windows")]
    {
        vc_os_windows::WinBrowserTabReader::read_tabs().unwrap_or_default()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Vec::new()
    }
}

fn discover_coding_sessions() -> Vec<SessionInfo> {
    let mut sessions = ClaudeCodeSessionProvider.discover().unwrap_or_default();
    sessions.truncate(MAX_CAPTURED_SESSIONS);
    sessions
}

#[tauri::command]
async fn get_session_snapshot(
    tool_id: String,
    session_ref: String,
) -> std::result::Result<SessionSnapshot, CommandError> {
    let registry = SessionProviderRegistry::new();
    let provider = registry.provider(&tool_id).ok_or_else(|| CommandError {
        message: format!("unknown session tool: {}", tool_id),
    })?;
    Ok(provider.read_snapshot(&session_ref)?)
}

/// Outcome of a restore: which resources were reopened, which failed (with
/// reasons), and which were intentionally skipped, so the UI can show a
/// partial-success report.
#[derive(Serialize, Default)]
pub struct RestoreReport {
    opened: Vec<String>,
    failed: Vec<String>,
    skipped: Vec<String>,
}

/// Reopen the reopenable resources in a bundle (apps, folders, URLs/tabs),
/// best-effort — a failure on one never aborts the rest. Coding sessions are
/// deliberately NOT resumed here: that would spawn a Terminal even when one is
/// already open, so they're resumed individually via `resume_coding_session`.
#[tauri::command]
async fn restore_bundle(bundle: WorkBundle) -> std::result::Result<RestoreReport, CommandError> {
    let mut report = RestoreReport::default();
    for resource in &bundle.resources {
        if matches!(resource.kind, ResourceKind::CodingSession) {
            report.skipped.push(resource.display_name.clone());
            continue;
        }
        match reopen_resource(resource) {
            Ok(()) => report.opened.push(resource.display_name.clone()),
            Err(e) => report
                .failed
                .push(format!("{}: {}", resource.display_name, e)),
        }
    }
    Ok(report)
}

/// Resume a single coding session in a new Terminal window (explicit, per
/// session — the user chooses when to spawn a terminal).
#[tauri::command]
async fn resume_coding_session(session_ref: String) -> std::result::Result<(), CommandError> {
    resume_session(&session_ref)?;
    Ok(())
}

/// Bring the already-open Claude Code terminal for a session to the front
/// (do NOT spawn a new one — that's what `resume_coding_session` is for).
/// This is what the "대화 보기" button now does: jump to the live terminal so
/// the user reads/continues the real conversation there.
#[tauri::command]
async fn activate_coding_session(session_ref: String) -> std::result::Result<(), CommandError> {
    activate_session_terminal(&session_ref)?;
    Ok(())
}

/// One live window/tab of a running app (FR-2.8 / AC-20). Shown when the user
/// expands an app group in the left panel; `handle` is an opaque, per-platform
/// activation token (Windows: an `HWND`; macOS: `name\u{1f}title`) passed back
/// verbatim to `activate_window`, never interpreted by the frontend.
#[derive(Serialize)]
pub struct RunningWindow {
    handle: String,
    /// Distinguishable window/tab title shown in the expanded list (FR-2.8).
    title: String,
    /// Child kind: "tab" (browser), "folder" (Finder), or "window" (everything
    /// else). Drives the icon/register behaviour and which resource kind the
    /// child registers as (FR-2.2 per-tab, per-instance for Finder/others).
    kind: String,
    /// Value registered into a group when this child is dragged in: a URL for a
    /// tab, a POSIX path for a folder, the window handle for a window.
    target: String,
    /// Whether this is the current foreground window/tab — the green dot inside a
    /// multi-window group (FR-2.4).
    is_focused: bool,
}

/// A live running application for the left-hand panel (FR-2.1 / §13.1), grouped
/// with its individual windows so the UI can expand it and let the user pick a
/// specific window/tab to activate (FR-2.8 / AC-20). The app icon is shown once
/// per group; `windows` may be empty when only app-level info is available
/// (e.g. macOS without Accessibility), in which case the UI treats the app as a
/// single activatable entry (single-window consistency).
#[derive(Serialize)]
pub struct RunningApp {
    name: String,
    /// Stable launch target; the bundle id when available (§13.4).
    bundle_id: Option<String>,
    /// This app's live windows/tabs, grouped under the one icon (FR-2.2/2.8).
    windows: Vec<RunningWindow>,
}

/// List apps currently running on this machine, each grouped with its live
/// windows/tabs (FR-2.1 / FR-2.8 / §13.1).
#[tauri::command]
async fn list_running_apps() -> std::result::Result<Vec<RunningApp>, CommandError> {
    Ok(enumerate_running_windows()
        .into_iter()
        .map(|(name, bundle_id, windows)| RunningApp {
            name,
            bundle_id,
            windows: windows
                .into_iter()
                .map(|(handle, title, is_focused)| RunningWindow {
                    kind: "window".to_string(),
                    target: handle.clone(),
                    handle,
                    title,
                    is_focused,
                })
                .collect(),
        })
        .collect())
}

/// Bring an app to the front, launching it if closed (FR-2.6 / FR-4.x / §13.4).
/// `target` is a bundle id or an app name. Used for the app-level fallback
/// (no per-window info) and for closed-resource re-launch.
#[tauri::command]
async fn activate_app(target: String) -> std::result::Result<(), CommandError> {
    open_app(&target)?;
    Ok(())
}

/// Bring a SPECIFIC window/tab to the front (FR-2.8 / FR-4.1 / AC-20). `handle`
/// is the opaque token from a `RunningWindow` (Windows `HWND` / macOS
/// `name\u{1f}title`); it activates exactly that window, not just the app
/// (FR-4.2). Errors if the window was closed since the last poll.
#[tauri::command]
async fn activate_window(handle: String) -> std::result::Result<(), CommandError> {
    focus_window(&handle)?;
    Ok(())
}

/// Lazily enumerate one running app's expandable children when the user expands
/// it in the left panel (FR-2.2 / FR-2.8). Kept off the fast `list_running_apps`
/// path because the per-window/tab scan is slow and permission-gated. Returns
/// per-tab items for Safari/Chrome, per-window folders for Finder, and per-window
/// items for everything else (VS Code, Terminal, …) — each independently
/// registerable and activatable.
#[tauri::command]
async fn list_app_children(
    name: String,
    bundle_id: Option<String>,
) -> std::result::Result<Vec<RunningWindow>, CommandError> {
    Ok(enumerate_app_children(&name, bundle_id.as_deref())
        .into_iter()
        .map(|(kind, handle, title, target, is_focused)| RunningWindow {
            handle,
            title,
            kind,
            target,
            is_focused,
        })
        .collect())
}

/// Activate a specific child (tab/folder/window) picked from an expanded app
/// group (FR-2.8 / FR-4.1). Browser tabs are selected by URL; folders and
/// windows are raised by their opaque handle.
#[tauri::command]
async fn activate_child(
    kind: String,
    target: String,
    handle: String,
) -> std::result::Result<(), CommandError> {
    match kind.as_str() {
        // handle is `app\u{1f}url`; select the tab whose URL == target.
        "tab" => {
            let app = handle.split('\u{1f}').next().unwrap_or("Safari");
            activate_browser_tab(app, &target)?;
        }
        // Folders open in Finder by path; a path-less window (Recents / saved
        // search) has no path, so raise it by its window name from the handle.
        "folder" => {
            if target.is_empty() {
                let name = handle.split('\u{1f}').nth(1).unwrap_or("");
                activate_finder_window(name)?;
            } else {
                open_path(&target)?;
            }
        }
        _ => focus_window(&handle)?,
    }
    Ok(())
}

/// Return an app's icon as a `data:image/png;base64,…` URI at high resolution
/// (§13.5), cached in-memory (NFR §9). `bundle_id` is a bundle id or app name —
/// the same value passed to `activate_app`, so cache keys line up across
/// commands. Fetched lazily per item by the UI so `list_running_apps` never
/// blocks on icons. Returns `Ok(None)` when no icon is available (or on a
/// non-macOS platform), never an error.
#[tauri::command]
fn get_app_icon(
    state: State<SharedState>,
    bundle_id: String,
) -> std::result::Result<Option<String>, CommandError> {
    // 1) Fast path: check the cache under the lock, then release it.
    {
        let app = state.lock().map_err(|_| CommandError {
            message: "state lock poisoned".into(),
        })?;
        if let Some(cached) = app.icon_cache.get(&bundle_id) {
            return Ok(cached.clone()); // hit (including a cached `None`)
        }
    } // guard dropped here — lock released BEFORE the slow osascript call

    // 2) Miss: extract WITHOUT holding the lock (osascript is slow, and this
    //    lock guards every state command). A racing duplicate extract is
    //    harmless — same key, identical value, last writer wins.
    let icon = extract_app_icon(&bundle_id);

    // 3) Re-lock and insert.
    let mut app = state.lock().map_err(|_| CommandError {
        message: "state lock poisoned".into(),
    })?;
    app.icon_cache.insert(bundle_id, icon.clone());

    Ok(icon)
}

/// Extract a native app icon as a base64 PNG data URI (macOS + Windows);
/// `None` on other platforms or when no icon is available.
fn extract_app_icon(target: &str) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        vc_os_macos::MacIconReader::icon_data_uri(target)
    }
    #[cfg(target_os = "windows")]
    {
        vc_os_windows::WinIconReader::icon_data_uri(target)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = target;
        None
    }
}

/// Create a new empty context group (= work bundle) and persist it (§13.2).
/// Returns the full bundle list so the UI can replace its state.
#[tauri::command]
fn create_bundle(
    state: State<SharedState>,
    name: String,
) -> std::result::Result<Vec<WorkBundle>, CommandError> {
    let mut app = state.lock().map_err(|_| CommandError {
        message: "state lock poisoned".into(),
    })?;
    let mut bundles = app.get_bundles().to_vec();
    bundles.push(WorkBundle::new(name));
    app.set_bundles(bundles)?;
    Ok(app.get_bundles().to_vec())
}

/// Delete a context group by id (FR-1.1). Returns the updated bundle list.
#[tauri::command]
fn delete_bundle(
    state: State<SharedState>,
    bundle_id: String,
) -> std::result::Result<Vec<WorkBundle>, CommandError> {
    let mut app = state.lock().map_err(|_| CommandError {
        message: "state lock poisoned".into(),
    })?;
    let mut bundles = app.get_bundles().to_vec();
    bundles.retain(|b| b.id.to_string() != bundle_id);
    app.set_bundles(bundles)?;
    Ok(app.get_bundles().to_vec())
}

/// Remove a single resource from a context group by its id (the × on each item).
/// Returns the updated bundle list; a no-op if the id isn't found (already gone).
#[tauri::command]
fn remove_resource(
    state: State<SharedState>,
    bundle_id: String,
    resource_id: String,
) -> std::result::Result<Vec<WorkBundle>, CommandError> {
    let mut app = state.lock().map_err(|_| CommandError {
        message: "state lock poisoned".into(),
    })?;
    let mut bundles = app.get_bundles().to_vec();
    let bundle = bundles
        .iter_mut()
        .find(|b| b.id.to_string() == bundle_id)
        .ok_or_else(|| CommandError {
            message: format!("group not found: {bundle_id}"),
        })?;
    if let Some(rid) = bundle
        .resources
        .iter()
        .find(|r| r.id.to_string() == resource_id)
        .map(|r| r.id)
    {
        bundle.remove_resource(rid);
    }
    app.set_bundles(bundles)?;
    Ok(app.get_bundles().to_vec())
}

/// Add a running app (dragged from the left panel) to a context group (§13.3).
/// Deduplicates by (kind, descriptor) so the same app isn't added twice
/// (FR-3.4). Returns the updated bundle list.
#[tauri::command]
fn add_app_resource(
    state: State<SharedState>,
    bundle_id: String,
    name: String,
    target: String,
) -> std::result::Result<Vec<WorkBundle>, CommandError> {
    let mut app = state.lock().map_err(|_| CommandError {
        message: "state lock poisoned".into(),
    })?;
    let mut bundles = app.get_bundles().to_vec();
    let bundle = bundles
        .iter_mut()
        .find(|b| b.id.to_string() == bundle_id)
        .ok_or_else(|| CommandError {
            message: format!("group not found: {bundle_id}"),
        })?;

    // FR-3.4: don't register the same app twice in one group.
    let already = bundle.resources.iter().any(|r| {
        matches!(r.kind, ResourceKind::AppLaunch) && r.identity.descriptor == target
    });
    if !already {
        bundle.add_resource(make_resource(&name, ResourceKind::AppLaunch, &target, Some(&target)));
    }

    app.set_bundles(bundles)?;
    Ok(app.get_bundles().to_vec())
}

/// Add a specific app child (a browser tab, a Finder folder, or a single app
/// window) dragged from an expanded group into a context group (FR-2.2 / §13.3).
/// `kind` selects the resource kind: "tab" → browser tab (target = URL), "folder"
/// → folder (target = POSIX path), anything else → window ref (target = handle).
/// Deduplicated by (kind, descriptor) so the same tab/window isn't added twice
/// (FR-3.4). Returns the updated bundle list.
#[tauri::command]
fn add_child_resource(
    state: State<SharedState>,
    bundle_id: String,
    kind: String,
    name: String,
    target: String,
    handle: String,
) -> std::result::Result<Vec<WorkBundle>, CommandError> {
    let rkind = match kind.as_str() {
        "tab" => ResourceKind::BrowserTab,
        "folder" => ResourceKind::Folder,
        _ => ResourceKind::WindowRef,
    };
    // The child handle carries identity the bare target lacks: `app\u{1f}url` for a
    // browser tab (WHICH browser to drive so activation selects the exact tab) and
    // `Finder\u{1f}name` for a path-less Finder window (Recents / saved search,
    // raised by name). Pick the descriptor (dedup + display) and the reopen hint
    // (drives activation) per kind so clicking the registered item lands on it.
    let handle_or_target = if handle.is_empty() {
        target.as_str()
    } else {
        handle.as_str()
    };
    let (descriptor, reopen): (&str, &str) = match rkind {
        // URL identifies the tab for dedup; the `app\u{1f}url` handle drives it.
        ResourceKind::BrowserTab => (target.as_str(), handle_or_target),
        // POSIX path when present; else fall back to the `Finder\u{1f}name` handle
        // so a path-less window is still uniquely identified and re-raisable.
        ResourceKind::Folder if target.is_empty() => (handle.as_str(), handle.as_str()),
        ResourceKind::Folder => (target.as_str(), target.as_str()),
        // A window ref's target already IS its opaque handle.
        _ => (target.as_str(), handle_or_target),
    };

    let mut app = state.lock().map_err(|_| CommandError {
        message: "state lock poisoned".into(),
    })?;
    let mut bundles = app.get_bundles().to_vec();
    let bundle = bundles
        .iter_mut()
        .find(|b| b.id.to_string() == bundle_id)
        .ok_or_else(|| CommandError {
            message: format!("group not found: {bundle_id}"),
        })?;

    // FR-3.4: don't register the same child twice in one group.
    let already = bundle
        .resources
        .iter()
        .any(|r| r.kind == rkind && r.identity.descriptor == descriptor);
    if !already {
        bundle.add_resource(make_resource(&name, rkind, descriptor, Some(reopen)));
    }

    app.set_bundles(bundles)?;
    Ok(app.get_bundles().to_vec())
}

/// Connection status for the Claude prompt console. Deliberately never carries
/// the key itself — only whether one is configured and where it came from.
#[derive(Serialize)]
pub struct ClaudeStatus {
    configured: bool,
    /// "settings" | "env" | "none"
    source: String,
    model: String,
    /// AWS region backing the Bedrock endpoint.
    region: String,
}

fn claude_status_of(app: &AppState) -> ClaudeStatus {
    ClaudeStatus {
        configured: app.claude_key().is_some(),
        source: app.key_source().to_string(),
        model: app.claude_model(),
        region: app.claude_region(),
    }
}

/// Running Bedrock token usage for the on-screen meter. All counters are
/// process-lifetime cumulative except the `last_*` pair (the most recent call).
#[derive(Serialize)]
pub struct ClaudeUsage {
    configured: bool,
    model: String,
    /// cumulative
    input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
    requests: u64,
    /// the most recent call
    last_input: u64,
    last_output: u64,
    last_total: u64,
    /// model context window, for gauge scaling (fill = last_total / this)
    context_window: u64,
    /// true when the today totals are the whole account's CloudWatch usage
    /// (all models, this region); false when they're only this app's tally.
    account: bool,
    /// region the account totals were read from (empty when app-local).
    region: String,
}

fn claude_usage_of(app: &AppState) -> ClaudeUsage {
    let input = app.settings.usage_input;
    let output = app.settings.usage_output;
    let last = app.usage;
    ClaudeUsage {
        configured: app.claude_key().is_some(),
        model: app.claude_model(),
        input_tokens: input,
        output_tokens: output,
        total_tokens: input.saturating_add(output),
        requests: app.settings.usage_requests,
        last_input: last.last_input,
        last_output: last.last_output,
        last_total: last.last_input.saturating_add(last.last_output),
        // Opus 4.8 / Haiku 4.5 both expose a 200K-token context window.
        context_window: 200_000,
        account: false,
        region: String::new(),
    }
}

/// Read TODAY's token usage for the graphical meter (no content, no key).
/// `today` is the caller's local date (YYYY-MM-DD); a new day zeroes the local
/// counters. `since` is local midnight (epoch seconds), the window start for the
/// account-wide CloudWatch read. When AWS credentials are present the totals are
/// the whole account's Bedrock usage today (all models, this region); otherwise
/// they fall back to this app's local tally.
#[tauri::command]
async fn claude_usage(
    state: State<'_, SharedState>,
    today: String,
    since: i64,
) -> std::result::Result<ClaudeUsage, CommandError> {
    // Snapshot the local tally under the lock, then drop it before any network
    // await (a std Mutex guard isn't Send across .await).
    let (mut usage, configured, region) = {
        let mut app = state.lock().map_err(|_| CommandError {
            message: "state lock poisoned".into(),
        })?;
        if app.roll_usage_day(&today) {
            let _ = app.store.save_settings(&app.settings);
        }
        let region = app.claude_region();
        let configured = app.claude_key().is_some();
        (claude_usage_of(&app), configured, region)
    };

    // Prefer account-wide CloudWatch totals. Reconcile with max() so the readout
    // never dips below what this app already sent while CloudWatch (which has a
    // few minutes' latency) catches up — the account total always ⊇ this app.
    if configured {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(since);
        if now > since {
            if let Ok(acct) = usage_cw::fetch_today(Some(region.clone()), since, now).await {
                usage.input_tokens = acct.input.max(usage.input_tokens);
                usage.output_tokens = acct.output.max(usage.output_tokens);
                usage.total_tokens = usage.input_tokens.saturating_add(usage.output_tokens);
                usage.requests = acct.calls.max(usage.requests);
                usage.account = true;
                usage.region = region;
            }
        }
    }
    Ok(usage)
}

/// Report whether Claude is connected (key present) without revealing the key.
#[tauri::command]
fn claude_status(state: State<SharedState>) -> std::result::Result<ClaudeStatus, CommandError> {
    let app = state.lock().map_err(|_| CommandError {
        message: "state lock poisoned".into(),
    })?;
    Ok(claude_status_of(&app))
}

/// Save (or clear, when blank) the Bedrock API key locally. Write-only from
/// the UI: the value is never read back out.
#[tauri::command]
fn set_claude_api_key(
    state: State<SharedState>,
    key: String,
) -> std::result::Result<ClaudeStatus, CommandError> {
    let mut app = state.lock().map_err(|_| CommandError {
        message: "state lock poisoned".into(),
    })?;
    app.set_claude_key(Some(key))?;
    Ok(claude_status_of(&app))
}

/// Choose which Claude model the console uses.
#[tauri::command]
fn set_claude_model(
    state: State<SharedState>,
    model: String,
) -> std::result::Result<ClaudeStatus, CommandError> {
    let mut app = state.lock().map_err(|_| CommandError {
        message: "state lock poisoned".into(),
    })?;
    app.set_claude_model(model)?;
    Ok(claude_status_of(&app))
}

/// Send the console conversation to Claude and return the reply text. The key
/// and model are resolved server-side; the lock is released before the network
/// call so it never blocks other state commands (and can't be held across the
/// await — a std Mutex guard isn't Send).
#[tauri::command]
async fn send_claude_message(
    state: State<'_, SharedState>,
    messages: Vec<ChatMsg>,
    today: String,
) -> std::result::Result<String, CommandError> {
    let (key, region, model) = {
        let app = state.lock().map_err(|_| CommandError {
            message: "state lock poisoned".into(),
        })?;
        (app.claude_key(), app.claude_region(), app.claude_model())
    };
    let key = key.ok_or_else(|| CommandError {
        message:
            "No Bedrock API key configured. Add one in settings or set AWS_BEARER_TOKEN_BEDROCK."
                .into(),
    })?;
    let (reply, usage) = claude::send_message(&key, &region, &model, &messages).await?;
    // Record token usage for the on-screen meter (best-effort: a poisoned lock
    // must not fail an otherwise-successful reply).
    if let Ok(mut app) = state.lock() {
        app.add_usage(usage.input_tokens, usage.output_tokens, &today);
    }
    Ok(reply)
}

fn reopen_resource(resource: &Resource) -> std::result::Result<(), String> {
    // Prefer an explicit reopen hint; fall back to the raw descriptor.
    let target = resource
        .identity
        .reopen_info
        .as_deref()
        .unwrap_or(resource.identity.descriptor.as_str());

    match resource.kind {
        ResourceKind::AppLaunch => open_app(target),
        // A window ref's target is an opaque window handle (`app\u{1f}title\u{1f}idx`),
        // not an app name/bundle id: `focus_window` splits it to raise that exact
        // window (relaunching the app if it's closed), whereas `open_app` would
        // treat the whole handle as an app name and fail.
        ResourceKind::WindowRef => focus_window(target),
        // A folder's descriptor is a POSIX path, or a `Finder\u{1f}name` handle for
        // a path-less window (Recents / saved search) → raise that window by name.
        ResourceKind::Folder => match resource.identity.descriptor.split_once('\u{1f}') {
            Some((_, win_name)) => activate_finder_window(win_name),
            None => open_path(&resource.identity.descriptor),
        },
        // A tab's reopen hint is `app\u{1f}url` → select that exact tab in that
        // browser (not a fresh default-browser tab). Legacy resources stored only
        // the url (no separator) → best-effort open it.
        ResourceKind::BrowserTab => match target.split_once('\u{1f}') {
            Some((app_name, url)) => activate_browser_tab(app_name, url),
            None => open_url(target),
        },
        ResourceKind::Url => open_url(target),
        // Coding sessions resume from the raw descriptor (session path/id),
        // not the reopen hint.
        ResourceKind::CodingSession => resume_session(&resource.identity.descriptor),
    }
}

fn open_app(target: &str) -> std::result::Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        vc_os_macos::MacLauncher::open_app(target).map_err(|e| e.to_string())
    }
    #[cfg(target_os = "windows")]
    {
        vc_os_windows::WinLauncher::open_app(target).map_err(|e| e.to_string())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = target;
        Err("app launch not supported on this platform".into())
    }
}

/// Bring a specific window to the front by its opaque per-platform handle
/// (Windows `HWND` / macOS `name\u{1f}title`), the per-window counterpart of
/// `open_app` (FR-2.8 / FR-4.1).
fn focus_window(handle: &str) -> std::result::Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        vc_os_macos::MacLauncher::focus_window(handle).map_err(|e| e.to_string())
    }
    #[cfg(target_os = "windows")]
    {
        vc_os_windows::WinLauncher::focus_window(handle).map_err(|e| e.to_string())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = handle;
        Err("window activation not supported on this platform".into())
    }
}

fn open_path(target: &str) -> std::result::Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        vc_os_macos::MacLauncher::open_path(target).map_err(|e| e.to_string())
    }
    #[cfg(target_os = "windows")]
    {
        vc_os_windows::WinLauncher::open_path(target).map_err(|e| e.to_string())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = target;
        Err("open path not supported on this platform".into())
    }
}

/// Raise an open Finder window by name (macOS only). Used for Finder windows
/// with no filesystem path (Recents / saved searches) that `open_path` can't
/// reach.
fn activate_finder_window(name: &str) -> std::result::Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        vc_os_macos::MacWindowEnumerator::activate_finder_window(name).map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = name;
        Err("Finder window activation not supported on this platform".into())
    }
}

fn open_url(target: &str) -> std::result::Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        vc_os_macos::MacLauncher::open_url(target).map_err(|e| e.to_string())
    }
    #[cfg(target_os = "windows")]
    {
        vc_os_windows::WinLauncher::open_url(target).map_err(|e| e.to_string())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = target;
        Err("open url not supported on this platform".into())
    }
}

fn resume_session(session_ref: &str) -> std::result::Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let (cwd, id) = ClaudeCodeSessionProvider::resume_info(session_ref)
            .ok_or_else(|| "cannot resolve session cwd/id".to_string())?;
        let command = format!("cd {} && claude --resume {}", shell_quote(&cwd), id);
        vc_os_macos::MacLauncher::run_in_terminal(&command).map_err(|e| e.to_string())
    }
    #[cfg(target_os = "windows")]
    {
        let (cwd, id) = ClaudeCodeSessionProvider::resume_info(session_ref)
            .ok_or_else(|| "cannot resolve session cwd/id".to_string())?;
        // Build a PowerShell-valid command (the window we open is PowerShell, so
        // cmd's `cd /d ... &&` would be invalid there). `Set-Location` changes
        // dir, `;` sequences, and the cwd is PowerShell-single-quoted; the id is
        // a session UUID.
        let command = format!(
            "Set-Location -LiteralPath {}; claude --resume {}",
            ps_single_quote(&cwd),
            id
        );
        vc_os_windows::WinLauncher::run_in_terminal(&command).map_err(|e| e.to_string())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = session_ref;
        Err("session resume not supported on this platform".into())
    }
}

/// Bring the existing Terminal/PowerShell running this session to the front,
/// best-effort focusing the window whose title matches the session's
/// working-directory name. Never spawns a new terminal.
fn activate_session_terminal(session_ref: &str) -> std::result::Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let hint = ClaudeCodeSessionProvider::resume_info(session_ref).and_then(|(cwd, _)| {
            std::path::Path::new(&cwd)
                .file_name()
                .and_then(|s| s.to_str())
                .map(str::to_string)
        });
        vc_os_macos::MacLauncher::activate_terminal(hint.as_deref()).map_err(|e| e.to_string())
    }
    #[cfg(target_os = "windows")]
    {
        let hint = ClaudeCodeSessionProvider::resume_info(session_ref).and_then(|(cwd, _)| {
            std::path::Path::new(&cwd)
                .file_name()
                .and_then(|s| s.to_str())
                .map(str::to_string)
        });
        vc_os_windows::WinLauncher::activate_terminal(hint.as_deref()).map_err(|e| e.to_string())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = session_ref;
        Err("terminal activation not supported on this platform".into())
    }
}

/// Single-quote a string for safe use in a POSIX shell command.
#[cfg(target_os = "macos")]
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Single-quote a string for safe embedding in a PowerShell command (a literal
/// single quote is escaped by doubling it).
#[cfg(target_os = "windows")]
fn ps_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn enumerate_running_apps_detailed() -> Vec<(String, Option<String>)> {
    #[cfg(target_os = "macos")]
    {
        vc_os_macos::MacWindowEnumerator::list_running_apps().unwrap_or_default()
    }
    #[cfg(target_os = "windows")]
    {
        vc_os_windows::WinWindowEnumerator::list_running_apps().unwrap_or_default()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Vec::new()
    }
}

/// One app grouped with its live windows:
/// `(name, bundle_id, [(handle, title, is_focused)])`.
type AppWindows = (String, Option<String>, Vec<(String, String, bool)>);

/// Enumerate running apps grouped with their individual windows/tabs (FR-2.8 /
/// AC-20). Empty on unsupported platforms; per-adapter failures degrade to an
/// empty vec.
fn enumerate_running_windows() -> Vec<AppWindows> {
    #[cfg(target_os = "macos")]
    {
        // App-level only (empty windows): the per-window/tab scan is slow and
        // permission-gated (`System Events`), and eagerly running it here stalls
        // the whole left panel for a minute+ when one app ignores Apple Events.
        // The UI fetches an app's children lazily via `list_app_children` when
        // the user expands it, so the running-apps list stays instant.
        vc_os_macos::MacWindowEnumerator::list_running_apps()
            .unwrap_or_default()
            .into_iter()
            .map(|(name, bundle_id)| (name, bundle_id, Vec::new()))
            .collect()
    }
    #[cfg(target_os = "windows")]
    {
        vc_os_windows::WinWindowEnumerator::list_running_windows().unwrap_or_default()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Vec::new()
    }
}

/// One expandable child of an app: `(kind, handle, title, target, is_focused)`.
type AppChild = (String, String, String, String, bool);

/// Lazily enumerate one app's children (tabs/folders/windows). Empty on
/// unsupported platforms or per-adapter failure.
fn enumerate_app_children(name: &str, bundle_id: Option<&str>) -> Vec<AppChild> {
    #[cfg(target_os = "macos")]
    {
        vc_os_macos::MacWindowEnumerator::list_app_children(name, bundle_id)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (name, bundle_id);
        Vec::new()
    }
}

/// Focus a specific browser tab by URL (per-tab activation, FR-2.2 / FR-4.1).
fn activate_browser_tab(app: &str, url: &str) -> std::result::Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        vc_os_macos::MacLauncher::activate_browser_tab(app, url).map_err(|e| e.to_string())
    }
    #[cfg(target_os = "windows")]
    {
        vc_os_windows::WinLauncher::activate_browser_tab(app, url).map_err(|e| e.to_string())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (app, url);
        Err("browser tab activation not supported on this platform".into())
    }
}

/// Request (and report) macOS Accessibility permission — required for the
/// `System Events` pass behind per-instance window listing (VS Code / Terminal /
/// any non-browser, non-Finder app). Prompts the one-time system dialog when not
/// yet granted; returns whether permission is currently held. Safe to call
/// repeatedly. On non-macOS platforms there is no gate, so returns `true`.
#[tauri::command]
fn request_accessibility() -> bool {
    vc_os_macos::accessibility_trusted(true)
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let state = AppState::new().map_err(|e| e.to_string())?;
            app.manage(Mutex::new(state));
            // Surface the Accessibility prompt at launch so per-instance window
            // listing works without the user having to hunt through Settings.
            // Unscriptable to grant; only prompts when not already trusted.
            let _ = vc_os_macos::accessibility_trusted(true);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_bundles,
            save_bundles,
            capture_current,
            get_session_snapshot,
            restore_bundle,
            resume_coding_session,
            activate_coding_session,
            list_running_apps,
            list_app_children,
            activate_app,
            activate_window,
            activate_child,
            get_app_icon,
            create_bundle,
            delete_bundle,
            remove_resource,
            add_app_resource,
            add_child_resource,
            claude_status,
            claude_usage,
            set_claude_api_key,
            set_claude_model,
            send_claude_message,
            request_accessibility
        ])
        .run(tauri::generate_context!())
        .expect("error while running vibe-control");
}
