import { invoke } from "@tauri-apps/api/core";
import type {
  WorkBundle,
  SessionSnapshot,
  RestoreReport,
  RunningApp,
  RunningWindow,
  ChatMsg,
  ClaudeStatus,
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
 *  readable (UI then shows the OS windows instead). */
export async function listBrowserTabs(name: string): Promise<RunningWindow[]> {
  if (!inTauri()) return [];
  return await invoke<RunningWindow[]>("list_browser_tabs", { name });
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

/** Save (or clear, when blank) the Bedrock API key locally. Write-only. */
export async function setClaudeApiKey(key: string): Promise<ClaudeStatus> {
  return await invoke<ClaudeStatus>("set_claude_api_key", { key });
}

/** Pick which Claude model the console uses. */
export async function setClaudeModel(model: string): Promise<ClaudeStatus> {
  return await invoke<ClaudeStatus>("set_claude_model", { model });
}

/** Send the console conversation to Claude; resolves to the reply text. */
export async function sendClaudeMessage(messages: ChatMsg[]): Promise<string> {
  return await invoke<string>("send_claude_message", { messages });
}
