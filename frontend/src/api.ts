import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  WorkBundle,
  SessionSnapshot,
  RestoreReport,
  RunningApp,
  RunningWindow,
  ChatMsg,
  ClaudeStatus,
  ContextClaudeStatus,
  CommandDelivery,
  InteractiveScreen,
  ClaudeUsage,
  LocalUsage,
  LayoutSettings,
} from "./types";

const inTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export async function getBundles(): Promise<WorkBundle[]> {
  if (!inTauri()) return [];
  return await invoke<WorkBundle[]>("get_bundles");
}

export async function saveBundles(bundles: WorkBundle[]): Promise<void> {
  if (!inTauri()) return;
  await invoke("save_bundles", { bundles });
}

export async function captureCurrent(name: string): Promise<WorkBundle> {
  return await invoke<WorkBundle>("capture_current", { name });
}

/** Create a brand-new Claude Code session in `cwd`, seeded with `prompt`, using
 *  the caller-minted `sessionId` (a UUID) as its stable id. Runs `claude` headless
 *  (no terminal) and resolves once the session exists. Caller then attaches it. */
export async function startNewCodingSession(
  cwd: string,
  prompt: string,
  sessionId: string
): Promise<void> {
  await invoke("start_new_coding_session", { cwd, prompt, sessionId });
}

/** Permanently delete a Claude Code session's conversation log from disk.
 *  Destructive — the caller must confirm first. The backend only deletes files
 *  under ~/.claude/projects. Local-only; nothing is sent anywhere. */
export async function deleteCodingSession(sessionRef: string): Promise<void> {
  await invoke("delete_coding_session", { sessionRef });
}

export async function getSessionSnapshot(
  toolId: string,
  sessionRef: string
): Promise<SessionSnapshot> {
  return await invoke<SessionSnapshot>("get_session_snapshot", {
    toolId,
    sessionRef,
  });
}

export async function restoreBundle(bundle: WorkBundle): Promise<RestoreReport> {
  return await invoke<RestoreReport>("restore_bundle", { bundle });
}

export async function resumeCodingSession(sessionRef: string): Promise<void> {
  await invoke("resume_coding_session", { sessionRef });
}

/** Bring the already-open Claude Code terminal for this session to the front
 *  (does NOT spawn a new terminal — use resumeCodingSession for that). */
export async function activateCodingSession(sessionRef: string): Promise<void> {
  await invoke("activate_coding_session", { sessionRef });
}

export async function listRunningApps(): Promise<RunningApp[]> {
  if (!inTauri()) return [];
  return await invoke<RunningApp[]>("list_running_apps");
}

export async function activateApp(target: string): Promise<void> {
  await invoke("activate_app", { target });
}

/** Request macOS Accessibility permission (needed to list per-instance windows
 *  for VS Code / Terminal / other non-browser apps). Shows the one-time system
 *  prompt when not yet granted and resolves to whether it's currently held.
 *  Returns true outside Tauri (no gate). */
export async function requestAccessibility(): Promise<boolean> {
  if (!inTauri()) return true;
  return await invoke<boolean>("request_accessibility");
}

/** Lazily fetch one app's expandable children (browser tabs, Finder folders, or
 *  individual windows) when the user expands its group (FR-2.2 / FR-2.8). */
export async function listAppChildren(
  name: string,
  bundleId: string | null
): Promise<RunningWindow[]> {
  if (!inTauri()) return [];
  return await invoke<RunningWindow[]>("list_app_children", { name, bundleId });
}

/** Activate a specific child (tab/folder/window) from an expanded group. */
export async function activateChild(
  kind: string,
  target: string,
  handle: string
): Promise<void> {
  await invoke("activate_child", { kind, target, handle });
}

/** Bring a SPECIFIC window/tab to the front (FR-2.8 / AC-20). `handle` is the
 *  opaque token from a RunningWindow; it activates exactly that window, not
 *  just the app. */
export async function activateWindow(handle: string): Promise<void> {
  await invoke("activate_window", { handle });
}

/** Lazily fetch a running browser's open tabs as individual selectable
 *  sessions (FR-9.6 / FR-10.12 / AC-20). `name` is the browser app-group name
 *  (e.g. "chrome" / "msedge"). Each RunningWindow is one tab; `handle` is an
 *  opaque per-tab token for `activateTab`. Fetched ON DEMAND when a browser
 *  group is expanded — never on the poll. Empty when no window's tabs are
 *  readable (UI then shows the OS windows instead).
 *
 *  `reveal` picks the entry point: on group-expand it stays false, so a
 *  background browser window is read best-effort but is NEVER pulled to the
 *  foreground (opening a group can't steal focus). The explicit "bring forward
 *  and re-read" button passes true, which foregrounds each browser window first
 *  — the reliable way to make Chromium build a background window's tab tree. */
export async function listBrowserTabs(
  name: string,
  reveal = false
): Promise<RunningWindow[]> {
  if (!inTauri()) return [];
  return await invoke<RunningWindow[]>("list_browser_tabs", { name, reveal });
}

/** Bring a SPECIFIC browser tab to the front (FR-2.8 / FR-4.2 / AC-20).
 *  `handle` is the opaque token from a listBrowserTabs entry; it focuses
 *  exactly that tab (foregrounding its window first), not just the browser. */
export async function activateTab(handle: string): Promise<void> {
  await invoke("activate_tab", { handle });
}

/** Lazily fetch a running app's icon as a base64 PNG data URI, or null.
 *  `id` is a bundle id when known, else an app name (same value used to
 *  activate the app). Returns null outside Tauri or when no icon exists. */
export async function getAppIcon(id: string): Promise<string | null> {
  if (!inTauri()) return null;
  return await invoke<string | null>("get_app_icon", { bundleId: id });
}

export async function createBundle(name: string): Promise<WorkBundle[]> {
  return await invoke<WorkBundle[]>("create_bundle", { name });
}

export async function deleteBundle(bundleId: string): Promise<WorkBundle[]> {
  return await invoke<WorkBundle[]>("delete_bundle", { bundleId });
}

/** Remove a single resource (the × on each item) from a context group. */
export async function removeResource(
  bundleId: string,
  resourceId: string
): Promise<WorkBundle[]> {
  return await invoke<WorkBundle[]>("remove_resource", {
    bundleId,
    resourceId,
  });
}

export async function addAppResource(
  bundleId: string,
  name: string,
  target: string
): Promise<WorkBundle[]> {
  return await invoke<WorkBundle[]>("add_app_resource", {
    bundleId,
    name,
    target,
  });
}

/** Register ONE live browser tab (dragged from the left panel) into a group as
 *  its own focus-only resource (FR-9.6 / AC-20). No URL is stored (FR-9.7) — the
 *  tab is keyed by `title` + an opaque `handle` under `browser`. The same tab
 *  can't be added twice; two different tabs of one window both can. Returns the
 *  updated bundle list. */
export async function addTabResource(
  bundleId: string,
  title: string,
  browser: string,
  handle: string
): Promise<WorkBundle[]> {
  return await invoke<WorkBundle[]>("add_tab_resource", {
    bundleId,
    title,
    browser,
    handle,
  });
}

/** Register a specific app child (browser tab, Finder folder, or single window)
 *  into a context group (FR-2.2 / §13.3). `kind` selects the resource kind. */
export async function addChildResource(
  bundleId: string,
  kind: string,
  name: string,
  target: string,
  handle: string
): Promise<WorkBundle[]> {
  return await invoke<WorkBundle[]>("add_child_resource", {
    bundleId,
    kind,
    name,
    target,
    handle,
  });
}

// ── Layout persistence (E2 / FR-8.10 / AC-14) ────────────────────────
/** Read the persisted UI layout (sidebar width, saved window rect). Returns
 *  null outside Tauri so the web dev shell just uses CSS defaults. */
export async function getLayout(): Promise<LayoutSettings | null> {
  if (!inTauri()) return null;
  return await invoke<LayoutSettings>("get_layout");
}

/** Persist the sidebar/card layout the user adjusted (debounced by the caller). */
export async function savePanelLayout(
  panelWidth: number,
  cardHeight: number
): Promise<void> {
  if (!inTauri()) return;
  await invoke("save_panel_layout", { panelWidth, cardHeight });
}

/** Activate a SAVED live browser tab (FR-4.1 / FR-4.2 / AC-20). `hint` is the
 *  stored opaque token and `title` the saved tab title (fallback match key when
 *  the token has gone stale). Focuses exactly that tab, not just the browser. */
export async function activateTabResource(
  hint: string,
  title: string
): Promise<void> {
  await invoke("activate_tab_resource", { hint, title });
}

/** Register ONE individual OS window (dragged from the left panel) into a group
 *  as a focus-only WindowRef resource (FR-2.8 / FR-3.5 / AC-20) — e.g. a single
 *  KakaoTalk chat-room window, or a browser window from the OS-window fallback.
 *  `title` is the window title, `app` the owning app-group name, `handle` the
 *  opaque HWND token. The same window title can't be added twice. Returns the
 *  updated bundle list. */
export async function addWindowResource(
  bundleId: string,
  title: string,
  app: string,
  handle: string
): Promise<WorkBundle[]> {
  return await invoke<WorkBundle[]>("add_window_resource", {
    bundleId,
    title,
    app,
    handle,
  });
}

/** Activate a SAVED individual window (FR-4.1 / FR-4.2 / AC-20). `hint` is the
 *  stored HWND token, `title` the saved window title (re-match key when the
 *  handle goes stale), `app` the owning app-group name. Focuses exactly that
 *  window, not just the app. A closed window is not reopened — it errors. */
export async function activateWindowResource(
  hint: string,
  title: string,
  app: string
): Promise<void> {
  await invoke("activate_window_resource", { hint, title, app });
}

// ── Claude prompt console ─────────────────────────────────────────────
const NO_CLAUDE: ClaudeStatus = {
  configured: false,
  source: "none",
  model: "",
  region: "",
};

/** Whether Claude is connected (a key is present). Never returns the key. */
export async function claudeStatus(): Promise<ClaudeStatus> {
  if (!inTauri()) return NO_CLAUDE;
  return await invoke<ClaudeStatus>("claude_status");
}

const NO_USAGE: ClaudeUsage = {
  configured: false,
  model: "",
  input_tokens: 0,
  output_tokens: 0,
  total_tokens: 0,
  requests: 0,
  last_input: 0,
  last_output: 0,
  last_total: 0,
  context_window: 200000,
  account: false,
  region: "",
};

/** The caller's local calendar date as YYYY-MM-DD, so the backend keys "today's"
 *  usage to the user's timezone (not UTC). */
function localDate(): string {
  const d = new Date();
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

/** Epoch seconds at the caller's local midnight today — the window start for the
 *  account-wide CloudWatch read (the backend has no local-timezone info). */
function localMidnightEpoch(): number {
  const d = new Date();
  return Math.floor(new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime() / 1000);
}

/** Today's Bedrock token usage for the graphical meter. Account-wide when AWS
 *  credentials are present, else this app's tally. Never returns content. */
export async function claudeUsage(): Promise<ClaudeUsage> {
  if (!inTauri()) return NO_USAGE;
  return await invoke<ClaudeUsage>("claude_usage", {
    today: localDate(),
    since: localMidnightEpoch(),
  });
}

const NO_LOCAL_USAGE: LocalUsage = {
  configured: false,
  total_cost_usd: 0,
  input_tokens: 0,
  output_tokens: 0,
  cache_creation_tokens: 0,
  cache_read_tokens: 0,
  total_tokens: 0,
  messages: 0,
};

/** Today's LOCAL `claude` CLI usage + USD cost for the graphical meter, summed
 *  across every session under `~/.claude/projects`. Reflects the interactive
 *  group terminals (user's own Claude auth); never returns content. */
export async function claudeLocalUsage(): Promise<LocalUsage> {
  if (!inTauri()) return NO_LOCAL_USAGE;
  return await invoke<LocalUsage>("claude_local_usage", {
    since: localMidnightEpoch(),
  });
}

/** Save (or clear, when blank) the Bedrock API key locally. Write-only. */
export async function setClaudeApiKey(key: string): Promise<ClaudeStatus> {
  return await invoke<ClaudeStatus>("set_claude_api_key", { key });
}

/** Pick which Claude model the console uses. */
export async function setClaudeModel(model: string): Promise<ClaudeStatus> {
  return await invoke<ClaudeStatus>("set_claude_model", { model });
}

/** Send the console conversation to Claude; resolves to the reply text. The
 *  local date rides along so the reply's tokens land in today's usage bucket. */
export async function sendClaudeMessage(messages: ChatMsg[]): Promise<string> {
  return await invoke<string>("send_claude_message", {
    messages,
    today: localDate(),
  });
}

// ── Claude Code context control (U1–U4) ───────────────────────────────

// Safe default when running in a plain browser (no Tauri backend): an empty,
// resolution=ContextNotFound status so the panel renders nothing intrusive.
const NO_CONTEXT_STATUS: ContextClaudeStatus = {
  context_ref: "",
  claude_sessions: [],
  recent_work: [],
  current_status: "Unknown",
  current_work: "Unknown",
  waiting_for_user: false,
  last_activity: null,
  session_resolution: "ContextNotFound",
};

/** Aggregated Claude Code status for a context (work bundle). Read-only; never
 *  interrupts a running session. Returns a benign empty status outside Tauri. */
export async function getContextClaudeStatus(
  contextRef: string
): Promise<ContextClaudeStatus> {
  if (!inTauri()) return NO_CONTEXT_STATUS;
  return await invoke<ContextClaudeStatus>("get_context_claude_status", {
    contextRef,
  });
}

/** Send a command to the context's Claude Code session (resumes it in a new
 *  terminal). Never injects into a running/working session — the backend
 *  guards on run state and returns the outcome as a CommandDelivery. */
export async function sendCommandToContextClaude(
  contextRef: string,
  command: string
): Promise<CommandDelivery> {
  return await invoke<CommandDelivery>("send_command_to_context_claude", {
    contextRef,
    command,
  });
}

/** Send a command to ONE specific session by its ref (not the whole context),
 *  so a group holding multiple sessions can Send to each independently. Never
 *  injects into a Working session — the backend guards on run state. */
export async function sendCommandToSession(
  sessionRef: string,
  command: string
): Promise<CommandDelivery> {
  return await invoke<CommandDelivery>("send_command_to_session", {
    sessionRef,
    command,
  });
}

// ── Interactive Claude-CLI selection ("A" / 스마트 프롬프트 감지) ──────────

/** Start (or reuse) an interactive PTY-backed `claude --resume` for a session,
 *  optionally seeding it with an initial prompt. Local-only; no external send.
 *  Resolves to the session key used for subsequent polls/keys. */
export async function startInteractiveSession(
  sessionRef: string,
  initialPrompt?: string
): Promise<string> {
  return await invoke<string>("start_interactive_session", {
    sessionRef,
    initialPrompt: initialPrompt ?? null,
  });
}

/** Start a BRAND-NEW interactive PTY-backed `claude --session-id <id>` in `cwd`,
 *  optionally seeding an initial prompt. Unlike startInteractiveSession (which
 *  resumes an existing log), this opens a fresh session; `claude` writes the
 *  `.jsonl` on first turn. Resolves to the session key (== sessionId). */
export async function startNewInteractive(
  cwd: string,
  sessionId: string,
  initialPrompt?: string
): Promise<string> {
  return await invoke<string>("start_new_interactive", {
    cwd,
    sessionId,
    initialPrompt: initialPrompt ?? null,
  });
}

/** Type a full line into the interactive REPL and submit it (text + Enter) in a
 *  single call, so the UI's send box reliably lands a turn. */
export async function submitInteractiveLine(
  sessionRef: string,
  text: string
): Promise<void> {
  await invoke("submit_interactive_line", { sessionRef, text });
}

/** Poll the interactive session's rendered screen and any detected prompt. */
export async function interactiveScreen(
  sessionRef: string
): Promise<InteractiveScreen> {
  return await invoke<InteractiveScreen>("interactive_screen", { sessionRef });
}

/** Send a named key (up/down/left/right/enter/esc/tab/backspace/space or a
 *  literal like a digit) to the interactive session. */
export async function sendInteractiveKey(
  sessionRef: string,
  key: string
): Promise<void> {
  await invoke("send_interactive_key", { sessionRef, key });
}

/** Type raw text into the interactive session (no implicit Enter). */
export async function sendInteractiveText(
  sessionRef: string,
  text: string
): Promise<void> {
  await invoke("send_interactive_text", { sessionRef, text });
}

/** Resize the interactive PTY so `claude`'s TUI repaints to fit the visible
 *  terminal (rows x cols). Called when the frontend xterm.js instance is fitted. */
export async function resizeInteractive(
  sessionRef: string,
  rows: number,
  cols: number
): Promise<void> {
  if (!inTauri()) return;
  await invoke("resize_interactive", { sessionRef, rows, cols });
}

/** Kill the interactive `claude` process for this session. */
export async function stopInteractiveSession(sessionRef: string): Promise<void> {
  await invoke("stop_interactive_session", { sessionRef });
}

/** One raw output chunk streamed from a PTY-backed `claude` (the `pty://output`
 *  event). `bytes` crosses the bridge as a number array; feed it to xterm.js. */
export interface PtyChunk {
  id: string;
  bytes: number[];
}

/** Subscribe to raw PTY output for ALL interactive sessions; the handler filters
 *  by `chunk.id`. Returns an unlisten fn (a no-op outside Tauri). Attach BEFORE
 *  starting a session so the opening banner isn't missed. */
export async function onPtyOutput(
  handler: (chunk: PtyChunk) => void
): Promise<UnlistenFn> {
  if (!inTauri()) return () => {};
  return await listen<PtyChunk>("pty://output", (e) => handler(e.payload));
}

// ── Session-coupled external terminals (session == terminal) ───────────
// Each session is bound 1:1 to a visible PowerShell window running interactive
// `claude`. The app injects input into (and reads results from) that SAME
// window/session; the window's lifetime is coupled to the session.

/** Open (or reuse + focus) the terminal that resumes an EXISTING session. */
export async function openSessionTerminal(sessionRef: string): Promise<void> {
  await invoke("open_session_terminal", { sessionRef });
}

/** Open a BRAND-NEW session in a visible terminal (`claude --session-id`),
 *  optionally seeding a first prompt. `sessionId` becomes the resource id. */
export async function openNewSessionTerminal(
  cwd: string,
  sessionId: string,
  initialPrompt?: string
): Promise<void> {
  await invoke("open_new_session_terminal", {
    cwd,
    sessionId,
    initialPrompt: initialPrompt ?? null,
  });
}

/** Send a line to the session's terminal — types it into the SAME interactive
 *  `claude` the user drives (app → terminal input). */
export async function sendToSessionTerminal(
  sessionRef: string,
  text: string
): Promise<void> {
  await invoke("send_to_session_terminal", { sessionRef, text });
}

/** Close (kill) the session's terminal window (called when removing a session). */
export async function closeSessionTerminal(sessionRef: string): Promise<void> {
  await invoke("close_session_terminal", { sessionRef });
}

/** Session refs whose terminals are still open (dead ones pruned). Empty outside
 *  Tauri. Used to drop sessions whose window the user closed. */
export async function listOpenSessionTerminals(): Promise<string[]> {
  if (!inTauri()) return [];
  return await invoke<string[]>("list_open_session_terminals");
}

/** Whether the user has consented to summarizing session logs (§12). Off by
 *  default; returns false outside Tauri. */
export async function getSummarizationConsent(): Promise<boolean> {
  if (!inTauri()) return false;
  return await invoke<boolean>("get_summarization_consent");
}

/** Set the summarization consent flag; resolves to the new value. */
export async function setSummarizationConsent(
  enabled: boolean
): Promise<boolean> {
  if (!inTauri()) return enabled;
  return await invoke<boolean>("set_summarization_consent", { enabled });
}
