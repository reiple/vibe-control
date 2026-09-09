export type ResourceKind =
  | "WindowRef"
  | "BrowserTab"
  | "BrowserTabLive"
  | "Folder"
  | "AppLaunch"
  | "Url"
  | "CodingSession";

export type ResourceStatus =
  | "Active"
  | "Inactive"
  | "PermissionRequired"
  | "Unknown";

export type SessionCompletion = "Waiting" | "NotWaiting" | "Unknown";

export interface ResourceIdentity {
  kind: ResourceKind;
  descriptor: string;
  hint: string | null;
  reopen_info: string | null;
}

export interface Resource {
  id: string;
  display_name: string;
  kind: ResourceKind;
  identity: ResourceIdentity;
  order: number;
}

export interface WorkBundle {
  id: string;
  name: string;
  resources: Resource[];
}

export interface Turn {
  role: string;
  content: string;
}

export interface SessionSnapshot {
  conversation: Turn[];
  last_question: string | null;
  completion: SessionCompletion;
  available: boolean;
}

export interface RestoreReport {
  opened: string[];
  failed: string[];
  skipped: string[];
}

/** One live window/tab of a running app, shown when its group is expanded
 *  (FR-2.8 / AC-20). `handle` is an opaque per-platform activation token passed
 *  back verbatim to `activateWindow` — never parsed by the frontend. */
export interface RunningWindow {
  handle: string;
  title: string;
  /** "tab" (browser), "folder" (Finder), or "window" (everything else) — drives
   *  the child icon and which resource kind it registers as. */
  kind: string;
  /** Value registered into a group when dragged in: a URL (tab), a POSIX path
   *  (folder), or the window handle (window). */
  target: string;
  is_focused: boolean;
}

export interface RunningApp {
  name: string;
  bundle_id: string | null;
  /** This app's live windows/tabs, grouped under one icon (FR-2.2/2.8). May be
   *  empty when only app-level info is available — the app is then a single
   *  activatable entry. */
  windows: RunningWindow[];
}

/** Persisted UI layout (sidebar width + saved window rect). Mirrors the backend
 *  `LayoutSettings` — never carries Claude credentials. */
export interface LayoutSettings {
  panel_width: number;
  card_height: number;
  window_x: number | null;
  window_y: number | null;
  window_width: number | null;
  window_height: number | null;
}

/** One turn of the in-app Claude prompt console. */
export interface ChatMsg {
  role: "user" | "assistant";
  content: string;
}

/** Claude connection status — never includes the key itself. */
export interface ClaudeStatus {
  configured: boolean;
  source: "settings" | "env" | "none";
  model: string;
  /** AWS region for the Bedrock endpoint (e.g. "ap-northeast-2"). */
  region: string;
}

// ── Claude Code context control (U1–U4) ───────────────────────────────
// These mirror the backend serde shapes exactly (crate vc-core's
// claude_status.rs). Structs keep snake_case fields; bare (data-less) Rust
// enums serialize as PascalCase string unions; enums with data are externally
// tagged (`{ Variant: payload }`).

/** Five-state run status for a Claude Code session. */
export type SessionRunState =
  | "Working"
  | "WaitingForUser"
  | "Idle"
  | "Inactive"
  | "Unknown";

/** Whether a work item is directly observed or inferred by summarization. */
export type Provenance = "Fact" | "Inferred";

/** A single unit of summarized work. */
export interface WorkItem {
  title: string;
  provenance: Provenance;
  /** Optional longer detail; omitted (absent) when the backend has none. */
  detail?: string;
}

/** What the session is doing right now. Externally tagged: `{ Known: … }` or
 *  the bare string `"Unknown"`. */
export type CurrentWork = { Known: WorkItem } | "Unknown";

/** Result of resolving a Context to its coding session(s). Externally tagged
 *  for the data variants; bare string for the unit variants. */
export type SessionResolution =
  | { Resolved: string }
  | { Multiple: string[] }
  | "Ambiguous"
  | "NoSession"
  | "ContextNotFound";

/** Status of a single Claude Code session within a context. */
export interface ClaudeSessionStatus {
  session_id: string;
  working_directory: string;
  is_running: boolean | null;
  last_activity: number | null;
  run_state: SessionRunState;
  current_work: CurrentWork;
  recent_work: WorkItem[];
  latest_work?: WorkItem;
}

/** Context-level (aggregated) Claude status for a work bundle. */
export interface ContextClaudeStatus {
  context_ref: string;
  claude_sessions: ClaudeSessionStatus[];
  recent_work: WorkItem[];
  latest_work?: WorkItem;
  current_status: SessionRunState;
  current_work: CurrentWork;
  waiting_for_user: boolean;
  last_activity: number | null;
  session_resolution: SessionResolution;
}

/** Delivery outcome for a command sent to a session. */
export type DeliveryStatus =
  | "Delivered"
  | "Resumed"
  | "Busy"
  | "Ambiguous"
  | "NoTarget"
  | "Failed";

/** Result of a command-delivery attempt. */
export interface CommandDelivery {
  context_ref: string;
  target_session?: string;
  delivery_status: DeliveryStatus;
  current_status: SessionRunState;
  current_work: CurrentWork;
  error?: string;
}

/** A selection prompt detected in the interactive PTY screen (the "A" feature).
 *  `selected` is the index of the currently highlighted option. */
export interface DetectedPrompt {
  question: string;
  options: string[];
  selected: number;
}

/** A poll of an interactive Claude-CLI session: liveness, the rendered screen
 *  lines, and any detected selection prompt. */
export interface InteractiveScreen {
  alive: boolean;
  lines: string[];
  prompt: DetectedPrompt | null;
}

/** Running Bedrock token usage for the on-screen meter. All cumulative except
 *  the `last_*` fields (the most recent call). Content is never included. */
export interface ClaudeUsage {
  configured: boolean;
  model: string;
  input_tokens: number;
  output_tokens: number;
  total_tokens: number;
  requests: number;
  last_input: number;
  last_output: number;
  last_total: number;
  /** Model context window, for gauge scaling. */
  context_window: number;
  /** True when the today totals are the whole account's CloudWatch usage
   *  (all models, this region); false when only this app's local tally. */
  account: boolean;
  /** Region the account totals were read from (empty when app-local). */
  region: string;
}
