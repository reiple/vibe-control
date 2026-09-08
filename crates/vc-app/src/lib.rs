// Application services + Tauri bridge for vibe-control
// Orchestrates domain + adapters, exposes Tauri commands.

mod claude;
mod status_query;

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

/// A live running application for the left-hand panel (FR-2.1 / §13.1).
#[derive(Serialize)]
pub struct RunningApp {
    name: String,
    /// Stable launch target; the bundle id when available (§13.4).
    bundle_id: Option<String>,
}

/// List apps currently running on this machine (FR-2.1 / §13.1).
#[tauri::command]
async fn list_running_apps() -> std::result::Result<Vec<RunningApp>, CommandError> {
    Ok(enumerate_running_apps_detailed()
        .into_iter()
        .map(|(name, bundle_id)| RunningApp { name, bundle_id })
        .collect())
}

/// Bring an app to the front, launching it if closed (FR-2.6 / FR-4.x / §13.4).
/// `target` is a bundle id or an app name.
#[tauri::command]
async fn activate_app(target: String) -> std::result::Result<(), CommandError> {
    open_app(&target)?;
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

/// Extract a macOS app icon as a base64 PNG data URI; `None` elsewhere.
fn extract_app_icon(target: &str) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        vc_os_macos::MacIconReader::icon_data_uri(target)
    }
    #[cfg(not(target_os = "macos"))]
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
    let reply = claude::send_message(&key, &region, &model, &messages).await?;
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
                if let Ok(mut app_state) = shared.lock() {
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
                }
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

    // D4/D5: deliver via resume one-shot (side effect performed outside any lock).
    let delivery = run_resume_with_command(&session_ref, &command);
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

fn reopen_resource(resource: &Resource) -> std::result::Result<(), String> {
    // Prefer an explicit reopen hint; fall back to the raw descriptor.
    let target = resource
        .identity
        .reopen_info
        .as_deref()
        .unwrap_or(resource.identity.descriptor.as_str());

    match resource.kind {
        ResourceKind::AppLaunch | ResourceKind::WindowRef => open_app(target),
        ResourceKind::Folder => open_path(target),
        ResourceKind::Url | ResourceKind::BrowserTab => open_url(target),
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
        vc_os_windows::WinLauncher::run_in_terminal(&terminal_command).map_err(|e| e.to_string())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (session_ref, command);
        Err("command delivery not supported on this platform".into())
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

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let state = AppState::new().map_err(|e| e.to_string())?;
            app.manage(Mutex::new(state));
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
            activate_app,
            get_app_icon,
            create_bundle,
            delete_bundle,
            add_app_resource,
            claude_status,
            set_claude_api_key,
            set_claude_model,
            send_claude_message,
            get_context_claude_status,
            get_summarization_consent,
            set_summarization_consent,
            send_command_to_context_claude
        ])
        .run(tauri::generate_context!())
        .expect("error while running vibe-control");
}
