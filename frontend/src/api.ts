import { invoke } from "@tauri-apps/api/core";
import type {
  WorkBundle,
  SessionSnapshot,
  RestoreReport,
  RunningApp,
  RunningWindow,
  ChatMsg,
  ClaudeStatus,
  ClaudeUsage,
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
