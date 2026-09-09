// Application services + Tauri bridge for vibe-control
// Orchestrates domain + adapters, exposes Tauri commands.

mod claude;
mod pty;
mod status_query;
mod term;
mod usage_cw;

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{Manager, State};

use claude::{ChatMsg, DEFAULT_MODEL};
use vc_core::{
    aggregate_context_status, delivery_block_reason, delivery_status_after_resume, resolve_sessions,
    select_delivery_target, AnalysisCache, AppSettings, CacheEntry, CommandDelivery,
    ContextClaudeStatus, CurrentWork, DeliveryStatus, DeliveryTarget, ProcessLivenessProbe, Resource,
    ResourceIdentity, ResourceKind, Result, SessionCompletion, SessionResolution, SessionRunState,
    SummarizationOutcome, SummarizationRequest, WorkBundle, WorkSummarizer,
};
use vc_sessions::{
    ClaudeCodeSessionProvider, CodingSessionProvider, SessionInfo, SessionProviderRegistry,
    SessionSnapshot,
};
use vc_store::{BundleStore, JsonBundleStore};

/// Machine-level liveness probe result is cached for this long to avoid spawning
/// a `ps`/`tasklist` subprocess on every ~1s status poll (design decision Q2=A).
const PROBE_TTL_SECS: u64 = 3;

/// Cap on coding sessions pulled into a capture (most recent first).
const MAX_CAPTURED_SESSIONS: usize = 8;

/// Unit separator joining the parts of a saved live-tab hint
/// (`<browser>\u{1f}<hwnd>\u{1f}<idx>`). The `<hwnd>\u{1f}<idx>` suffix is
/// exactly the token the Windows tab reader emits/consumes; we only prefix the
/// browser name so activation knows which process to re-enumerate on a stale
/// token. Matches the separator the adapters already use for opaque handles.
const TAB_SEP: char = '\u{1f}';

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
    // --- U2 Status Query state (all additive; the BundleStore trait is unchanged) ---
    /// Concrete store handle so status queries can reach the incremental-analysis
    /// cache (`load/save_analysis_cache` are inherent methods, not on the trait —
    /// design decision Q4=A). Same on-disk location as `store`.
    analysis_store: JsonBundleStore,
    /// In-memory authoritative copy of the analysis cache (NFR-1.2, Q2=A
    /// write-back). Flushed to disk best-effort; correctness never depends on it.
    analysis_cache: AnalysisCache,
    /// True when `analysis_cache` has unpersisted changes awaiting a flush.
    analysis_cache_dirty: bool,
    /// Memoized machine-level liveness probe result `(value, unix_seconds)` with a
    /// `PROBE_TTL_SECS` TTL, so the ~1s poll doesn't spawn a subprocess every tick.
    probe_cache: Option<(Option<bool>, u64)>,
    /// Session ids with a background summarization in flight, so the per-second
    /// poll never spawns a duplicate summary for the same session (Q2=A dedup).
    in_flight: HashSet<String>,
    /// In-memory memo of each session's last parsed completion state, so the warm
    /// (unchanged-file) fast-path can derive a run state without re-parsing the
    /// log. Process-lifetime only; empty at startup degrades to `Unknown`.
    last_completion: HashMap<String, SessionCompletion>,
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
        // One concrete store, cloned into the trait object. The concrete handle
        // is kept so status queries can reach `load/save_analysis_cache` (Q4=A);
        // `Clone` on JsonBundleStore is a cheap PathBuf copy to the same dir.
        let concrete = JsonBundleStore::new()?;
        let bundles = concrete.load()?;
        // Corrupt settings must not brick the app — fall back to defaults.
        let settings = concrete.load_settings().unwrap_or_default();
        // The analysis cache is non-critical: a missing/corrupt file loads empty.
        let analysis_cache = concrete.load_analysis_cache();
        let store: Box<dyn BundleStore + Send> = Box::new(concrete.clone());

        Ok(Self {
            store,
            bundles,
            settings,
            icon_cache: HashMap::new(),
            analysis_store: concrete,
            analysis_cache,
            analysis_cache_dirty: false,
            probe_cache: None,
            in_flight: HashSet::new(),
            last_completion: HashMap::new(),
            usage: UsageAccum::default(),
        })
    }

    /// Whether the user has opted in to sending session content for summarization
    /// (§12 / NFR-4). Gates every external summarization call.
    pub fn summarization_consent(&self) -> bool {
        self.settings.session_summary_consent
    }

    /// Set (and persist) the summarization consent flag.
    pub fn set_summarization_consent(&mut self, enabled: bool) -> Result<()> {
        self.settings.session_summary_consent = enabled;
        self.store.save_settings(&self.settings)
    }

    /// Flush the in-memory analysis cache to disk if dirty. Best-effort
    /// (NFR-1.2): a write failure is non-fatal — the cache is reconstructible.
    fn flush_analysis_cache(&mut self) {
        if self.analysis_cache_dirty {
            let _ = self.analysis_store.save_analysis_cache(&self.analysis_cache);
            self.analysis_cache_dirty = false;
        }
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

    /// The persisted UI layout (sidebar width, card height, saved window rect),
    /// with the Claude fields deliberately excluded — this is safe to hand to
    /// the frontend (E2 / FR-8.10 / AC-14).
    pub fn layout(&self) -> LayoutSettings {
        LayoutSettings {
            panel_width: self.settings.panel_width,
            card_height: self.settings.card_height,
            window_x: self.settings.window_x,
            window_y: self.settings.window_y,
            window_width: self.settings.window_width,
            window_height: self.settings.window_height,
        }
    }

    /// Persist the panel/card layout the frontend owns (sidebar drag-resize).
    /// The window rect is written by the backend's window-event path, so it is
    /// left untouched here.
    pub fn set_panel_layout(&mut self, panel_width: f32, card_height: f32) -> Result<()> {
        // Guard against a degenerate 0/NaN width bricking the sidebar on reload.
        if panel_width.is_finite() && panel_width > 0.0 {
            self.settings.panel_width = panel_width;
        }
        if card_height.is_finite() && card_height > 0.0 {
            self.settings.card_height = card_height;
        }
        self.store.save_settings(&self.settings)
    }

    /// Record the current window position/size in memory only (no disk write) —
    /// called on every move/resize event, which fire rapidly during a drag.
    /// The values are flushed to disk once by `persist_settings` on window close.
    pub fn stash_window_rect(&mut self, x: i32, y: i32, width: u32, height: u32) {
        self.settings.window_x = Some(x);
        self.settings.window_y = Some(y);
        self.settings.window_width = Some(width);
        self.settings.window_height = Some(height);
    }

    /// Flush the in-memory settings (including the stashed window rect) to disk.
    pub fn persist_settings(&self) -> Result<()> {
        self.store.save_settings(&self.settings)
    }
}

/// UI layout the frontend reads on boot and writes back on change (E2). Excludes
/// the Claude connection fields of `AppSettings`, which the UI must never see.
#[derive(Serialize, serde::Deserialize, Clone)]
pub struct LayoutSettings {
    pub panel_width: f32,
    pub card_height: f32,
    pub window_x: Option<i32>,
    pub window_y: Option<i32>,
    pub window_width: Option<u32>,
    pub window_height: Option<u32>,
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

/// List a running browser's currently-open tabs as individual selectable
/// sessions (FR-9.6 / FR-10.12 / AC-20). `name` is the app-group name the UI
/// already has (the browser's process/exe stem, e.g. `chrome` / `msedge`). Each
/// returned `RunningWindow` is one tab: `handle` an opaque per-tab activation
/// token (never interpreted by the frontend), `title` the tab's page title,
/// `is_focused` the active tab (green dot). Fetched ON DEMAND when the user
/// expands a browser group — NEVER on the 1-second poll (FR-10.7) — since it
/// reads the browser's accessibility tree, which is heavier than window
/// enumeration. Returns empty (UI falls back to the OS-window list) when no
/// browser window's tabs are readable — e.g. a fully-background window whose
/// accessibility tree the browser hasn't built, or an unsupported platform.
///
/// `reveal` distinguishes the two entry points (FR-10.7 / accessibility-tree
/// readiness): group-expand passes `false` — a background window is read
/// best-effort with bounded retries but is NEVER forced to the foreground, so
/// merely opening a group can't steal focus. The explicit "bring forward and
/// re-read" action passes `true`, foregrounding each browser window first,
/// which is the reliable way to make Chromium build a background window's tree.
#[tauri::command]
async fn list_browser_tabs(
    name: String,
    reveal: bool,
) -> std::result::Result<Vec<RunningWindow>, CommandError> {
    Ok(enumerate_browser_tab_sessions(&name, reveal)
        .into_iter()
        .map(|(handle, title, is_focused)| RunningWindow {
            handle,
            title,
            // A browser tab session; `target` is intentionally empty — a live tab
            // carries no URL (FR-9.7 forbids guessing one), it is focus-only.
            kind: "tab".into(),
            target: String::new(),
            is_focused,
        })
        .collect())
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

/// Bring a SPECIFIC browser tab to the front (FR-2.8 / FR-4.1 / FR-4.2, AC-20):
/// the per-tab counterpart of `activate_window`. `handle` is the opaque token
/// from a `list_browser_tabs` entry. It focuses exactly that tab (foregrounding
/// its window first), not just the browser app; errors if the tab or window is
/// gone since the last fetch so the UI can drop it and re-enumerate.
#[tauri::command]
async fn activate_tab(handle: String) -> std::result::Result<(), CommandError> {
    focus_tab(&handle)?;
    Ok(())
}

/// Activate a SAVED live browser tab (FR-4.1 / FR-4.2 / AC-20): the double-click
/// counterpart of `add_tab_resource`. `hint` is the stored
/// `<browser>\u{1f}<hwnd>\u{1f}<idx>` token, `title` the saved tab title used as
/// a fallback match key when the token has gone stale. Focuses exactly that tab,
/// never just the browser app; errors when the tab can no longer be found.
#[tauri::command]
async fn activate_tab_resource(
    hint: String,
    title: String,
) -> std::result::Result<(), CommandError> {
    activate_live_tab(&hint, &title)?;
    Ok(())
}

/// Activate a SAVED individual window (FR-2.8 / FR-4.1 / FR-4.2 / AC-20): the
/// double-click counterpart of `add_window_resource`. `hint` is the stored HWND
/// token, `title` the saved window title, `app` the owning app-group name (used
/// to scope the re-match). Focuses exactly that window — never just the app —
/// and errors when the window can no longer be found. Closed windows are NOT
/// auto-reopened (out of scope): a gone window surfaces as an error.
#[tauri::command]
async fn activate_window_resource(
    hint: String,
    title: String,
    app: String,
) -> std::result::Result<(), CommandError> {
    activate_live_window(&hint, &title, &app)?;
    Ok(())
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

/// Find an 8-4-4-4-12 hex UUID token anywhere in a terminal window title.
/// Claude Code resumes launched by this app show `claude --resume <uuid>` in
/// the title, so this exactly identifies the session when present.
#[cfg(target_os = "macos")]
fn session_id_from_title(title: &str) -> Option<String> {
    fn is_uuid(tok: &str) -> bool {
        let b = tok.as_bytes();
        if b.len() != 36 {
            return false;
        }
        b.iter().enumerate().all(|(i, &c)| match i {
            8 | 13 | 18 | 23 => c == b'-',
            _ => c.is_ascii_hexdigit(),
        })
    }
    title
        .split(|c: char| c.is_whitespace())
        .find(|tok| is_uuid(tok))
        .map(str::to_string)
}

/// If a dragged window is actually an open Claude Code terminal, resolve it to
/// its session transcript so it can register as a live coding session (with a
/// conversation preview + 대화 보기) instead of a bare window ref. Returns the
/// session's `(label, session_ref)`. Best-effort — `None` when the window isn't
/// a recognizable coding session, so the caller falls back to a plain window.
///
/// Two join keys, most precise first: (1) a session id embedded in the terminal
/// title (how our own resumes launch), (2) the terminal's working directory
/// matched to the most-recently-active session started there.
#[cfg(target_os = "macos")]
fn resolve_terminal_session(handle: &str, title: &str) -> Option<(String, String)> {
    let mut parts = handle.split('\u{1f}');
    let app = parts.next().unwrap_or("");
    let _title = parts.next();
    let idx: usize = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);

    // discover() returns sessions most-recently-modified first, so the first
    // match on either key is the active session for that terminal.
    let sessions = ClaudeCodeSessionProvider.discover().unwrap_or_default();

    // (1) Exact: session id in the title.
    if let Some(id) = session_id_from_title(title) {
        let needle = format!("{id}.jsonl");
        if let Some(s) = sessions.iter().find(|s| s.session_ref.ends_with(&needle)) {
            return Some((s.label.clone(), s.session_ref.clone()));
        }
    }

    // (2) Robust: match the terminal's cwd to a session's cwd.
    let cwd = vc_os_macos::terminal_window_cwd(app, title, idx)?;
    let cwd_canon = std::fs::canonicalize(&cwd).ok();
    for s in &sessions {
        let Some(scwd) = ClaudeCodeSessionProvider::session_cwd(&s.session_ref) else {
            continue;
        };
        let same = match (&cwd_canon, std::fs::canonicalize(&scwd).ok()) {
            (Some(a), Some(b)) => *a == b,
            _ => scwd == cwd,
        };
        if same {
            return Some((s.label.clone(), s.session_ref.clone()));
        }
    }
    None
}

/// Return the persisted UI layout so the frontend can restore the sidebar width
/// on boot (E2 / FR-8.10 / AC-14). Never includes Claude credentials.
#[tauri::command]
fn get_layout(state: State<SharedState>) -> std::result::Result<LayoutSettings, CommandError> {
    let app = state.lock().map_err(|_| CommandError {
        message: "state lock poisoned".into(),
    })?;
    Ok(app.layout())
}

/// Persist the sidebar/card layout the user adjusted (debounced by the UI).
#[tauri::command]
fn save_panel_layout(
    state: State<SharedState>,
    panel_width: f32,
    card_height: f32,
) -> std::result::Result<(), CommandError> {
    let mut app = state.lock().map_err(|_| CommandError {
        message: "state lock poisoned".into(),
    })?;
    app.set_panel_layout(panel_width, card_height)?;
    Ok(())
}

/// Add ONE live browser tab (dragged from the left panel) to a context group as
/// its own focus-only resource (FR-9.6 / FR-10.12 / AC-20). Unlike a captured
/// `BrowserTab` (which persists a URL), a live tab carries NO URL — UIA can't
/// read a background tab's URL and FR-9.7 forbids guessing one — so it is
/// display+activate only: `display_name`/`descriptor` = the tab title, `hint` =
/// `<browser>\u{1f}<hwnd>\u{1f}<idx>` (the non-stable activation token),
/// `reopen_info` = None. `browser` is the app-group name (browser exe stem, e.g.
/// `chrome`), `handle` the per-tab token from a `list_browser_tabs` entry.
/// Dedup is on the full hint, so the SAME live tab can't be registered twice
/// while two DIFFERENT tabs of one window (distinct index) both can (FR-3.4).
/// Returns the updated bundle list.
#[tauri::command]
fn add_tab_resource(
    state: State<SharedState>,
    bundle_id: String,
    title: String,
    browser: String,
    handle: String,
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

    let hint = format!("{browser}{TAB_SEP}{handle}");
    // FR-3.4: the exact same live tab (browser + window + tab index) can't be
    // registered twice; two different tabs of one window (different index) still
    // both register, since the whole hint differs.
    let already = bundle.resources.iter().any(|r| {
        matches!(r.kind, ResourceKind::BrowserTabLive)
            && r.identity.hint.as_deref() == Some(hint.as_str())
    });
    if !already {
        bundle.add_resource(Resource::new(
            title.clone(),
            ResourceKind::BrowserTabLive,
            ResourceIdentity {
                kind: ResourceKind::BrowserTabLive,
                descriptor: title,
                hint: Some(hint),
                reopen_info: None,
            },
        ));
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
    // A dragged window that is actually an open Claude Code terminal registers
    // as a live CodingSession (conversation preview + 대화 보기) — the same kind
    // "Capture current" produces — rather than a bare window ref. Best-effort;
    // falls through to the normal kind mapping when it isn't a coding session.
    #[cfg(target_os = "macos")]
    let session = (kind != "tab" && kind != "folder")
        .then(|| resolve_terminal_session(&handle, &name))
        .flatten();
    #[cfg(not(target_os = "macos"))]
    let session: Option<(String, String)> = None;

    // (display_name, kind, descriptor, reopen). For a coding session the label
    // and .jsonl path come from the session, matching capture_current so the two
    // paths dedup together. Otherwise the child handle carries identity the bare
    // target lacks: `app\u{1f}url` for a browser tab (WHICH browser to drive) and
    // `Finder\u{1f}name` for a path-less Finder window (raised by name).
    let (display_name, rkind, descriptor, reopen): (String, ResourceKind, String, String) =
        if let Some((label, session_ref)) = session {
            (label, ResourceKind::CodingSession, session_ref.clone(), session_ref)
        } else {
            let rkind = match kind.as_str() {
                "tab" => ResourceKind::BrowserTab,
                "folder" => ResourceKind::Folder,
                _ => ResourceKind::WindowRef,
            };
            let handle_or_target = if handle.is_empty() {
                target.as_str()
            } else {
                handle.as_str()
            };
            let (descriptor, reopen): (&str, &str) = match rkind {
                // URL identifies the tab for dedup; the `app\u{1f}url` handle drives it.
                ResourceKind::BrowserTab => (target.as_str(), handle_or_target),
                // POSIX path when present; else fall back to the `Finder\u{1f}name`
                // handle so a path-less window is uniquely identified and re-raisable.
                ResourceKind::Folder if target.is_empty() => (handle.as_str(), handle.as_str()),
                ResourceKind::Folder => (target.as_str(), target.as_str()),
                // A window ref's target already IS its opaque handle.
                _ => (target.as_str(), handle_or_target),
            };
            (name.clone(), rkind, descriptor.to_string(), reopen.to_string())
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
        bundle.add_resource(make_resource(&display_name, rkind, &descriptor, Some(&reopen)));
    }

    app.set_bundles(bundles)?;
    Ok(app.get_bundles().to_vec())
}

/// Add ONE individual OS window (dragged from the left panel) to a context group
/// as a focus-only `WindowRef` resource (FR-2.8 / FR-3.5 / AC-20) — e.g. a single
/// KakaoTalk chat-room window rather than the whole app. This complements
/// whole-app registration (`add_app_resource`) and browser-tab registration
/// (`add_tab_resource`), and works from BOTH kinds of left-panel window rows: a
/// general app's OS windows and the browser's OS-window fallback rows (shown when
/// tab reading fails). `title` = the window title (display + descriptor), `app` =
/// the owning app-group name (kept in `reopen_info` to scope re-matching), `handle`
/// = the volatile HWND token (kept in `hint`, used only after confirming identity).
/// Dedup is on the title (consistent with vc-core `distinct_key(WindowRef)`), since
/// the OS recycles HWNDs so they can't be part of the identity key. Closed windows
/// are NOT auto-reopened — that's deliberately out of scope. Returns the updated
/// bundle list.
#[tauri::command]
fn add_window_resource(
    state: State<SharedState>,
    bundle_id: String,
    title: String,
    app: String,
    handle: String,
) -> std::result::Result<Vec<WorkBundle>, CommandError> {
    let mut guard = state.lock().map_err(|_| CommandError {
        message: "state lock poisoned".into(),
    })?;
    let mut bundles = guard.get_bundles().to_vec();
    let bundle = bundles
        .iter_mut()
        .find(|b| b.id.to_string() == bundle_id)
        .ok_or_else(|| CommandError {
            message: format!("group not found: {bundle_id}"),
        })?;

    // FR-3.4/3.5: dedup on the window title within the group, consistent with
    // vc-core `distinct_key(WindowRef) = descriptor`. The HWND is volatile (the
    // OS recycles it), so it is NEVER part of the identity key.
    let already = bundle
        .resources
        .iter()
        .any(|r| matches!(r.kind, ResourceKind::WindowRef) && r.identity.descriptor == title);
    if !already {
        bundle.add_resource(Resource::new(
            title.clone(),
            ResourceKind::WindowRef,
            ResourceIdentity {
                kind: ResourceKind::WindowRef,
                descriptor: title,
                hint: Some(handle),
                reopen_info: Some(app),
            },
        ));
    }

    guard.set_bundles(bundles)?;
    Ok(guard.get_bundles().to_vec())
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

/// Current wall-clock in unix seconds (0 if the clock is before the epoch —
/// never panics). Read once per query and passed as a value into the pure
/// domain logic (NFR-8 determinism).
fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// A queued background summarization, built during the (locked) status pass and
/// spawned afterwards so no LLM work happens on the command's return path.
struct Dispatch {
    session_id: String,
    context_ref: String,
    request: SummarizationRequest,
    /// Resume point (file size + mtime) to persist when the summary lands.
    new_offset: u64,
    new_mtime: u64,
}

/// Resolve machine-level Claude liveness, memoized with a `PROBE_TTL_SECS` TTL.
/// The lock is only held to read/write the small cache; the (slow) `ps`/
/// `tasklist` subprocess runs WITHOUT the lock (mirrors `get_app_icon`).
fn resolve_liveness(state: &State<SharedState>, now: u64) -> Option<bool> {
    // Fast path: a fresh cached probe.
    if let Ok(app) = state.lock() {
        if let Some((value, at)) = app.probe_cache {
            if now.saturating_sub(at) < PROBE_TTL_SECS {
                return value;
            }
        }
    }
    // Miss: probe OUTSIDE the lock, then store.
    let value = status_query::SystemProbe.any_claude_process_running();
    if let Ok(mut app) = state.lock() {
        app.probe_cache = Some((value, now));
    }
    value
}

/// Query the aggregated Claude Code status for a Context (= work bundle) (FR-2).
///
/// Non-blocking (design decision Q1=A): this returns the current status
/// immediately from cached summaries; when a session has fresh content and
/// consent is granted, a summarization is dispatched to the background and its
/// result rides the *next* poll. Total (NFR-2): every failure degrades to
/// `Unknown`/`Inactive`/empty rather than erroring, so the command effectively
/// always returns `Ok` (U2-P6). The std Mutex guard is never held across an
/// `.await` (the only awaited work is inside detached spawned tasks).
#[tauri::command]
async fn get_context_claude_status(
    app: tauri::AppHandle,
    state: State<'_, SharedState>,
    context_ref: String,
) -> std::result::Result<ContextClaudeStatus, CommandError> {
    let now = unix_now();

    // Phase 1: read config + resolve the bundle under a short lock.
    let (bundle, consent, key_opt, region, model) = {
        let app_state = state.lock().map_err(|_| CommandError {
            message: "state lock poisoned".into(),
        })?;
        let bundle = app_state
            .get_bundles()
            .iter()
            .find(|b| b.id.to_string() == context_ref)
            .cloned();
        (
            bundle,
            app_state.summarization_consent(),
            app_state.claude_key(),
            app_state.claude_region(),
            app_state.claude_model(),
        )
    };
    let key_present = key_opt.is_some();

    let Some(bundle) = bundle else {
        // Unknown Context — report cleanly, never error (FR-1 / NFR-2).
        return Ok(aggregate_context_status(
            context_ref,
            SessionResolution::ContextNotFound,
            vec![],
        ));
    };

    let resolution = resolve_sessions(&bundle);
    let refs: Vec<String> = match &resolution {
        SessionResolution::Resolved(r) => vec![r.clone()],
        SessionResolution::Multiple(v) => v.clone(),
        // NoSession / Ambiguous / ContextNotFound → no sessions to query.
        _ => vec![],
    };

    // Phase 2: one machine-level liveness probe for the whole query (TTL-cached).
    let is_running = resolve_liveness(&state, now);

    // Phase 3: per-session status assembly + summarization gating.
    let mut statuses = Vec::with_capacity(refs.len());
    let mut dispatches: Vec<Dispatch> = Vec::new();

    for session_ref in &refs {
        let (working_directory, session_id) = ClaudeCodeSessionProvider::resume_info(session_ref)
            .unwrap_or_else(|| (String::new(), session_ref.clone()));

        // Read this session's cache/memo/in-flight state under a short lock.
        let (prev_offset, prev_mtime, cached_summary, analyzed_at, in_flight_already, memo) = {
            let app_state = state.lock().map_err(|_| CommandError {
                message: "state lock poisoned".into(),
            })?;
            let (o, m, cs, a) = match app_state.analysis_cache.get(&session_id) {
                Some(e) => (
                    e.last_analyzed_offset,
                    e.last_analyzed_mtime,
                    e.cached_summary.clone(),
                    e.analyzed_at,
                ),
                None => (0, 0, None, 0),
            };
            (
                o,
                m,
                cs,
                a,
                app_state.in_flight.contains(&session_id),
                app_state.last_completion.get(&session_id).copied(),
            )
        };

        // Incremental read (read-only FS; NFR-3). `unchanged` distinguishes the
        // warm fast-path (reuse memoized completion) from a real change.
        let incr =
            ClaudeCodeSessionProvider::read_snapshot_incremental(session_ref, prev_offset, prev_mtime);
        let has_new_content = !incr.unchanged;
        let last_activity = incr.last_activity;

        let (completion, new_turns) = match &incr.snapshot {
            Some(snap) => (snap.completion, status_query::snapshot_turns(snap)),
            // Unchanged since last analysis: reuse the memoized completion so we
            // don't re-parse an idle log every tick (Q2=A). Empty memo → Unknown.
            None => (
                memo.unwrap_or(SessionCompletion::Unknown),
                Vec::new(),
            ),
        };

        // Memoize a freshly parsed completion for the next warm read.
        if incr.snapshot.is_some() {
            if let Ok(mut app_state) = state.lock() {
                app_state
                    .last_completion
                    .insert(session_id.clone(), completion);
            }
        }

        let status = status_query::build_session_status(
            &session_id,
            &working_directory,
            is_running,
            completion,
            last_activity,
            now,
            cached_summary.as_ref(),
        );
        statuses.push(status);

        // Gate summarization. `analyzed_at == 0` means never summarized → eligible
        // immediately (subject to consent + new content).
        let throttle_entry = if analyzed_at == 0 {
            None
        } else {
            Some(CacheEntry {
                analyzed_at,
                ..Default::default()
            })
        };
        if status_query::should_dispatch(
            consent,
            key_present,
            throttle_entry.as_ref(),
            now,
            has_new_content,
            in_flight_already,
        ) {
            // Reserve the in-flight slot under the lock so a concurrent poll
            // can't double-spawn.
            if let Ok(mut app_state) = state.lock() {
                app_state.in_flight.insert(session_id.clone());
            }
            dispatches.push(Dispatch {
                session_id,
                context_ref: context_ref.clone(),
                request: SummarizationRequest {
                    prior_summary: cached_summary,
                    new_turns,
                },
                new_offset: incr.new_offset,
                new_mtime: incr.new_mtime,
            });
        }
    }

    // Phase 4: fire-and-forget the summarizations (design decision Q1=A). Each
    // task calls the LLM WITHOUT holding the lock, then re-locks briefly to
    // write the result back — the next poll picks it up. `key_opt` is `Some`
    // whenever a dispatch exists (the gate requires `key_present`).
    if let Some(token) = key_opt {
        for dispatch in dispatches {
            let app_handle = app.clone();
            let token = token.clone();
            let region = region.clone();
            let model = model.clone();
            tauri::async_runtime::spawn(async move {
                let summarizer = status_query::CloudWorkSummarizer {
                    token,
                    region,
                    model,
                };
                let outcome = summarizer.summarize(dispatch.request).await;

                let shared = app_handle.state::<SharedState>();
                // Bind the guard to a named local (not an `if let` temporary) so its
                // borrow of `shared` ends before `shared` is dropped at task end.
                let mut app_state = match shared.lock() {
                    Ok(guard) => guard,
                    Err(_) => return,
                };
                if let SummarizationOutcome::Summarized(summary) = outcome {
                    app_state.analysis_cache.upsert(CacheEntry {
                        session_id: dispatch.session_id.clone(),
                        context_ref: dispatch.context_ref.clone(),
                        last_analyzed_offset: dispatch.new_offset,
                        last_analyzed_mtime: dispatch.new_mtime,
                        analyzed_at: unix_now(),
                        cached_summary: Some(summary),
                    });
                    app_state.analysis_cache_dirty = true;
                }
                // Always release the in-flight slot (success, insufficient, or
                // failure) so the next poll can retry naturally (NFR-2).
                app_state.in_flight.remove(&dispatch.session_id);
                app_state.flush_analysis_cache();
            });
        }
    }

    // Phase 5: aggregate and return immediately.
    Ok(aggregate_context_status(context_ref, resolution, statuses))
}

/// Report whether the user has opted in to session summarization (§12 / NFR-4).
#[tauri::command]
fn get_summarization_consent(
    state: State<SharedState>,
) -> std::result::Result<bool, CommandError> {
    let app = state.lock().map_err(|_| CommandError {
        message: "state lock poisoned".into(),
    })?;
    Ok(app.summarization_consent())
}

/// Set (and persist) the session-summarization consent flag. Returns the new
/// value so the UI can confirm the stored state.
#[tauri::command]
fn set_summarization_consent(
    state: State<SharedState>,
    enabled: bool,
) -> std::result::Result<bool, CommandError> {
    let mut app = state.lock().map_err(|_| CommandError {
        message: "state lock poisoned".into(),
    })?;
    app.set_summarization_consent(enabled)?;
    Ok(app.summarization_consent())
}

/// Build the current run status of a single target session (U3 busy guard input).
/// Reuses U2's probe cache + incremental read + pure `build_session_status`, so a
/// status query immediately followed by a delivery does not re-probe. Returns the
/// `(session_id, run_state, current_work)`. Never holds the lock across the probe.
fn target_session_status(
    state: &State<SharedState>,
    session_ref: &str,
    now: u64,
) -> (String, SessionRunState, CurrentWork) {
    let (working_directory, session_id) = ClaudeCodeSessionProvider::resume_info(session_ref)
        .unwrap_or_else(|| (String::new(), session_ref.to_string()));

    let is_running = resolve_liveness(state, now);

    // Read this session's cache/memo under a short lock.
    let (prev_offset, prev_mtime, cached_summary, memo) = {
        match state.lock() {
            Ok(app) => {
                let (o, m, cs) = match app.analysis_cache.get(&session_id) {
                    Some(e) => (e.last_analyzed_offset, e.last_analyzed_mtime, e.cached_summary.clone()),
                    None => (0, 0, None),
                };
                (o, m, cs, app.last_completion.get(&session_id).copied())
            }
            Err(_) => (0, 0, None, None),
        }
    };

    let incr =
        ClaudeCodeSessionProvider::read_snapshot_incremental(session_ref, prev_offset, prev_mtime);
    let completion = match &incr.snapshot {
        Some(snap) => snap.completion,
        None => memo.unwrap_or(SessionCompletion::Unknown),
    };
    // Memoize a freshly parsed completion for later warm reads (parity with U2).
    if incr.snapshot.is_some() {
        if let Ok(mut app) = state.lock() {
            app.last_completion.insert(session_id.clone(), completion);
        }
    }

    let status = status_query::build_session_status(
        &session_id,
        &working_directory,
        is_running,
        completion,
        incr.last_activity,
        now,
        cached_summary.as_ref(),
    );
    (session_id, status.run_state, status.current_work)
}

/// Deliver a command to a Context's Claude Code session (FR-4).
///
/// Safety-first (business-rules D1-D5): refuses empty commands (`Failed`), never
/// picks among multiple/ambiguous sessions (`Ambiguous`, FR-6/AC-14), protects a
/// `Working` session (`Busy`, AC-16), and delivers to any other state via a
/// resume one-shot that never interrupts a running session (NFR-3.2). Total: all
/// failures are reported through `DeliveryStatus`/`error`, so this returns `Ok`
/// except for a poisoned lock. The Bedrock token is never involved.
#[tauri::command]
async fn send_command_to_context_claude(
    state: State<'_, SharedState>,
    context_ref: String,
    command: String,
) -> std::result::Result<CommandDelivery, CommandError> {
    let now = unix_now();

    // Resolve the bundle under a short lock.
    let bundle = {
        let app = state.lock().map_err(|_| CommandError {
            message: "state lock poisoned".into(),
        })?;
        app.get_bundles()
            .iter()
            .find(|b| b.id.to_string() == context_ref)
            .cloned()
    };

    // Helper to build a rejection result with no target.
    let reject = |status: DeliveryStatus, error: Option<String>| CommandDelivery {
        context_ref: context_ref.clone(),
        target_session: None,
        delivery_status: status,
        current_status: SessionRunState::Unknown,
        current_work: CurrentWork::Unknown,
        error,
    };

    let Some(bundle) = bundle else {
        return Ok(reject(
            DeliveryStatus::NoTarget,
            Some("context not found".into()),
        ));
    };

    // D1: refuse an empty/whitespace-only command outright.
    if command.trim().is_empty() {
        return Ok(reject(DeliveryStatus::Failed, Some("empty command".into())));
    }

    // D2: choose a single unambiguous target (never guess among many).
    let resolution = resolve_sessions(&bundle);
    let session_ref = match select_delivery_target(&resolution) {
        DeliveryTarget::Session(session_ref) => session_ref,
        DeliveryTarget::Reject(status) => {
            let error = match status {
                DeliveryStatus::Ambiguous => Some("multiple or ambiguous sessions".into()),
                DeliveryStatus::NoTarget => Some("no coding session in this context".into()),
                _ => None,
            };
            return Ok(reject(status, error));
        }
    };

    // Compute the target's current run state (busy-guard input).
    let (target_id, run_state, current_work) = target_session_status(&state, &session_ref, now);

    // D3: block a Working session (AC-16). Command is non-blank here.
    if let Some(status) = delivery_block_reason(run_state, false) {
        return Ok(CommandDelivery {
            context_ref,
            target_session: Some(target_id),
            delivery_status: status,
            current_status: run_state,
            current_work,
            error: None,
        });
    }

    // D4/D5: deliver via a HEADLESS resume (no terminal window). Runs
    // `claude --resume <id> -p "<command>"` as a detached background process in
    // the session's cwd; it appends the new turn to the same session log, so the
    // app's 1s status poll reflects Working→WaitingForUser and the summarized
    // recent-work updates in place — the user drives Claude Code entirely from
    // the app, never a popped-up terminal. Side effect performed outside any lock.
    let delivery = run_resume_headless(&session_ref, &command);
    let status = delivery_status_after_resume(delivery.is_ok());
    let error = delivery.err();

    Ok(CommandDelivery {
        context_ref,
        target_session: Some(target_id),
        delivery_status: status,
        current_status: run_state,
        current_work,
        error,
    })
}

/// Deliver a command to ONE specific Claude Code session by its ref, bypassing
/// context resolution. This is what lets a group hold MULTIPLE sessions and
/// still Send to each one individually (FR-4, per-session variant): the caller
/// names the exact session, so there is no "ambiguous among many" rejection.
/// Same safety rules as the context variant — an empty command is `Failed`, a
/// `Working` session is protected (`Busy`, AC-16/NFR-3.2), otherwise a headless
/// resume (`Resumed`/`Failed`) that never interrupts a running session. Returns
/// `Ok` except on a poisoned lock; all outcomes ride `DeliveryStatus`/`error`.
/// The Bedrock token is never involved (`claude` uses its own local auth).
#[tauri::command]
async fn send_command_to_session(
    state: State<'_, SharedState>,
    session_ref: String,
    command: String,
) -> std::result::Result<CommandDelivery, CommandError> {
    let now = unix_now();

    // D1: refuse an empty/whitespace-only command outright.
    if command.trim().is_empty() {
        return Ok(CommandDelivery {
            context_ref: String::new(),
            target_session: None,
            delivery_status: DeliveryStatus::Failed,
            current_status: SessionRunState::Unknown,
            current_work: CurrentWork::Unknown,
            error: Some("empty command".into()),
        });
    }

    // Current run state of this exact session (busy-guard input).
    let (target_id, run_state, current_work) = target_session_status(&state, &session_ref, now);

    // D3: never inject into a Working session (AC-16).
    if let Some(status) = delivery_block_reason(run_state, false) {
        return Ok(CommandDelivery {
            context_ref: String::new(),
            target_session: Some(target_id),
            delivery_status: status,
            current_status: run_state,
            current_work,
            error: None,
        });
    }

    // D4/D5: deliver via a HEADLESS resume (no terminal), same as the context
    // path — appends the turn to this session's log so the 1s poll reflects it.
    let delivery = run_resume_headless(&session_ref, &command);
    let status = delivery_status_after_resume(delivery.is_ok());
    let error = delivery.err();

    Ok(CommandDelivery {
        context_ref: String::new(),
        target_session: Some(target_id),
        delivery_status: status,
        current_status: run_state,
        current_work,
        error,
    })
}

/// Create a brand-new Claude Code session in `cwd` seeded with `prompt`, so the
/// user can start a fresh conversation entirely from the app (the "+세션 → 새
/// 세션" flow). Runs `claude --session-id <id> -p "<prompt>"` HEADLESSLY (no
/// terminal window) and WAITS for it to finish, so the new session `.jsonl`
/// exists — with its first turn recorded — by the time we return; the caller
/// then attaches it to a group. `session_id` is a UUID minted by the caller and
/// becomes the session's stable descriptor (resolve_path accepts a bare id).
///
/// Synchronous command: Tauri runs it on a worker thread, so the blocking wait
/// never freezes the UI. Args are distinct argv entries (no shell/injection).
/// No Bedrock token is involved — `claude` uses its own local auth.
#[tauri::command]
fn start_new_coding_session(
    cwd: String,
    prompt: String,
    session_id: String,
) -> std::result::Result<(), CommandError> {
    let dir = std::path::Path::new(&cwd);
    if !dir.is_dir() {
        return Err(CommandError {
            message: format!("작업 폴더를 찾을 수 없습니다: {cwd}"),
        });
    }
    // A blank prompt still needs *something* for `claude -p` to run; seed a
    // benign opener so the session is created and left awaiting the user.
    let prompt = if prompt.trim().is_empty() {
        "새 세션을 시작합니다. 준비되면 다음 지시를 기다려 주세요.".to_string()
    } else {
        prompt
    };

    let mut cmd = std::process::Command::new("claude");
    cmd.current_dir(dir)
        .arg("--session-id")
        .arg(&session_id)
        .arg("-p")
        .arg(&prompt)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let status = cmd.status().map_err(|e| CommandError {
        message: format!("claude 실행 실패: {e}"),
    })?;
    if !status.success() {
        return Err(CommandError {
            message: format!("claude가 비정상 종료했습니다 (코드 {:?})", status.code()),
        });
    }
    Ok(())
}

/// Permanently delete a Claude Code session's conversation log from disk (the
/// "+세션 → 대화 제거" flow). Destructive and irreversible, so the UI confirms
/// first. The backend still guards the path — `delete_session` only removes a
/// file that resolves inside `~/.claude/projects`. Local-only; no external send.
#[tauri::command]
fn delete_coding_session(session_ref: String) -> std::result::Result<(), CommandError> {
    ClaudeCodeSessionProvider::delete_session(&session_ref).map_err(|e| CommandError {
        message: format!("세션 삭제 실패: {e}"),
    })
}

// ── Session-coupled external terminals (session == terminal) ─────────────
//
// Each session is bound 1:1 to a visible PowerShell window running interactive
// `claude`. The user types in that window; the app also injects input into the
// SAME window (open/send below) and reflects results by reading the session
// `.jsonl`. Lifecycle is coupled via the window PID (see the `term` module):
// removing the session closes the window; closing the window drops the session.

/// PowerShell one-liner that cd's into `cwd` and resumes `id` interactively (the
/// window the user drives). The cwd is single-quoted; the id is a UUID.
fn resume_terminal_command(cwd: &str, id: &str) -> String {
    format!(
        "Set-Location -LiteralPath {}; claude --resume {}",
        ps_single_quote(cwd),
        id
    )
}

/// Open (or reuse + focus) the terminal that resumes an EXISTING session.
#[tauri::command]
fn open_session_terminal(session_ref: String) -> std::result::Result<(), CommandError> {
    let (cwd, id) =
        ClaudeCodeSessionProvider::resume_info(&session_ref).ok_or_else(|| CommandError {
            message: "세션 경로/ID를 확인할 수 없습니다".to_string(),
        })?;
    let command = resume_terminal_command(&cwd, &id);
    term::open(&session_ref, &command)
        .map(|_pid| ())
        .map_err(|message| CommandError { message })
}

/// Open a BRAND-NEW session in a visible terminal (`claude --session-id <id>`),
/// optionally seeding an initial prompt as claude's first positional argument.
/// `session_id` (a UUID) becomes the resource descriptor; claude writes the
/// `.jsonl` on first turn.
#[tauri::command]
fn open_new_session_terminal(
    cwd: String,
    session_id: String,
    initial_prompt: Option<String>,
) -> std::result::Result<(), CommandError> {
    let dir = std::path::Path::new(&cwd);
    if !dir.is_dir() {
        return Err(CommandError {
            message: format!("작업 폴더를 찾을 수 없습니다: {cwd}"),
        });
    }
    let mut command = format!(
        "Set-Location -LiteralPath {}; claude --session-id {}",
        ps_single_quote(&cwd),
        session_id
    );
    if let Some(p) = initial_prompt.as_deref() {
        let p = p.trim();
        if !p.is_empty() {
            command.push(' ');
            command.push_str(&ps_single_quote(p));
        }
    }
    term::open(&session_id, &command)
        .map(|_pid| ())
        .map_err(|message| CommandError { message })
}

/// Send a line to the session's terminal (app → the SAME interactive `claude`).
#[tauri::command]
fn send_to_session_terminal(
    session_ref: String,
    text: String,
) -> std::result::Result<(), CommandError> {
    term::send_line(&session_ref, &text).map_err(|message| CommandError { message })
}

/// Close (kill) the session's terminal window — called when removing the session.
#[tauri::command]
fn close_session_terminal(session_ref: String) -> std::result::Result<(), CommandError> {
    term::close(&session_ref);
    Ok(())
}

/// Whether a session's terminal is currently open.
#[tauri::command]
fn session_terminal_open(session_ref: String) -> bool {
    term::is_open(&session_ref)
}

/// The session refs whose terminals are still open (dead ones are pruned). The
/// frontend diffs this against its list to drop sessions whose window the user
/// closed.
#[tauri::command]
fn list_open_session_terminals() -> Vec<String> {
    term::open_refs()
}

// ── Interactive Claude-CLI selection ("A" / 스마트 프롬프트 감지) ─────────────
//
// Run `claude` in a PTY so its interactive selection/permission prompts render,
// detect them, and surface them as native app buttons. All local; no Bedrock
// token, no external send (§12). See `pty` module for the mechanism.

/// Start (or reuse) an interactive PTY-backed `claude --resume` for a session,
/// optionally seeding it with an initial prompt. Returns the key to poll/drive.
#[tauri::command]
fn start_interactive_session(
    session_ref: String,
    initial_prompt: Option<String>,
) -> std::result::Result<String, CommandError> {
    pty::start(&session_ref, initial_prompt.as_deref())
        .map_err(|message| CommandError { message })
}

/// Open a BRAND-NEW interactive PTY-backed `claude --session-id <id>` session in
/// `cwd` (the "새 세션" flow — a real terminal the user works in, not a headless
/// file). `session_id` is a UUID minted by the caller and becomes the resource
/// descriptor; `claude` creates the `.jsonl` on first turn. Returns the session
/// ref (== `session_id`) the frontend polls/drives.
#[tauri::command]
fn start_new_interactive(
    cwd: String,
    session_id: String,
    initial_prompt: Option<String>,
) -> std::result::Result<String, CommandError> {
    let dir = std::path::Path::new(&cwd);
    if !dir.is_dir() {
        return Err(CommandError {
            message: format!("작업 폴더를 찾을 수 없습니다: {cwd}"),
        });
    }
    pty::start_new(&session_id, &cwd, &session_id, initial_prompt.as_deref())
        .map_err(|message| CommandError { message })
}

/// Type a full line into the interactive session and submit it (text + Enter) in
/// one call, so the app's send box reliably lands a turn.
#[tauri::command]
fn submit_interactive_line(
    session_ref: String,
    text: String,
) -> std::result::Result<(), CommandError> {
    pty::submit_line(&session_ref, &text).map_err(|message| CommandError { message })
}

/// Poll the interactive session's rendered screen + any detected selection prompt.
#[tauri::command]
fn interactive_screen(
    session_ref: String,
) -> std::result::Result<pty::InteractiveScreen, CommandError> {
    pty::screen(&session_ref).map_err(|message| CommandError { message })
}

/// Send a named key (up/down/enter/esc/digit/…) to the interactive session.
#[tauri::command]
fn send_interactive_key(
    session_ref: String,
    key: String,
) -> std::result::Result<(), CommandError> {
    pty::send_key(&session_ref, &key).map_err(|message| CommandError { message })
}

/// Type raw text into the interactive session (no implicit Enter).
#[tauri::command]
fn send_interactive_text(
    session_ref: String,
    text: String,
) -> std::result::Result<(), CommandError> {
    pty::write_input(&session_ref, text.as_bytes()).map_err(|message| CommandError { message })
}

/// Resize the interactive PTY to match the frontend terminal (rows x cols) so
/// `claude`'s TUI repaints to fit — keeps boxes/wrapping aligned with the visible
/// width. Called by the frontend when its xterm.js instance is fitted/resized.
#[tauri::command]
fn resize_interactive(
    session_ref: String,
    rows: u16,
    cols: u16,
) -> std::result::Result<(), CommandError> {
    pty::resize(&session_ref, rows, cols).map_err(|message| CommandError { message })
}

/// Kill the interactive `claude` process and drop it from the registry.
#[tauri::command]
fn stop_interactive_session(session_ref: String) -> std::result::Result<(), CommandError> {
    pty::stop(&session_ref).map_err(|message| CommandError { message })
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
        // A saved individual window: re-focus that exact window in its running
        // app from the stored HWND (verified by title). Never re-launch/re-open
        // a closed window — auto-reopen is deliberately out of scope; a gone
        // window surfaces as an error instead.
        ResourceKind::WindowRef => activate_live_window(
            resource.identity.hint.as_deref().unwrap_or(""),
            &resource.identity.descriptor,
            resource.identity.reopen_info.as_deref().unwrap_or(""),
        ),
        // A folder's descriptor is a POSIX path, or a `Finder\u{1f}name` handle for
        // a path-less window (Recents / saved search) → raise that window by name.
        ResourceKind::Folder => match resource.identity.descriptor.split_once('\u{1f}') {
            Some((_, win_name)) => activate_finder_window(win_name),
            None => open_path(&resource.identity.descriptor),
        },
        // A captured tab's reopen hint is `app\u{1f}url` → select that exact tab in
        // that browser (not a fresh default-browser tab). Legacy resources stored
        // only the url (no separator) → best-effort open it.
        ResourceKind::BrowserTab => match target.split_once('\u{1f}') {
            Some((app_name, url)) => activate_browser_tab(app_name, url),
            None => open_url(target),
        },
        ResourceKind::Url => open_url(target),
        // A live tab has no URL (FR-9.7): re-focus the exact tab in its running
        // browser from the saved handle, never open a URL. The title is the
        // fallback match key if the stored (hwnd,idx) token went stale.
        ResourceKind::BrowserTabLive => activate_live_tab(
            resource.identity.hint.as_deref().unwrap_or(""),
            &resource.identity.descriptor,
        ),
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

/// A browser's live tabs as `(handle_token, title, is_active)`, on demand. Only
/// Windows currently has a live tab reader (UI Automation over Chromium's tab
/// strip); macOS reads Safari/Chrome tabs for CAPTURE (title+url) but not as
/// activatable live-panel sessions, so it returns empty here (the UI falls back
/// to the app's OS windows). Never errors — an empty list is the graceful
/// degradation the caller expects.
fn enumerate_browser_tab_sessions(name: &str, bring_to_front: bool) -> Vec<(String, String, bool)> {
    #[cfg(target_os = "windows")]
    {
        vc_os_windows::WinBrowserTabReader::list_tabs(name, bring_to_front).unwrap_or_default()
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (name, bring_to_front);
        Vec::new()
    }
}

/// Focus a specific browser tab by its opaque per-platform handle, the per-tab
/// counterpart of `focus_window`. Only Windows supports it today; elsewhere the
/// UI never surfaces tab handles, so this is an explicit unsupported error.
fn focus_tab(handle: &str) -> std::result::Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        vc_os_windows::WinBrowserTabReader::activate_tab(handle).map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = handle;
        Err("browser-tab activation not supported on this platform".into())
    }
}

/// Activate a stored live browser tab from its `<browser>\u{1f}<hwnd>\u{1f}<idx>`
/// hint. Tries the stored (hwnd, index) token first; if that's stale — the
/// browser was restarted, or tabs were reordered/closed so the index moved —
/// re-reads the browser's live tabs and re-matches by the saved title (FR-9.7:
/// title only, we never fabricate a URL). Errors if the tab can no longer be
/// found so the UI surfaces it rather than silently focusing the wrong thing
/// (FR-4.2 — activating the browser without switching the tab is NOT success).
/// Split a saved live-tab hint `<browser>\u{1f}<hwnd>\u{1f}<idx>` back into its
/// `(browser, "<hwnd>\u{1f}<idx>")` parts — the inverse of the `add_tab_resource`
/// compose. The second element is exactly the token the tab reader consumes.
/// `None` when either part is missing (malformed / legacy hint).
fn split_tab_hint(hint: &str) -> Option<(&str, &str)> {
    let mut parts = hint.splitn(2, TAB_SEP);
    let browser = parts.next().unwrap_or("");
    let handle = parts.next().unwrap_or("");
    if browser.is_empty() || handle.is_empty() {
        return None;
    }
    Some((browser, handle))
}

fn activate_live_tab(hint: &str, title: &str) -> std::result::Result<(), String> {
    let (browser, handle) =
        split_tab_hint(hint).ok_or_else(|| "invalid saved tab handle".to_string())?;
    // Enumerate live tabs first so the stored position can be verified before
    // committing. Calling focus_tab blindly on a stale index silently activates
    // the wrong tab (the script returns OK as long as the index is in range,
    // regardless of whether the title matches — FR-4.2). Activating a saved tab
    // is an explicit user action, so foreground the window while re-reading:
    // that forces Chromium to build a background window's tree, so the title
    // re-match below works even when the browser has been backgrounded since.
    let live_tabs = enumerate_browser_tab_sessions(browser, true);
    // 1) Fast path: stored position still points to the expected tab.
    if live_tabs.iter().any(|(h, t, _)| h == handle && t == title) {
        return focus_tab(handle);
    }
    // 2) Position is stale — find the tab by title (handles reorder/close).
    for (h, t, _active) in &live_tabs {
        if t == title {
            return focus_tab(h);
        }
    }
    Err(format!("tab no longer open: {title}"))
}

/// Activate a stored individual window from its saved HWND `hint`, scoped to the
/// owning `app`. The OS recycles window handles, so — exactly like
/// `activate_live_tab` — we re-enumerate the app's live windows and confirm the
/// stored handle still names the SAME window before focusing it; if it went
/// stale (app restarted, window recreated) we re-match by the saved `title`
/// within that app. Errors when the window can no longer be found so the UI
/// surfaces it rather than focusing the wrong window (FR-4.2 — focusing the app
/// or a different window is NOT success). A closed window is intentionally NOT
/// reopened (out of scope): it simply reports "no longer open".
fn activate_live_window(handle: &str, title: &str, app: &str) -> std::result::Result<(), String> {
    if handle.is_empty() {
        return Err("invalid saved window handle".to_string());
    }
    let apps = enumerate_running_windows();
    if let Some((_, _, windows)) = apps.iter().find(|(name, _, _)| name == app) {
        // 1) Fast path: stored handle still names the same window.
        if windows.iter().any(|(h, t, _)| h == handle && t == title) {
            return focus_window(handle);
        }
        // 2) Handle went stale — re-match by title within the same app.
        for (h, t, _focused) in windows {
            if t == title {
                return focus_window(h);
            }
        }
    }
    Err(format!("window no longer open: {title}"))
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
        vc_os_windows::WinLauncher::run_in_terminal(&command)
            .map(|_pid| ())
            .map_err(|e| e.to_string())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = session_ref;
        Err("session resume not supported on this platform".into())
    }
}

/// Deliver a command to a coding session via a **resume one-shot** (U3 / D4):
/// open a fresh terminal that runs `claude --resume <id> "<command>"` in the
/// session's working directory. This never injects into or interrupts a running
/// session (NFR-3.2) — it starts a new resumed process, exactly like
/// [`resume_session`] but with the user's command as the initial prompt.
///
/// The cwd, session id, and command are all passed through the platform's safe
/// quoting (`shell_quote` / `ps_single_quote`), never string-concatenated, so a
/// command containing quotes/spaces/`;`/`&&` cannot break out (NFR-3 / D5). The
/// Bedrock token is not involved — this is a purely local CLI resume.
///
/// Retained as the "open in a visible terminal" alternative; the default Send
/// path is now [`run_resume_headless`] (in-app, no terminal window).
#[allow(dead_code)]
fn run_resume_with_command(session_ref: &str, command: &str) -> std::result::Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let (cwd, id) = ClaudeCodeSessionProvider::resume_info(session_ref)
            .ok_or_else(|| "cannot resolve session cwd/id".to_string())?;
        // `claude --resume <id> <prompt>` sends the (quoted) command as the
        // resumed session's initial prompt.
        let terminal_command = format!(
            "cd {} && claude --resume {} {}",
            shell_quote(&cwd),
            id,
            shell_quote(command)
        );
        vc_os_macos::MacLauncher::run_in_terminal(&terminal_command).map_err(|e| e.to_string())
    }
    #[cfg(target_os = "windows")]
    {
        let (cwd, id) = ClaudeCodeSessionProvider::resume_info(session_ref)
            .ok_or_else(|| "cannot resolve session cwd/id".to_string())?;
        let terminal_command = format!(
            "Set-Location -LiteralPath {}; claude --resume {} {}",
            ps_single_quote(&cwd),
            id,
            ps_single_quote(command)
        );
        vc_os_windows::WinLauncher::run_in_terminal(&terminal_command)
            .map(|_pid| ())
            .map_err(|e| e.to_string())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (session_ref, command);
        Err("command delivery not supported on this platform".into())
    }
}

/// Deliver a command to a coding session HEADLESSLY (no terminal window): spawn
/// `claude --resume <id> -p "<command>"` as a detached background child in the
/// session's working directory. This is the in-app control path — unlike
/// [`run_resume_with_command`] it opens no visible terminal; `claude`'s
/// non-interactive print mode executes the prompt immediately and appends the
/// new turn to the same session `.jsonl`, so the app's status poll updates in
/// place (Working while it runs, then WaitingForUser with the reply).
///
/// Fire-and-forget: we don't await the child (a coding task can take minutes),
/// so `send` returns promptly and the UI reflects progress via polling. The cwd,
/// id, and command are passed as distinct argv entries (never shell-concatenated),
/// so no quoting/injection concern (NFR-3 / D5). No Bedrock token is involved —
/// `claude` uses its own local auth.
fn run_resume_headless(session_ref: &str, command: &str) -> std::result::Result<(), String> {
    let (cwd, id) = ClaudeCodeSessionProvider::resume_info(session_ref)
        .ok_or_else(|| "cannot resolve session cwd/id".to_string())?;

    let mut cmd = std::process::Command::new("claude");
    cmd.current_dir(&cwd)
        .arg("--resume")
        .arg(&id)
        .arg("-p")
        .arg(command)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    // Windows: run with no console window so nothing pops up (CREATE_NO_WINDOW).
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    // Spawn and detach: dropping the Child neither waits nor kills it, so the
    // headless run continues to completion on its own.
    cmd.spawn()
        .map(|_child| ())
        .map_err(|e| format!("failed to launch claude headlessly: {e}"))
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
/// single quote is escaped by doubling it). Available on all platforms: the
/// session-terminal commands that call it (`open_session_terminal`,
/// `open_new_session_terminal`) are registered cross-platform, so this must
/// compile everywhere. On non-Windows `term::open` is a stub (Terminal.app is
/// out of scope), so the PowerShell string is built but unused.
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

/// Restore the saved window position/size (E2 / FR-8.10 / AC-14) before the
/// window is shown, so it reopens where the user left it. Physical pixels are
/// stored and restored as-is (consistent round-trip on the same display).
/// Best-effort: any missing value or failed call leaves the configured default.
fn restore_window_rect(app: &tauri::AppHandle, layout: &LayoutSettings) {
    use tauri::{LogicalPosition, LogicalSize, Manager};
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    if let (Some(w), Some(h)) = (layout.window_width, layout.window_height) {
        let _ = win.set_size(LogicalSize::new(w as f64, h as f64));
    }
    if let (Some(x), Some(y)) = (layout.window_x, layout.window_y) {
        let _ = win.set_position(LogicalPosition::new(x as f64, y as f64));
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
            let layout = state.layout();
            app.manage(Mutex::new(state));
            // Hand the PTY module an app handle so its reader threads can stream
            // raw output to the frontend terminal (xterm.js) via `pty://output`.
            pty::set_app_handle(app.handle().clone());
            restore_window_rect(app.handle(), &layout);
            // Surface the Accessibility prompt at launch so per-instance window
            // listing works without the user having to hunt through Settings.
            // Unscriptable to grant; only prompts when not already trusted.
            let _ = vc_os_macos::accessibility_trusted(true);
            Ok(())
        })
        .on_window_event(|window, event| {
            use tauri::{Manager, WindowEvent};
            // Track the window rect in memory as it moves/resizes (cheap, no
            // disk I/O — these fire many times per drag), then flush once when
            // the window is closing so the next launch reopens in place.
            let stash = |window: &tauri::Window| {
                if let (Ok(pos), Ok(size)) = (window.outer_position(), window.inner_size()) {
                    let scale = window.scale_factor().unwrap_or(1.0);
                    let logical_pos = pos.to_logical::<f64>(scale);
                    let logical_size = size.to_logical::<f64>(scale);
                    if let Some(state) = window.try_state::<SharedState>() {
                        if let Ok(mut app) = state.lock() {
                            app.stash_window_rect(
                                logical_pos.x as i32,
                                logical_pos.y as i32,
                                logical_size.width as u32,
                                logical_size.height as u32,
                            );
                        }
                    }
                }
            };
            match event {
                WindowEvent::Moved(_) | WindowEvent::Resized(_) => stash(window),
                WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed => {
                    stash(window);
                    if let Some(state) = window.try_state::<SharedState>() {
                        if let Ok(app) = state.lock() {
                            let _ = app.persist_settings();
                        }
                    }
                }
                _ => {}
            }
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
            list_browser_tabs,
            activate_tab,
            activate_tab_resource,
            activate_window_resource,
            activate_child,
            get_app_icon,
            create_bundle,
            delete_bundle,
            remove_resource,
            add_app_resource,
            add_tab_resource,
            add_window_resource,
            add_child_resource,
            get_layout,
            save_panel_layout,
            claude_status,
            claude_usage,
            set_claude_api_key,
            set_claude_model,
            send_claude_message,
            get_context_claude_status,
            get_summarization_consent,
            set_summarization_consent,
            send_command_to_context_claude,
            send_command_to_session,
            start_new_coding_session,
            delete_coding_session,
            start_interactive_session,
            start_new_interactive,
            submit_interactive_line,
            interactive_screen,
            send_interactive_key,
            send_interactive_text,
            resize_interactive,
            stop_interactive_session,
            open_session_terminal,
            open_new_session_terminal,
            send_to_session_terminal,
            close_session_terminal,
            session_terminal_open,
            list_open_session_terminals,
            request_accessibility
        ])
        .build(tauri::generate_context!())
        .expect("error while running vibe-control")
        .run(|_app_handle, event| {
            // Terminals the user opened via Resume are theirs to close — we do
            // NOT kill them on app exit (the user drives the session lifetime).
            // Only reap any PTY-backed `claude` child we still own (normally
            // none — the PTY path is unused by the current UI).
            if let tauri::RunEvent::Exit = event {
                pty::stop_all();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The addressing scheme registration composes and activation splits must be
    /// exact inverses: `add_tab_resource` stores `<browser>\u{1f}<handle>` and
    /// `activate_live_tab` must recover `(browser, handle)` where `handle` is the
    /// verbatim `<hwnd>\u{1f}<idx>` token the tab reader consumes. Dedup depends
    /// on the same string, so a drift here would silently break both.
    #[test]
    fn tab_hint_compose_split_roundtrip() {
        let browser = "chrome";
        let handle = format!("12345{TAB_SEP}2"); // <hwnd>\u{1f}<idx>, as the reader emits
        let hint = format!("{browser}{TAB_SEP}{handle}");

        let (b, h) = split_tab_hint(&hint).expect("well-formed hint splits");
        assert_eq!(b, browser);
        assert_eq!(h, handle); // the inner \u{1f} survives (splitn(2) stops at the first)
    }

    #[test]
    fn tab_hint_rejects_malformed() {
        assert!(split_tab_hint("").is_none());
        assert!(split_tab_hint("chrome").is_none()); // no separator → no handle
        assert!(split_tab_hint(&format!("chrome{TAB_SEP}")).is_none()); // empty handle
        assert!(split_tab_hint(&format!("{TAB_SEP}12345{TAB_SEP}2")).is_none()); // empty browser
    }
}
