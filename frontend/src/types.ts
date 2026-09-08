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

export interface RunningApp {
  name: string;
  bundle_id: string | null;
}
