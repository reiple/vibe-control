export type ResourceKind =
  | "WindowRef"
  | "BrowserTab"
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
