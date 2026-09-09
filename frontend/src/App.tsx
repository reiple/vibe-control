import { Fragment, useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type {
  WorkBundle,
  Resource,
  ResourceStatus,
  RestoreReport,
  RunningApp,
  RunningWindow,
  ClaudeStatus,
  LocalUsage,
  SessionSnapshot,
} from "./types";
import {
  getBundles,
  restoreBundle,
  listRunningApps,
  activateApp,
  activateWindow,
  getAppIcon,
  createBundle,
  deleteBundle,
  addAppResource,
  saveBundles,
  claudeStatus,
  claudeLocalUsage,
  setClaudeApiKey,
  setClaudeModel,
  startInteractiveSession,
  startNewInteractive,
  submitInteractiveLine,
  interactiveScreen,
  stopInteractiveSession,
  // ── Restored (#15 dropped the App.tsx wiring; backend + api.ts intact) ──
  // Left-panel per-tab/per-window enumeration, drag-registration, and exact
  // activation of saved tabs/windows. Windows uses listBrowserTabs (tabs) +
  // the polled app.windows (windows); macOS uses the generic children model.
  listBrowserTabs,
  activateTab,
  activateTabResource,
  addTabResource,
  addWindowResource,
  activateWindowResource,
  removeResource,
  listAppChildren,
  activateChild,
  addChildResource,
  requestAccessibility,
  // Group conversation Log viewer (FR-12.3 / AC-17) + persisted sidebar layout
  // (E2). Backend commands survived #15; only the App.tsx wiring was dropped.
  getSessionSnapshot,
  getLayout,
  savePanelLayout,
} from "./api";
import { GroupTerminal } from "./GroupTerminal";

// Selectable models for the console. These are AWS Bedrock inference-profile
// ids (the app talks to the Bedrock runtime, not api.anthropic.com), matching
// how Claude Code itself reaches the model here.
const CLAUDE_MODELS: { id: string; label: string }[] = [
  { id: "global.anthropic.claude-opus-4-8", label: "Opus 4.8" },
  { id: "global.anthropic.claude-haiku-4-5-20251001-v1:0", label: "Haiku 4.5" },
];

// Fallback shown when no model is saved yet — matches the backend default.
const DEFAULT_MODEL = "global.anthropic.claude-opus-4-8";

// KO-II usage meter helpers ------------------------------------------------
// Compact token formatter: 942 · 12.3k · 1.24M.
const fmtTokens = (n: number): string => {
  if (n < 1000) return String(n);
  if (n < 1_000_000) return `${(n / 1000).toFixed(n < 10_000 ? 1 : 0)}k`;
  return `${(n / 1_000_000).toFixed(2)}M`;
};
// The big cumulative counter shows full digits with thousands separators.
const fmtFull = (n: number): string => n.toLocaleString("en-US");
// Today's local CLI spend as USD. Sub-dollar keeps cents visible ($0.42);
// larger amounts round to whole dollars with separators ($1,234) so the LED
// stays legible. Cache tokens are already priced in at their reduced rate.
const fmtUSD = (n: number): string =>
  n < 100
    ? `$${n.toFixed(2)}`
    : `$${Math.round(n).toLocaleString("en-US")}`;
// Human label for the connected model id (falls back to the raw id's tail).
const modelLabel = (id: string): string =>
  CLAUDE_MODELS.find((m) => m.id === id)?.label ??
  (id ? id.split(".").pop() ?? id : "—");

// Tauri command errors arrive as a serialized object ({ message }), so String(e)
// would render "[object Object]". Pull out the human-readable message.
function errText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) {
    const m = (e as { message: unknown }).message;
    if (typeof m === "string") return m;
  }
  try {
    return JSON.stringify(e);
  } catch {
    return String(e);
  }
}

const kindLabel: Record<string, string> = {
  WindowRef: "Window",
  BrowserTab: "Tab",
  BrowserTabLive: "Tab",
  Folder: "Folder",
  AppLaunch: "App",
  Url: "URL",
  CodingSession: "Session",
};

const isActivatable = (kind: string) =>
  kind === "AppLaunch" || kind === "WindowRef" || kind === "Folder";

// A saved browser tab the user can re-focus by double-click. Covers BOTH the
// legacy `BrowserTab` (persisted with a URL descriptor + `browser␟url` reopen
// info — every tab in existing bundles is this kind) and the newer live
// `BrowserTabLive` (focus-only handle, FR-9.6 / AC-20). Excluding `BrowserTab`
// here made `canActivate` false, so double-clicking a saved Safari tab was a
// silent no-op ("사파리가 아예 안 뜬다") — the backend was never even called.
const isSavedTab = (kind: string) =>
  kind === "BrowserTabLive" || kind === "BrowserTab";

// Apps whose expanded list shows browser TABS (via UI Automation) rather than
// OS windows — matched case-insensitively against the app-group name, which on
// Windows is the browser's exe stem (FR-9.6 / FR-10.12). A browser group is
// always expandable so its tabs can be revealed even when it has one window.
// Tabs are fetched lazily on expand, never on the 1-second poll (FR-10.7).
const BROWSER_APPS = new Set(["chrome", "msedge", "brave", "whale"]);

// The left-panel expansion has two OS-specific models that must not regress each
// other: Windows uses UI-Automation tab/window enumeration (per-tab via
// listBrowserTabs, per-window from the polled app.windows), while macOS uses the
// generic children model (listAppChildren → Safari/Chrome tabs, Finder folders,
// per-instance windows). One flag switches the expand/fetch/render/drag paths.
const IS_MACOS =
  /Mac|iP(hone|ad|od)/.test(navigator.platform) ||
  /Macintosh|Mac OS X/.test(navigator.userAgent);

// Apps are keyed by (name + bundle id): two distinct apps can share a display
// name with different bundle ids, so keying on name alone would collide React
// keys and merge their expand/children state. NUL (U+0000) can't occur in a
// name or bundle id, so it's an unambiguous separator.
const APP_KEY_SEP = String.fromCharCode(0);
const appKey = (a: { name: string; bundle_id?: string | null }) =>
  `${a.name}${APP_KEY_SEP}${a.bundle_id ?? ""}`;

// A WindowRef's descriptor/reopen_info is an opaque window handle
// (`app<U+001F>title<U+001F>idx`); its app-name part (before the first U+001F
// unit separator) resolves the app icon. A no-op for handle-less targets.
const UNIT_SEP = String.fromCharCode(31);
const appNameOf = (handle: string) => handle.split(UNIT_SEP)[0];

/** A saved resource's live status indicator (FR-7.1/7.2): a green dot when the
 *  resource is currently running, a hollow grey dot when it's not, and an amber
 *  dot when the OS won't let us tell. `Unknown`/absent renders nothing so
 *  resources whose status can't meaningfully be evaluated stay unadorned. */
function StatusDot({ status }: { status?: ResourceStatus }) {
  if (!status || status === "Unknown") return null;
  const label =
    status === "Active"
      ? "실행 중"
      : status === "Inactive"
        ? "실행 중 아님"
        : "권한 필요";
  return (
    <span
      className={`res-status res-status--${status.toLowerCase()}`}
      title={label}
      aria-label={label}
    />
  );
}

/** A group's live-terminal target: the descriptor of its (first) Claude Code
 *  session, or null when the group has no session to drive. */
const groupSessionRef = (b: WorkBundle): string | null =>
  b.resources.find((r) => r.kind === "CodingSession")?.identity.descriptor ??
  null;

/** The working folder stored on a group's (first) Claude Code session (in
 *  `hint`), or null. Used only as a fallback for a session with no transcript to
 *  resume — the backend starts it fresh there so a never-run session still
 *  opens. Older sessions saved before cwd was persisted return null. */
const groupSessionCwd = (b: WorkBundle): string | null =>
  b.resources.find((r) => r.kind === "CodingSession")?.identity.hint ?? null;

/** Parse a leading `@group` target out of a prompt. Matches the longest group
 *  name that the text (after `@`) starts with — so multi-word names work — and
 *  also a space-collapsed token form (`@Group1` for "Group 1"). Only matches
 *  KNOWN group names; anything else is left verbatim as prompt text (so a stray
 *  `@someone` in a real prompt isn't hijacked). */
function parseMention(
  raw: string,
  bundles: WorkBundle[]
): { bundle?: WorkBundle; body: string } {
  if (!raw.startsWith("@")) return { body: raw };
  const rest = raw.slice(1);
  const lower = rest.toLowerCase();
  const prefixMatch = bundles
    .filter((b) => lower.startsWith(b.name.toLowerCase()))
    .sort((a, b) => b.name.length - a.name.length)[0];
  if (prefixMatch) {
    return { bundle: prefixMatch, body: rest.slice(prefixMatch.name.length).trim() };
  }
  const token = rest.split(/\s+/)[0] ?? "";
  const collapsed = bundles.find(
    (b) => b.name.replace(/\s+/g, "").toLowerCase() === token.toLowerCase()
  );
  if (collapsed) return { bundle: collapsed, body: rest.slice(token.length).trim() };
  return { body: raw };
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

// `IS_MACOS` (defined above for the left-panel OS-specific models) is reused
// here so the frameless-window custom controls (below) render on Windows ONLY —
// macOS keeps its native traffic lights (FR-8.11 / AC-22).

// Windows has no native title bar (decorations are stripped in vc-app so the
// window is frameless like macOS's Overlay — FR-8.11). macOS still draws its
// native traffic lights, so these custom controls render on Windows ONLY and
// deliberately mimic the macOS traffic lights (same top-left spot, same
// red/amber/green dots) so both platforms read as the same screen (AC-22).
// They sit outside any `data-tauri-drag-region`, so clicks activate the button
// instead of starting a window drag.
function WindowControls() {
  const win = getCurrentWindow();
  return (
    <div className="wc-bar" role="group" aria-label="Window controls">
      <button
        type="button"
        className="wc-dot wc-close"
        aria-label="Close"
        title="Close"
        onClick={() => void win.close()}
      >
        <svg className="wc-glyph" viewBox="0 0 12 12" aria-hidden="true">
          <path d="M3.5 3.5 L8.5 8.5 M8.5 3.5 L3.5 8.5" />
        </svg>
      </button>
      <button
        type="button"
        className="wc-dot wc-min"
        aria-label="Minimize"
        title="Minimize"
        onClick={() => void win.minimize()}
      >
        <svg className="wc-glyph" viewBox="0 0 12 12" aria-hidden="true">
          <path d="M3 6 L9 6" />
        </svg>
      </button>
      <button
        type="button"
        className="wc-dot wc-max"
        aria-label="Maximize"
        title="Maximize"
        onClick={() => void win.toggleMaximize()}
      >
        <svg className="wc-glyph" viewBox="0 0 12 12" aria-hidden="true">
          <rect x="3.4" y="3.4" width="5.2" height="5.2" rx="0.6" />
        </svg>
      </button>
    </div>
  );
}

/** A freshly-spawned `claude` TUI isn't ready for input the instant it starts —
 *  text lands in the box but an immediate Enter is swallowed during boot (so the
 *  first prompt sits unsent). Poll the rendered screen until the REPL shows its
 *  input hint, THEN submit the line (text + Enter). Falls back to submitting
 *  after a deadline so a missed marker never strands the prompt. */
async function submitWhenReady(ref: string, text: string): Promise<void> {
  const deadline = Date.now() + 15000;
  while (Date.now() < deadline) {
    try {
      const scr = await interactiveScreen(ref);
      if (scr.alive) {
        const joined = scr.lines.join("\n").toLowerCase();
        // The REPL footer ("? for shortcuts") only renders once it's accepting
        // input — a reliable "ready to submit" signal across claude versions.
        if (joined.includes("shortcuts")) break;
      }
    } catch {
      // Session not registered yet (spawn still in flight) — keep waiting.
    }
    await sleep(300);
  }
  await submitInteractiveLine(ref, text);
}

// Module-level cache: an app's high-res icon (§13.5) is fetched once per
// identifier for the app's lifetime. Seeding useState from this on mount keeps
// refresh/filter/remount from refetching or flashing the fallback dot.
const iconCache = new Map<string, string | null>();

function useAppIcon(target: string | null | undefined): string | null {
  const [icon, setIcon] = useState<string | null>(() =>
    target ? (iconCache.get(target) ?? null) : null
  );

  useEffect(() => {
    let cancelled = false;
    if (!target) {
      setIcon(null);
      return;
    }
    if (iconCache.has(target)) {
      setIcon(iconCache.get(target)!); // cache hit → no fetch, no flicker
      return;
    }
    getAppIcon(target)
      .then((uri) => {
        iconCache.set(target, uri);
        if (!cancelled) setIcon(uri);
      })
      .catch(() => {
        iconCache.set(target, null); // failure cached as "no icon"
        if (!cancelled) setIcon(null);
      });
    return () => {
      cancelled = true; // ignore late resolves after unmount/target change
    };
  }, [target]);

  return icon;
}

function AppIcon({
  target,
  className = "app-icon",
}: {
  target: string | null | undefined;
  className?: string;
}) {
  const icon = useAppIcon(target);
  return (
    <span className={className} aria-hidden>
      {icon ? (
        <img
          src={icon}
          alt=""
          draggable={false}
          decoding="async"
          onError={(e) => {
            (e.currentTarget as HTMLImageElement).style.display = "none";
          }}
        />
      ) : (
        <span className="app-dot" />
      )}
    </span>
  );
}

// Full-viewport boot splash / intro animation. The intro is ported verbatim
// from the `intro-animation` example (frontend/public/intro/{loader.js,
// loader.css} + the vendored GSAP/EasePack): a "fake loading" counter runs
// 000 → 100 %, morphs into "DEV", then the hero grid scrambles and decodes the
// letters into the VIBE-CONTROL wordmark — the spasoje.dev loading-animation
// language. React mounts this overlay on boot (blocking all input underneath)
// and fades it out via `hiding` once the app's first load has finished.
//
// The example ships as a self-contained IIFE that queries the loader markup by
// id/class, auto-plays on `document.fonts.ready`, and registers
// `window.spasojeLoader`. So we render its exact markup (dangerouslySetInnerHTML,
// straight from the example's demo.html body) and, on mount, inject its
// stylesheet + the vendored scripts. The stylesheet is scoped to the splash's
// lifetime — removed on unmount — so the example's global resets never leak
// into the app.
const INTRO_HTML = `<main id="hero" class="hero" hidden><div class="hero-top">NOTHANGTON</div><div class="hero-grid"></div></main><div class="loader-loader" id="loader" role="status" aria-label="Loading"><div class="loader-wrapper"><div class="loader-empty loader-empty1"><p>S</p></div><div class="loader-empty loader-empty2"><p>P</p></div><div class="loader-text"><h2>LOADING</h2></div><div class="loader-slash"><p>/</p></div><div class="loader-number loader-number1"><p>0</p></div><div class="loader-number loader-number2"><p>0</p></div><div class="loader-number loader-number3"><p>0</p></div><div class="loader-percent"><p>%</p></div><div class="loader-logo"><p>NOTHANGTON</p></div><div class="loader-arrow loader-arrow1"><p>&gt;</p></div></div></div><button class="replay" id="replay" hidden>다시 재생 ↗</button>`;

function Splash({ hiding }: { hiding: boolean }) {
  useEffect(() => {
    // Vite emits public/ assets at the app root; a relative base resolves them
    // under both the dev server and Tauri's custom protocol.
    const base = "./";

    // Inject the example's stylesheet only while the splash is mounted, so its
    // global resets (`* { margin: 0 }`, body background/font) can't leak into
    // the app once the splash unmounts.
    const link = document.createElement("link");
    link.rel = "stylesheet";
    link.dataset.introStyle = "1";
    link.href = base + "intro/loader.css";
    document.head.appendChild(link);

    let cancelled = false;
    const loadScript = (src: string) =>
      new Promise<void>((resolve, reject) => {
        // De-dupe: the vendored/loader scripts define their globals once, and
        // React StrictMode remounts this effect in development.
        const existing = document.querySelector(
          `script[data-intro-src="${src}"]`,
        );
        if (existing) {
          resolve();
          return;
        }
        const s = document.createElement("script");
        s.src = src;
        s.dataset.introSrc = src;
        s.onload = () => resolve();
        s.onerror = () => reject(new Error(`failed to load ${src}`));
        document.body.appendChild(s);
      });

    (async () => {
      try {
        await loadScript(base + "intro/vendor/gsap.min.js");
        await loadScript(base + "intro/vendor/EasePack.min.js");
        const w = window as unknown as {
          spasojeLoader?: { play: () => void; destroy: () => void };
        };
        // loader.js is an IIFE that auto-plays on load and registers
        // window.spasojeLoader. If it already ran (StrictMode remount), replay
        // it against the freshly-rendered markup instead of loading it again.
        if (w.spasojeLoader) {
          if (!cancelled) w.spasojeLoader.play();
        } else {
          await loadScript(base + "intro/loader.js");
        }
      } catch {
        // If GSAP fails to load, the splash simply holds its background and the
        // boot timer still lifts it — no hard failure, no stranded UI.
      }
    })();

    return () => {
      cancelled = true;
      const w = window as unknown as { spasojeLoader?: { destroy: () => void } };
      w.spasojeLoader?.destroy?.();
      link.remove();
    };
  }, []);

  return (
    <div
      className={`splash ${hiding ? "splash--hidden" : ""}`}
      role="progressbar"
      aria-label="Loading vibe-control"
      aria-busy="true"
      // Exact loader/hero markup from the intro-animation example (demo.html).
      dangerouslySetInnerHTML={{ __html: INTRO_HTML }}
    />
  );
}

// Read-only full-conversation viewer for one Claude Code session (FR-12.3 /
// AC-17). Opened from a group's terminal bar; `title` is the session's display
// name so, when a group has several sessions, the header says WHICH one this is.
// Independent of the live embedded terminal (#15) — this just reads a snapshot.
function ConversationModal({
  sessionRef,
  title,
  onClose,
}: {
  sessionRef: string;
  title: string;
  onClose: () => void;
}) {
  const [snap, setSnap] = useState<SessionSnapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [err, setErr] = useState<string | null>(null);
  const bottomRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    getSessionSnapshot("claude-code", sessionRef)
      .then((s) => {
        if (!cancelled) setSnap(s);
      })
      .catch((e) => {
        if (!cancelled) setErr(errText(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [sessionRef]);

  // Start pinned to the latest turn (that's the interesting end of a session).
  useEffect(() => {
    bottomRef.current?.scrollIntoView();
  }, [snap]);

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal conversation-modal"
        role="dialog"
        aria-modal="true"
        aria-label="Session conversation"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="conversation-head">
          <h3 className="modal-title" title={title}>
            {title}
          </h3>
          <button className="ghost icon" onClick={onClose} title="Close">
            ✕
          </button>
        </div>
        <div className="conversation-body">
          {loading && <div className="conversation-empty">Loading…</div>}
          {err && <div className="console-error">{err}</div>}
          {!loading &&
            !err &&
            (!snap || !snap.available || snap.conversation.length === 0) && (
              <div className="conversation-empty">
                No conversation available for this session.
              </div>
            )}
          {snap?.conversation.map((t, i) => (
            <div key={i} className={`conv-turn ${t.role}`}>
              <span className="conv-role">
                {t.role === "user" ? "You" : "Claude"}
              </span>
              <p className="conv-text">{t.content}</p>
            </div>
          ))}
          <div ref={bottomRef} />
        </div>
      </div>
    </div>
  );
}

export default function App() {
  const [bundles, setBundles] = useState<WorkBundle[]>([]);
  const [runningApps, setRunningApps] = useState<RunningApp[]>([]);
  const [filter, setFilter] = useState("");
  // App groups the user expanded to reveal their windows/tabs (FR-2.2/2.8).
  // Keyed by appKey (name+bundle id) so the expansion survives the 1s poll
  // replacing the list, and two same-named apps don't collide.
  const [expandedApps, setExpandedApps] = useState<Set<string>>(new Set());

  // ── Windows browser tabs (read lazily on expand, never on the poll, FR-10.7) ──
  const [tabsByApp, setTabsByApp] = useState<Record<string, RunningWindow[]>>({});
  const [tabsLoadingApps, setTabsLoadingApps] = useState<Set<string>>(new Set());

  // ── macOS children (tabs/folders/windows via listAppChildren), keyed by appKey ──
  const [appChildren, setAppChildren] = useState<Record<string, RunningWindow[]>>({});
  const [loadingApp, setLoadingApp] = useState<Set<string>>(new Set());
  const childFetchSeq = useRef<Record<string, number>>({});
  // macOS-only: Accessibility not yet granted → per-instance window lists are empty.
  const [accessNeeded, setAccessNeeded] = useState(false);

  // Today's LOCAL `claude` CLI usage for the KO-II meter, summed from the on-disk
  // transcripts under ~/.claude/projects across every session. This tracks what
  // the interactive group terminals actually spend (they run on the user's own
  // Claude auth) and is priced per model, so the amber readout shows real USD.
  // Polled every few seconds so the number climbs live as a terminal works.
  const [usage, setUsage] = useState<LocalUsage | null>(null);
  // Distinguish the meter's four states so an un-fetched value is never shown as
  // "0": `usageLoading` = a fetch is in flight and we have no data yet;
  // `usageErr` = the last fetch failed (holds the reason). A successful fetch
  // clears both; `usage.configured === false` is the "CLI not used yet" state.
  const [usageLoading, setUsageLoading] = useState(true);
  const [usageErr, setUsageErr] = useState<string | null>(null);

  // Persisted sidebar width (E2). Applied imperatively to the aside via a ref so
  // React never re-renders it and fights the CSS `resize` drag; a ResizeObserver
  // persists changes (debounced). cardHeight is carried through unchanged for now.
  const sidebarRef = useRef<HTMLElement | null>(null);
  const [savedPanelWidth, setSavedPanelWidth] = useState<number | null>(null);
  const cardHeightRef = useRef<number>(150);

  // Full-conversation viewer (FR-12.3 / AC-17): the session whose transcript is
  // open (ref = session descriptor, title = its display name for the header).
  const [convSession, setConvSession] = useState<{
    ref: string;
    title: string;
  } | null>(null);

  // Boot splash: `booting` keeps the overlay mounted (blocking input);
  // `splashOut` triggers its fade just before we unmount it.
  const [booting, setBooting] = useState(true);
  const [splashOut, setSplashOut] = useState(false);

  const [newGroupName, setNewGroupName] = useState("");
  const [showGroupModal, setShowGroupModal] = useState(false);
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);
  // Two-step confirm for killing+removing a group's live claude session (the
  // terminal): first click arms, second click deletes. Keyed by bundle id.
  const [confirmKillId, setConfirmKillId] = useState<string | null>(null);
  const [dragOverId, setDragOverId] = useState<string | null>(null);
  const draggedApp = useRef<RunningApp | null>(null);
  // Per-row drag payloads: a browser tab, an OS window, or a macOS child. Exactly
  // one is set at a time (each drag-start clears the others) so onDropToBundle
  // registers the right resource kind (FR-2.2 / FR-9.6 / FR-2.8 / §13.3).
  const draggedTab = useRef<{ browser: string; handle: string; title: string } | null>(null);
  const draggedWin = useRef<{ app: string; handle: string; title: string } | null>(null);
  const draggedChild = useRef<{ kind: string; name: string; target: string; handle: string } | null>(null);

  const [report, setReport] = useState<RestoreReport | null>(null);
  const [reportBundleId, setReportBundleId] = useState<string | null>(null);
  const [restoringId, setRestoringId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // ── Global group prompt bar (@mention → a group's live claude terminal) ──
  const [claude, setClaude] = useState<ClaudeStatus | null>(null);
  const [promptInput, setPromptInput] = useState("");
  // The group the bar routes to when the prompt carries no @mention — sticky to
  // the last group sent to, so a follow-up line needs no re-mention.
  const [promptTarget, setPromptTarget] = useState<string | null>(null);
  const [promptSending, setPromptSending] = useState(false);
  const [promptError, setPromptError] = useState<string | null>(null);
  // Bundle ids whose embedded terminal is mounted (started). A group's terminal
  // opens on first send (or via its "터미널" toggle) and then streams live.
  const [openTerms, setOpenTerms] = useState<Set<string>>(new Set());
  const [showClaudeModal, setShowClaudeModal] = useState(false);
  const [keyInput, setKeyInput] = useState("");

  // @mention autocomplete keyboard state: which suggestion is highlighted and
  // whether the user dismissed the menu (Esc) for the current `@partial` query.
  const [mentionActive, setMentionActive] = useState(0);
  const [mentionDismissed, setMentionDismissed] = useState(false);

  // Auto-create-session-on-send: when a prompt targets a group with no claude
  // session, hold the pending {group, text} and ask for a working folder. On
  // confirm we spawn a new PTY session there and send the prompt as its first
  // turn — "명령을 내리면 파워쉘(세션)이 그룹에 추가"된다.
  const [pendingPrompt, setPendingPrompt] = useState<{
    bundleId: string;
    text: string;
  } | null>(null);
  const [sessionCwd, setSessionCwd] = useState("");
  const [creatingSession, setCreatingSession] = useState(false);

  // Boot sequence. The splash overlay blocks all input; hold it until the
  // essential shell data has loaded — saved groups, Claude status, and the
  // first running-apps scan (the slow OS call the splash exists to cover) — so
  // the window never appears interactive while still empty. A minimum on-screen
  // time keeps it from flickering on a fast start; a hard cap guarantees the
  // splash is always lifted even if a backend call hangs.
  useEffect(() => {
    let cancelled = false;
    let finished = false;
    const startedAt = performance.now();
    // Floor the visible time to the length of the ported intro animation so the
    // full sequence (fake-loading counter → "DEV" morph → hero letters decoding
    // into the wordmark, ~4.9s at the sped-up timing) plays before the fade,
    // rather than being cut short the instant the (usually faster) boot data
    // lands. The hard cap still guarantees the splash lifts if boot stalls.
    const MIN_SPLASH_MS = 5100;
    const MAX_SPLASH_MS = 8000;
    const FADE_MS = 0; // switch to the app instantly when the intro ends (no fade)

    // Hide the splash, then unmount it. Idempotent: whichever of the boot
    // completion or the hard-cap fires first wins; the other is a no-op.
    const finish = () => {
      if (cancelled || finished) return;
      finished = true;
      setSplashOut(true);
      window.setTimeout(() => {
        if (!cancelled) setBooting(false);
      }, FADE_MS);
    };

    // Safety net: never strand the UI behind the splash if boot stalls.
    const hardCap = window.setTimeout(finish, MAX_SPLASH_MS);

    (async () => {
      await Promise.allSettled([
        getBundles()
          .then((b) => {
            if (!cancelled) setBundles(b);
          })
          .catch((e) => {
            if (!cancelled) setError(String(e));
          }),
        claudeStatus()
          .then((c) => {
            if (!cancelled) setClaude(c);
          })
          .catch(() => {
            if (!cancelled) setClaude(null);
          }),
        getLayout()
          .then((l) => {
            if (cancelled || !l) return;
            if (l.card_height > 0) cardHeightRef.current = l.card_height;
            if (l.panel_width > 0) setSavedPanelWidth(l.panel_width);
          })
          .catch(() => {
            /* no saved layout → CSS defaults */
          }),
        claudeLocalUsage()
          .then((u) => {
            if (cancelled) return;
            setUsage(u);
            setUsageErr(null);
          })
          .catch((e) => {
            // Keep any prior data; record the reason so the meter shows "—"
            // with a cause rather than a misleading zero.
            if (!cancelled) setUsageErr(errText(e));
          })
          .finally(() => {
            if (!cancelled) setUsageLoading(false);
          }),
        // Trigger the one-time macOS Accessibility prompt at launch and record
        // whether it's granted, so per-instance window lists work without the
        // user hunting through System Settings (a no-op / true on Windows).
        requestAccessibility()
          .then((ok) => {
            if (!cancelled) setAccessNeeded(!ok);
          })
          .catch(() => {}),
        refreshRunning(), // first running-apps scan
      ]);

      // Floor the visible time so the splash reads as intentional, not a blip.
      const elapsed = performance.now() - startedAt;
      if (elapsed < MIN_SPLASH_MS) {
        await new Promise((r) => setTimeout(r, MIN_SPLASH_MS - elapsed));
      }
      window.clearTimeout(hardCap);
      finish();
    })();

    return () => {
      cancelled = true;
      window.clearTimeout(hardCap);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Apply the saved sidebar width once, imperatively (see savedPanelWidth note).
  // Runs after the value loads and the aside is mounted; CSS `resize` owns it
  // afterwards, so we never re-apply and cancel an in-progress drag.
  useEffect(() => {
    if (sidebarRef.current && savedPanelWidth != null) {
      sidebarRef.current.style.width = `${savedPanelWidth}px`;
    }
  }, [savedPanelWidth]);

  // Persist the sidebar width when the user drag-resizes it (debounced). The
  // observer only writes — it never sets React state, so it can't fight the drag.
  useEffect(() => {
    const el = sidebarRef.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    let timer: number | undefined;
    let last = 0;
    const ro = new ResizeObserver((entries) => {
      const w = entries[0]?.contentRect.width;
      if (!w || Math.abs(w - last) < 1) return;
      last = w;
      window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        savePanelLayout(w, cardHeightRef.current).catch(() => {});
      }, 500);
    });
    ro.observe(el);
    return () => {
      ro.disconnect();
      window.clearTimeout(timer);
    };
  }, []);

  const refreshRunning = async (silent = false) => {
    // Never re-render the running list mid-drag: replacing/reordering the
    // items cancels the in-flight native HTML5 drag before it can drop. The
    // background poll yields to an active drag; a non-silent refresh (boot) runs
    // when no drag is in flight, so it isn't gated.
    if (silent && draggedApp.current) return;
    try {
      const apps = await listRunningApps();
      if (silent && draggedApp.current) return; // a drag began while fetching
      setRunningApps(apps);
    } catch (e) {
      // A background poll shouldn't flash the error banner every tick — only
      // surface failures from an explicit (non-silent) refresh.
      if (!silent) setError(String(e));
    }
  };

  // Live status: poll running apps every second so the left panel stays current
  // automatically. Silent (no spinner / no error banner).
  useEffect(() => {
    const id = setInterval(() => refreshRunning(true), 1000);
    return () => clearInterval(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Keep the "today" usage meter live: the CLI appends each turn's usage to its
  // transcript as a terminal works, so a short poll makes the amber readout
  // climb in near real time. The backend caches per-file parses, so this only
  // re-reads the one transcript that changed (boot did the first fetch).
  useEffect(() => {
    const id = window.setInterval(() => {
      claudeLocalUsage()
        .then((u) => {
          setUsage(u);
          setUsageErr(null);
        })
        .catch((e) => setUsageErr(errText(e)));
    }, 3_000);
    return () => window.clearInterval(id);
  }, []);

  const filteredApps = runningApps.filter((a) =>
    a.name.toLowerCase().includes(filter.trim().toLowerCase())
  );

  const activate = (target: string) => {
    setError(null);
    activateApp(target).catch((e) => setError(String(e)));
  };

  // Activate exactly one window/tab by its opaque handle (FR-2.8/AC-20). Surface
  // failures (e.g. the window was closed between polls) in the error banner.
  const activateWin = (handle: string) => {
    setError(null);
    activateWindow(handle).catch((e) => setError(errText(e)));
  };

  const toggleExpanded = (key: string) =>
    setExpandedApps((cur) => {
      const next = new Set(cur);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });

  // ── Windows browser tabs (UIA) ──────────────────────────────────────────
  // Lazily read a browser's open tabs when its group is expanded (FR-9.6 /
  // FR-10.12) — never on the poll (FR-10.7). `reveal=false` (expand) never
  // foregrounds the browser; `reveal=true` (the explicit "앞으로 가져와 다시
  // 읽기" button) foregrounds each window first so a background window's lazily
  // -built tab tree becomes readable.
  const fetchTabs = async (name: string, reveal = false) => {
    setTabsLoadingApps((cur) => new Set(cur).add(name));
    try {
      const tabs = await listBrowserTabs(name, reveal);
      setTabsByApp((cur) => ({ ...cur, [name]: tabs }));
    } catch (e) {
      setError(errText(e));
    } finally {
      setTabsLoadingApps((cur) => {
        const next = new Set(cur);
        next.delete(name);
        return next;
      });
    }
  };

  // Activate exactly one browser tab (FR-2.8/FR-4.2/AC-20): focus that tab, then
  // re-read the strip so the active-tab dot reflects the switch.
  const activateTabRow = (handle: string, appName: string) => {
    setError(null);
    activateTab(handle)
      .then(() => fetchTabs(appName))
      .catch((e) => setError(errText(e)));
  };

  // Expand/collapse a Windows browser group and lazily read its tabs on open
  // (never on the poll). Window rows come from the polled app.windows.
  const handleToggleTabApp = (app: RunningApp, isBrowser: boolean) => {
    const key = appKey(app);
    const willExpand = !expandedApps.has(key);
    toggleExpanded(key);
    if (willExpand && isBrowser) void fetchTabs(app.name);
  };

  // ── macOS children (tabs/folders/windows) ───────────────────────────────
  const fetchChildren = async (app: RunningApp) => {
    const key = appKey(app);
    const seq = (childFetchSeq.current[key] ?? 0) + 1;
    childFetchSeq.current[key] = seq;
    setLoadingApp((s) => new Set(s).add(key));
    try {
      const kids = await listAppChildren(app.name, app.bundle_id);
      if (draggedChild.current || draggedApp.current) return; // don't yank a drag
      if (childFetchSeq.current[key] !== seq) return; // superseded by a newer fetch
      setAppChildren((c) => ({ ...c, [key]: kids }));
    } catch {
      if (childFetchSeq.current[key] === seq)
        setAppChildren((c) => ({ ...c, [key]: [] }));
    } finally {
      setLoadingApp((s) => {
        const n = new Set(s);
        n.delete(key);
        return n;
      });
    }
  };

  const handleToggleApp = (app: RunningApp) => {
    const key = appKey(app);
    const willExpand = !expandedApps.has(key);
    toggleExpanded(key);
    if (willExpand && appChildren[key] === undefined) void fetchChildren(app);
  };

  const activateChildItem = (w: RunningWindow) => {
    setError(null);
    activateChild(w.kind, w.target, w.handle).catch((e) => setError(errText(e)));
  };

  // ── Saved-resource activation dispatch (exact tab/window focus) ──────────
  // A WindowRef re-focuses the exact HWND (verified by title, scoped to the
  // owning app); a BrowserTabLive re-focuses the exact tab by its stored handle
  // (re-matched by title, never opening a URL, FR-9.7); a Folder opens its path;
  // everything else is an app-level launch/focus. Passing a window handle to
  // activateApp would fail — hence the per-kind dispatch (regression cause when
  // the old wiring was dropped).
  const activateResource = (r: Resource) => {
    setError(null);
    const descriptor = r.identity.descriptor;
    const reopen = r.identity.reopen_info ?? descriptor;
    let p: Promise<void>;
    switch (r.kind) {
      case "BrowserTabLive":
        p = activateTabResource(r.identity.hint ?? "", descriptor);
        break;
      case "BrowserTab":
        p = activateChild("tab", descriptor, reopen);
        break;
      case "Folder":
        p = descriptor.includes(UNIT_SEP)
          ? activateChild("folder", "", descriptor)
          : activateChild("folder", descriptor, "");
        break;
      case "WindowRef":
        p = IS_MACOS
          ? activateWindow(reopen)
          : activateWindowResource(r.identity.hint ?? "", descriptor, reopen);
        break;
      default:
        p = activateApp(reopen);
    }
    p.catch((e) => setError(errText(e)));
  };

  // Remove a single saved resource (the × on each item) from a group.
  const handleRemoveResource = async (bundleId: string, resourceId: string) => {
    setError(null);
    try {
      setBundles(await removeResource(bundleId, resourceId));
    } catch (e) {
      setError(errText(e));
    }
  };

  // ── Per-row drag starts (register just that tab/window/child, not the app) ──
  const onDragStartTab = (e: React.DragEvent, browser: string, w: RunningWindow) => {
    draggedTab.current = { browser, handle: w.handle, title: w.title };
    draggedApp.current = null;
    draggedWin.current = null;
    draggedChild.current = null;
    e.dataTransfer.effectAllowed = "copy";
    e.dataTransfer.setData("text/plain", w.title);
  };

  const onDragStartWin = (e: React.DragEvent, app: string, w: RunningWindow) => {
    draggedWin.current = { app, handle: w.handle, title: w.title };
    draggedApp.current = null;
    draggedTab.current = null;
    draggedChild.current = null;
    e.dataTransfer.effectAllowed = "copy";
    e.dataTransfer.setData("text/plain", w.title);
  };

  const onDragStartChild = (e: React.DragEvent, w: RunningWindow) => {
    draggedChild.current = {
      kind: w.kind,
      name: w.title,
      target: w.target,
      handle: w.handle,
    };
    draggedApp.current = null;
    draggedTab.current = null;
    draggedWin.current = null;
    e.dataTransfer.effectAllowed = "copy";
    e.dataTransfer.setData("text/plain", w.title);
  };

  const openGroupModal = () => {
    setError(null);
    setNewGroupName("");
    setShowGroupModal(true);
  };
  const closeGroupModal = () => setShowGroupModal(false);

  const handleCreateGroup = async () => {
    setError(null);
    const name = newGroupName.trim() || `Group ${bundles.length + 1}`;
    try {
      setBundles(await createBundle(name));
      setNewGroupName("");
      setShowGroupModal(false);
    } catch (e) {
      setError(String(e));
    }
  };

  const handleDeleteGroup = async (bundleId: string) => {
    setError(null);
    try {
      setBundles(await deleteBundle(bundleId));
      setConfirmDeleteId(null);
      if (reportBundleId === bundleId) {
        setReport(null);
        setReportBundleId(null);
      }
    } catch (e) {
      setError(String(e));
    }
  };

  const onDragStartApp = (e: React.DragEvent, app: RunningApp) => {
    draggedApp.current = app;
    draggedTab.current = null; // never drop with a stale per-row ref
    draggedWin.current = null;
    draggedChild.current = null;
    e.dataTransfer.effectAllowed = "copy";
    e.dataTransfer.setData("text/plain", app.name);
  };

  const onDropToBundle = async (e: React.DragEvent, bundle: WorkBundle) => {
    e.preventDefault();
    setDragOverId(null);
    const tab = draggedTab.current;
    const win = draggedWin.current;
    const child = draggedChild.current;
    const app = draggedApp.current;
    draggedTab.current = null;
    draggedWin.current = null;
    draggedChild.current = null;
    draggedApp.current = null;
    setError(null);
    try {
      // A macOS child (tab/folder/window) registers as its own resource kind; a
      // Windows tab → focus-only live tab; a Windows window → WindowRef; a whole
      // app → app launch (FR-2.2 / FR-9.6 / FR-2.8 / §13.3).
      if (child) {
        setBundles(
          await addChildResource(
            bundle.id,
            child.kind,
            child.name,
            child.target,
            child.handle
          )
        );
      } else if (tab) {
        setBundles(
          await addTabResource(bundle.id, tab.title, tab.browser, tab.handle)
        );
      } else if (win) {
        setBundles(
          await addWindowResource(bundle.id, win.title, win.app, win.handle)
        );
      } else if (app) {
        const target = app.bundle_id ?? app.name;
        setBundles(await addAppResource(bundle.id, app.name, target));
      }
    } catch (e) {
      setError(String(e));
    }
  };

  const handleRestore = async (bundle: WorkBundle) => {
    setError(null);
    setReport(null);
    setReportBundleId(bundle.id);
    setRestoringId(bundle.id);
    try {
      setReport(await restoreBundle(bundle));
    } catch (e) {
      setError(String(e));
    } finally {
      setRestoringId(null);
    }
  };

  // ── Claude console handlers ────────────────────────────────────────
  const openClaudeModal = () => {
    setKeyInput(""); // never prefill: the key is write-only, never read back
    setShowClaudeModal(true);
  };
  const closeClaudeModal = () => setShowClaudeModal(false);

  const handleSaveKey = async () => {
    try {
      setClaude(await setClaudeApiKey(keyInput.trim()));
      setKeyInput("");
      setShowClaudeModal(false);
      setPromptError(null);
    } catch (e) {
      setPromptError(errText(e));
    }
  };

  const handleSelectModel = async (model: string) => {
    try {
      setClaude(await setClaudeModel(model));
    } catch (e) {
      setPromptError(errText(e));
    }
  };

  // Open (mount + start) a group's embedded terminal. Idempotent — the backend
  // reuses a live PTY, and the Set membership drives whether GroupTerminal is
  // mounted. Also makes the group the sticky prompt target.
  const openGroupTerminal = (bundleId: string) => {
    setOpenTerms((prev) => {
      if (prev.has(bundleId)) return prev;
      const next = new Set(prev);
      next.add(bundleId);
      return next;
    });
    setPromptTarget(bundleId);
  };

  const closeGroupTerminal = (bundleId: string) => {
    setOpenTerms((prev) => {
      if (!prev.has(bundleId)) return prev;
      const next = new Set(prev);
      next.delete(bundleId);
      return next;
    });
  };

  // Route the bottom prompt bar to a group's live claude. A leading `@group`
  // picks the target (else the sticky last target); the rest is submitted to
  // that group's PTY as one line. If the group has no session yet, we ask for a
  // working folder and spawn one there (see createSessionAndSend).
  const handleSendPrompt = async () => {
    const raw = promptInput.trim();
    if (!raw || promptSending) return;

    const { bundle: mentioned, body } = parseMention(raw, bundles);
    const target =
      mentioned ?? bundles.find((b) => b.id === promptTarget) ?? undefined;
    if (!target) {
      setPromptError("대상 그룹을 지정하세요 — 예: @그룹명 메시지");
      return;
    }
    const text = mentioned ? body : raw; // only strip the @token when it matched
    if (!text.trim()) {
      setPromptError("보낼 내용을 입력하세요");
      return;
    }
    const ref = groupSessionRef(target);
    if (!ref) {
      // No claude session in this group yet — ask for a working folder, then
      // spawn a session there and send (user choice: pick a folder each time).
      setPromptError(null);
      setSessionCwd("");
      setPendingPrompt({ bundleId: target.id, text: text.trim() });
      return;
    }

    setPromptError(null);
    setPromptSending(true);
    try {
      openGroupTerminal(target.id); // mount/stream the terminal if not already
      // idempotent: reuses a live PTY; cwd is the transcript-less fallback.
      await startInteractiveSession(ref, undefined, groupSessionCwd(target));
      await submitInteractiveLine(ref, text);
      setPromptTarget(target.id);
      setPromptInput("");
    } catch (e) {
      setPromptError(errText(e));
    } finally {
      setPromptSending(false);
    }
  };

  // Spawn a brand-new claude PTY session in the chosen folder, attach it to the
  // group as a CodingSession, open its terminal, and send the pending prompt as
  // the session's first turn. Reached when a prompt targets a session-less group.
  const createSessionAndSend = async () => {
    if (!pendingPrompt || creatingSession) return;
    const cwd = sessionCwd.trim();
    if (!cwd) {
      setPromptError("작업 폴더 경로를 입력하세요.");
      return;
    }
    const bundle = bundles.find((b) => b.id === pendingPrompt.bundleId);
    if (!bundle) {
      setPendingPrompt(null);
      return;
    }
    const sessionId =
      typeof crypto !== "undefined" && "randomUUID" in crypto
        ? crypto.randomUUID()
        : `${Date.now()}-${Math.random().toString(16).slice(2)}`;
    const text = pendingPrompt.text;
    setCreatingSession(true);
    setPromptError(null);
    try {
      // Start the interactive PTY WITHOUT seeding the prompt — a brand-new TUI
      // swallows an immediate Enter, so we submit the first line only once the
      // REPL is ready (submitWhenReady, fired below). The terminal we open next
      // reuses this live session rather than spawning a duplicate.
      await startNewInteractive(cwd, sessionId);
      const base = cwd.split(/[\\/]/).filter(Boolean).pop() ?? cwd;
      const created: Resource = {
        id: sessionId,
        display_name: `${base} (claude)`,
        kind: "CodingSession",
        identity: {
          kind: "CodingSession",
          // Persist the working folder in `hint` so the session stays restorable
          // even if it never writes a transcript (a never-run session can then
          // be re-opened fresh in this folder instead of a dead "no transcript"
          // error after a restart).
          descriptor: sessionId,
          hint: cwd,
          reopen_info: sessionId,
        },
        order: bundle.resources.length,
      };
      const updated = bundles.map((b) =>
        b.id === bundle.id ? { ...b, resources: [...b.resources, created] } : b
      );
      await saveBundles(updated);
      setBundles(updated);
      openGroupTerminal(bundle.id); // mount + stream the new terminal
      setPromptTarget(bundle.id);
      setPromptInput("");
      setPendingPrompt(null);
      setSessionCwd("");
      // Submit the first prompt once the freshly-booted REPL is ready (async;
      // the terminal is already live so the user sees it type + send). Skipped
      // when created with no prompt (the "New session" button just opens a REPL).
      if (text) void submitWhenReady(sessionId, text);
    } catch (e) {
      setPromptError(errText(e));
    } finally {
      setCreatingSession(false);
    }
  };

  const cancelPendingPrompt = () => {
    setPendingPrompt(null);
    setSessionCwd("");
  };

  // Delete a group's claude session(s): kill the live PTY (best-effort — a
  // never-run/already-dead session just no-ops), drop the CodingSession
  // resource(s) from the group, and collapse the terminal view. The group then
  // has no session, so the "New session" affordance below lets it be recreated.
  const handleDeleteSession = async (bundleId: string) => {
    setConfirmKillId(null);
    setPromptError(null);
    const target = bundles.find((b) => b.id === bundleId);
    if (!target) return;
    const sessions = target.resources.filter((r) => r.kind === "CodingSession");
    try {
      let latest = bundles;
      for (const s of sessions) {
        // Kill first so the process doesn't linger after its resource is gone.
        await stopInteractiveSession(s.identity.descriptor).catch(() => {});
        latest = await removeResource(bundleId, s.id);
      }
      setBundles(latest);
      closeGroupTerminal(bundleId);
    } catch (e) {
      setPromptError(errText(e));
    }
  };

  // Open the folder picker to (re)create a session for a group WITHOUT sending a
  // prompt — the empty text makes createSessionAndSend just start the REPL. This
  // is the counterpart to handleDeleteSession so a terminal can be freely
  // deleted and made again.
  const openNewSession = (bundleId: string) => {
    setPromptError(null);
    setSessionCwd("");
    setPendingPrompt({ bundleId, text: "" });
  };

  // Live @mention autocomplete: when the caret is on a trailing `@partial`, show
  // matching group names; picking one rewrites the token to the full name.
  const mentionQuery = (() => {
    const m = promptInput.match(/@([^\s@]*)$/);
    return m ? m[1] : null;
  })();
  const mentionSuggestions =
    mentionQuery !== null
      ? bundles.filter((b) =>
          b.name.toLowerCase().includes(mentionQuery.toLowerCase())
        )
      : [];
  // Reset the highlighted item / un-dismiss whenever the query changes.
  useEffect(() => {
    setMentionActive(0);
    setMentionDismissed(false);
  }, [mentionQuery]);
  const showMentions =
    mentionQuery !== null && !mentionDismissed && mentionSuggestions.length > 0;
  // Clamp in case the suggestion list shrank since the last keypress.
  const activeMention = Math.min(
    mentionActive,
    Math.max(0, mentionSuggestions.length - 1)
  );
  const applyMention = (b: WorkBundle) => {
    setPromptInput((cur) => cur.replace(/@[^\s@]*$/, `@${b.name} `));
    setMentionDismissed(false);
  };

  const targetName =
    bundles.find((b) => b.id === promptTarget)?.name ?? null;

  return (
    <div className={`app-shell${IS_MACOS ? "" : " wc-chrome"}`}>
    {!IS_MACOS && <WindowControls />}
    {booting && <Splash hiding={splashOut} />}
    <div className="app">
      <aside className="sidebar" ref={sidebarRef}>
        <header className="sidebar-header" data-tauri-drag-region>
          <h1 data-tauri-drag-region>Running Apps</h1>
        </header>
        <input
          className="search"
          placeholder="Search apps…"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
        <ul className="running-list">
          {filteredApps.map((app) => {
            // ── Windows (UIA) expansion ──────────────────────────────────
            // A browser expands to its live tabs (read lazily on open, FR-9.6);
            // any other app expands to its OS windows. Falls back to the window
            // list when a browser's tabs can't be read (background window).
            if (!IS_MACOS) {
              const key = appKey(app);
              const wins = app.windows ?? [];
              const multi = wins.length > 1;
              const isBrowser = BROWSER_APPS.has(app.name.toLowerCase());
              const isExpanded = expandedApps.has(key);
              // A browser is always expandable (to reveal its tabs); other apps
              // expand only when they own more than one window.
              const expandable = multi || isBrowser;
              const tabs = tabsByApp[app.name];
              const tabsLoading = tabsLoadingApps.has(app.name);
              // Show TABS for an expanded browser once some were read; otherwise
              // fall back to the OS windows so multi-window apps still list every
              // window.
              const showTabs = isBrowser && (tabs?.length ?? 0) > 0;
              const rows = showTabs ? tabs! : wins;
              const count = showTabs ? tabs!.length : wins.length;
              // Clicking an app: expandable (browser, or >1 window) → toggle the
              // list; exactly 1 window → activate it; otherwise activate by name.
              const onAppClick = () => {
                if (expandable) {
                  handleToggleTabApp(app, isBrowser);
                } else if (wins.length === 1) {
                  activateWin(wins[0].handle);
                } else {
                  activate(app.bundle_id ?? app.name);
                }
              };
              return (
                <Fragment key={key}>
                  <li
                    className={`running-item${expandable ? " has-windows" : ""}${
                      expandable && isExpanded ? " expanded" : ""
                    }`}
                    draggable
                    onDragStart={(e) => onDragStartApp(e, app)}
                    onDragEnd={() => {
                      // Clear the drag ref even when the drag is cancelled
                      // (dropped outside a group), so the paused poll resumes.
                      draggedApp.current = null;
                      setDragOverId(null);
                    }}
                    onClick={onAppClick}
                    title={
                      isBrowser
                        ? "Click: show tabs · Drag: add to a group"
                        : multi
                        ? "Click: show windows · Drag: add to a group"
                        : "Click: bring to front · Drag: add to a group"
                    }
                  >
                    <AppIcon target={app.bundle_id ?? app.name} />
                    <span className="running-name">{app.name}</span>
                    {expandable && count > 0 && (
                      <span className="running-count" aria-hidden>
                        {isExpanded ? "▾" : "▸"} {count}
                      </span>
                    )}
                  </li>
                  {expandable &&
                    isExpanded &&
                    rows.map((w, i) => (
                      <li
                        key={`${key}${w.handle}${i}`}
                        className="running-window"
                        draggable
                        onDragStart={
                          showTabs
                            ? (e) => onDragStartTab(e, app.name, w)
                            : (e) => onDragStartWin(e, app.name, w)
                        }
                        onDragEnd={() => {
                          draggedTab.current = null;
                          draggedWin.current = null;
                          setDragOverId(null);
                        }}
                        onClick={() =>
                          showTabs
                            ? activateTabRow(w.handle, app.name)
                            : activateWin(w.handle)
                        }
                        title={
                          showTabs
                            ? "Click: switch to this tab · Drag: add to a group"
                            : "Click: bring this window to front · Drag: add this window to a group"
                        }
                      >
                        <span
                          className={`win-dot${w.is_focused ? " active" : ""}`}
                          aria-hidden
                        />
                        <span className="win-title">{w.title}</span>
                      </li>
                    ))}
                  {isBrowser && isExpanded && !showTabs && (
                    <li className="running-window running-window-hint">
                      <span className="win-title">
                        {tabsLoading
                          ? "탭 읽는 중…"
                          : wins.length > 0
                          ? "백그라운드 창이라 탭을 읽지 못했습니다 — 창 목록을 표시합니다"
                          : "열린 탭을 찾을 수 없습니다"}
                      </span>
                      {!tabsLoading && (
                        <button
                          className="ghost reveal-tabs"
                          onClick={(e) => {
                            e.stopPropagation();
                            void fetchTabs(app.name, true);
                          }}
                          title="브라우저 창을 앞으로 가져와 탭을 다시 읽습니다"
                        >
                          ⟳ 앞으로 가져와 다시 읽기
                        </button>
                      )}
                    </li>
                  )}
                </Fragment>
              );
            }
            // ── macOS expansion: children (tabs / folders / windows) ─────
            const key = appKey(app);
            const isExpanded = expandedApps.has(key);
            const kids = appChildren[key];
            const loading = loadingApp.has(key);
            // Safari/Chrome (tabs) and Finder (folders) read via Apple Events;
            // every other app's windows come from the System Events pass that
            // needs Accessibility. Only those show the permission hint when the
            // list comes back empty for lack of access.
            const winApp = !["Safari", "Google Chrome", "Finder"].includes(
              app.name
            );
            return (
              <Fragment key={key}>
                <li
                  className={`running-item has-windows${
                    isExpanded ? " expanded" : ""
                  }`}
                  draggable
                  onDragStart={(e) => onDragStartApp(e, app)}
                  onDragEnd={() => {
                    draggedApp.current = null;
                    setDragOverId(null);
                  }}
                  onClick={() => activate(app.bundle_id ?? app.name)}
                  title="Click: bring to front · Drag: add to a group"
                >
                  <AppIcon target={app.bundle_id ?? app.name} />
                  <span className="running-name">{app.name}</span>
                  {/* Chevron reveals per-tab/per-window items (FR-2.2/2.8). */}
                  <button
                    className="running-expand"
                    aria-label={isExpanded ? "Hide items" : "Show items"}
                    title={isExpanded ? "Hide windows / tabs" : "Show windows / tabs"}
                    onClick={(e) => {
                      e.stopPropagation();
                      handleToggleApp(app);
                    }}
                  >
                    {isExpanded ? "▾" : "▸"}
                    {kids && kids.length > 0 ? ` ${kids.length}` : ""}
                  </button>
                </li>
                {isExpanded &&
                  (loading && !kids ? (
                    <li className="running-window muted">Loading…</li>
                  ) : kids && kids.length > 0 ? (
                    kids.map((w, i) => (
                      <li
                        key={`${key}-${w.handle}-${i}`}
                        className="running-window"
                        draggable
                        onDragStart={(e) => onDragStartChild(e, w)}
                        onDragEnd={() => {
                          draggedChild.current = null;
                          setDragOverId(null);
                        }}
                        onClick={() => activateChildItem(w)}
                        title={
                          w.kind === "tab"
                            ? "Click: focus this tab · Drag: add to a group"
                            : w.kind === "folder"
                            ? "Click: open this folder · Drag: add to a group"
                            : "Click: bring this window to front · Drag: add to a group"
                        }
                      >
                        <span
                          className={`win-dot${w.is_focused ? " active" : ""}`}
                          aria-hidden
                        />
                        <span className="win-title">{w.title}</span>
                      </li>
                    ))
                  ) : accessNeeded && winApp ? (
                    <li
                      className="running-window muted access-needed"
                      onClick={async (e) => {
                        e.stopPropagation();
                        const ok = await requestAccessibility();
                        setAccessNeeded(!ok);
                        if (ok) void fetchChildren(app);
                      }}
                      title="Enable vibe-control under System Settings → Privacy & Security → Accessibility, then click to retry"
                    >
                      ⚠ Grant Accessibility to list windows
                    </li>
                  ) : (
                    <li className="running-window muted">No windows or tabs</li>
                  ))}
              </Fragment>
            );
          })}
          {filteredApps.length === 0 && (
            <li className="empty">No apps to show</li>
          )}
        </ul>
      </aside>

      <main className="main">
        <header className="main-header" data-tauri-drag-region>
          <h2 data-tauri-drag-region>vibe-control</h2>
          <div className="header-actions">
            <button className="rec add-group" onClick={openGroupModal}>
              Add Group
            </button>
          </div>
        </header>

        {(() => {
          // KO-II usage meter — always rendered (like the original) so the LCD
          // face never collapses. Four distinct states, so an un-fetched value
          // is never shown as a misleading "0":
          //   loading      → a fetch is in flight, no data yet
          //   unconfigured → no Bedrock key ("connect" guidance)
          //   error        → last fetch failed / no data ("—" + reason)
          //   ready        → real numbers (a genuine 0 shows as "0")
          // State priority (highest first): loading → error → unconfigured →
          // ready. A fetch in flight wins; then a failed/empty fetch shows
          // "—"+reason (never a stale or zero value); then no-key guidance;
          // finally real numbers (a genuine 0 shows as "0").
          const meterState = usageLoading
            ? "loading"
            : usageErr
              ? "error"
              : usage && !usage.configured
                ? "unconfigured"
                : usage && usage.configured
                  ? "ready"
                  : "loading";
          const ready = meterState === "ready" && usage != null;
          const ledOn = usage?.configured ?? claude?.configured ?? false;
          const model = modelLabel(claude?.model ?? DEFAULT_MODEL);
          // Scope note only for the non-ready states (guidance / loading /
          // error). When real numbers are shown, no caption — the LCD speaks for
          // itself (the `{note && …}` render guard hides the empty string).
          const note = ready
            ? ""
            : meterState === "unconfigured"
              ? "※ 아직 로컬 Claude CLI 기록이 없습니다 — 그룹 터미널에서 대화하면 집계됩니다."
              : meterState === "loading"
                ? "※ 사용량을 불러오는 중입니다…"
                : `※ 조회 실패: ${usageErr ?? "데이터 없음"}`;
          return (
            <>
            <div className="ko-screen">
              <div className="ko-screen-glass">
                <div className="ko-usage">
                  <div className="ko-usage-total">
                    <span
                      className={`ko-led ${ledOn ? "on" : "off"}`}
                      aria-hidden
                    />
                    <b title={ready ? `$${usage!.total_cost_usd.toFixed(4)}` : undefined}>
                      {ready ? fmtUSD(usage!.total_cost_usd) : "—"}
                    </b>
                    <span className="ko-usage-model">{model}</span>
                  </div>
                  <div className="ko-legend">
                    {ready ? (
                      <>
                        <span className="ko-chip in">
                          <em />
                          IN {fmtTokens(usage!.input_tokens)}
                        </span>
                        <span className="ko-chip out">
                          <em />
                          OUT {fmtTokens(usage!.output_tokens)}
                        </span>
                        <span className="ko-chip req">
                          <em />
                          CACHE{" "}
                          {fmtTokens(
                            usage!.cache_creation_tokens +
                              usage!.cache_read_tokens
                          )}
                        </span>
                        <span className="ko-chip">오늘 · 로컬 CLI</span>
                      </>
                    ) : (
                      <span className="ko-chip">
                        {meterState === "loading"
                          ? "조회 중…"
                          : meterState === "unconfigured"
                            ? "기록 없음"
                            : "조회 실패"}
                      </span>
                    )}
                  </div>
                </div>
                <div className="ko-side">
                  <div
                    className="ko-last"
                    title={
                      ready
                        ? `오늘 누적 ${fmtFull(usage!.total_tokens)} 토큰 · ${usage!.messages} 턴`
                        : undefined
                    }
                  >
                    <b>{ready ? fmtTokens(usage!.total_tokens) : "—"}</b>
                    <i>tokens today</i>
                  </div>
                  <div className="ko-mini">
                    <span>
                      <b>{String(bundles.length).padStart(2, "0")}</b>
                      <i>GRP</i>
                    </span>
                    <span>
                      <b>{String(runningApps.length).padStart(2, "0")}</b>
                      <i>APP</i>
                    </span>
                  </div>
                </div>
                <span className="ko-screen-glare" aria-hidden />
              </div>
            </div>
            {note && <div className="ko-note">{note}</div>}
            </>
          );
        })()}

        {error && <div className="error">{error}</div>}

        <div className="group-grid">
          {bundles.length === 0 && (
            <div className="placeholder">
              No groups yet. Create one with “Add Group”, then drag running apps
              from the left into it.
            </div>
          )}
          {bundles.map((b) => {
            // Coding sessions are represented by the live terminal below, not as
            // resource rows — the list shows only apps/windows/folders.
            const appResources = b.resources.filter(
              (r) => r.kind !== "CodingSession"
            );
            return (
            <section
              key={b.id}
              className={`group-card ${dragOverId === b.id ? "drag-over" : ""}`}
              onDragOver={(e) => {
                e.preventDefault();
                if (dragOverId !== b.id) setDragOverId(b.id);
              }}
              onDragLeave={() =>
                setDragOverId((cur) => (cur === b.id ? null : cur))
              }
              onDrop={(e) => onDropToBundle(e, b)}
            >
              <header className="group-card-header">
                <span className="group-name" title={b.name}>
                  {b.name}
                </span>
                <span className="group-count">{b.resources.length}</span>
                <div className="group-actions">
                  <button
                    className="mini"
                    onClick={() => handleRestore(b)}
                    disabled={restoringId === b.id || b.resources.length === 0}
                  >
                    {restoringId === b.id ? "Restoring…" : "Restore"}
                  </button>
                  {confirmDeleteId === b.id ? (
                    <>
                      <button
                        className="mini danger"
                        onClick={() => handleDeleteGroup(b.id)}
                      >
                        Confirm
                      </button>
                      <button
                        className="mini"
                        onClick={() => setConfirmDeleteId(null)}
                      >
                        Cancel
                      </button>
                    </>
                  ) : (
                    <span
                      className="close-group"
                      role="button"
                      tabIndex={0}
                      onClick={() => setConfirmDeleteId(b.id)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" || e.key === " ") {
                          e.preventDefault();
                          setConfirmDeleteId(b.id);
                        }
                      }}
                      title="Close (delete) this group"
                      aria-label="Close group"
                    >
                      ×
                    </span>
                  )}
                </div>
              </header>

              <ul className="resource-list">
                {appResources.length === 0 && (
                  <li className="empty drop-hint">Drag apps here</li>
                )}
                {appResources.map((r) => {
                  // Both app-level entries and saved live tabs re-focus exactly;
                  // dispatch is by kind (activateResource), never a bare activate()
                  // that would mis-handle a window handle or tab (regression cause).
                  const canActivate =
                    isActivatable(r.kind) || isSavedTab(r.kind);
                  // Icon: a WindowRef stores its owning app in reopen_info; a
                  // saved tab stores its browser as the LEADING segment (U+001F)
                  // of hint (`browser␟handle`, live tabs) or reopen_info
                  // (`browser␟url`, legacy BrowserTab whose hint is null), so
                  // appNameOf pulls the browser name that resolves the icon —
                  // passing the whole handle would fail to match; everything else
                  // uses reopen/descriptor.
                  const iconTarget =
                    r.kind === "WindowRef"
                      ? appNameOf(r.identity.reopen_info ?? r.identity.descriptor)
                      : isSavedTab(r.kind)
                      ? appNameOf(r.identity.hint ?? r.identity.reopen_info ?? "")
                      : r.identity.reopen_info ?? r.identity.descriptor;
                  return (
                    <li
                      key={r.id}
                      className={`resource ${canActivate ? "activatable" : ""}${
                        r.status === "Inactive" ? " res-inactive" : ""
                      }`}
                      onDoubleClick={() => canActivate && activateResource(r)}
                      title={
                        canActivate
                          ? isSavedTab(r.kind)
                            ? "Double-click: switch to this saved tab"
                            : "Double-click: bring to front · opens it if closed"
                          : undefined
                      }
                    >
                      <StatusDot status={r.status} />
                      {canActivate && (
                        <AppIcon
                          target={iconTarget}
                          className="app-icon app-icon--sm"
                        />
                      )}
                      <span className="badge">{kindLabel[r.kind] ?? r.kind}</span>
                      <span className="resource-name">{r.display_name}</span>
                      <button
                        className="resource-remove"
                        aria-label="Remove from group"
                        title="Remove this item from the group"
                        onClick={(e) => {
                          e.stopPropagation();
                          void handleRemoveResource(b.id, r.id);
                        }}
                      >
                        ×
                      </button>
                    </li>
                  );
                })}
              </ul>

              {(() => {
                const ref = groupSessionRef(b);
                if (!ref) {
                  // No session yet (never created, or just deleted) — offer to
                  // start one so a terminal can be (re)created from the card.
                  return (
                    <div className="group-live">
                      <div className="group-live-bar">
                        <span className="group-live-title">Terminal</span>
                        <button
                          className="mini"
                          onClick={() => openNewSession(b.id)}
                          title="이 그룹에 새 claude 세션(터미널)을 만듭니다"
                        >
                          New session
                        </button>
                      </div>
                    </div>
                  );
                }
                const open = openTerms.has(b.id);
                // Every Claude Code session in this group, for the read-only Log
                // viewer. The live terminal drives only the first (groupSessionRef),
                // but a group can hold several sessions — the Log button is shown
                // per session and labelled so it's clear WHICH transcript opens
                // (FR-12.3 / AC-17), distinct from the live terminal above.
                const sessions = b.resources.filter(
                  (r) => r.kind === "CodingSession"
                );
                const multiSession = sessions.length > 1;
                return (
                  <div className="group-live">
                    <div className="group-live-bar">
                      <span className="group-live-title">Terminal</span>
                      <button
                        className="mini"
                        onClick={() =>
                          open ? closeGroupTerminal(b.id) : openGroupTerminal(b.id)
                        }
                        title={
                          open
                            ? "Collapse the terminal view (claude keeps running)"
                            : "Open this group's claude terminal"
                        }
                      >
                        {open ? "Close" : "Open"}
                      </button>
                      {sessions.map((s) => (
                        <button
                          key={s.id}
                          className="mini"
                          onClick={() =>
                            setConvSession({
                              ref: s.identity.descriptor,
                              title: s.display_name,
                            })
                          }
                          title={`대화 기록 보기 (읽기 전용): ${s.display_name}`}
                        >
                          {multiSession ? `Log · ${s.display_name}` : "Log"}
                        </button>
                      ))}
                      {confirmKillId === b.id ? (
                        <>
                          <button
                            className="mini danger"
                            onClick={() => handleDeleteSession(b.id)}
                            title="세션을 종료하고 그룹에서 삭제합니다"
                          >
                            정말 삭제
                          </button>
                          <button
                            className="mini"
                            onClick={() => setConfirmKillId(null)}
                          >
                            취소
                          </button>
                        </>
                      ) : (
                        <button
                          className="mini danger"
                          onClick={() => setConfirmKillId(b.id)}
                          title="이 세션(터미널)을 종료하고 삭제 — 이후 다시 만들 수 있습니다"
                        >
                          Delete
                        </button>
                      )}
                      {promptTarget === b.id && (
                        <span className="group-live-target" title="프롬프트 바 기본 대상">
                          ◀ 프롬프트 대상
                        </span>
                      )}
                    </div>
                    {open && (
                      <GroupTerminal
                        sessionRef={ref}
                        cwd={groupSessionCwd(b)}
                      />
                    )}
                  </div>
                );
              })()}

              {reportBundleId === b.id && report && (
                <div className="restore-report">
                  <p>
                    <strong>{report.opened.length}</strong> opened
                    {report.failed.length > 0 && (
                      <span className="failed-count">
                        {" "}
                        · {report.failed.length} failed
                      </span>
                    )}
                  </p>
                  {report.failed.length > 0 && (
                    <ul className="failed-list">
                      {report.failed.map((f, i) => (
                        <li key={i}>{f}</li>
                      ))}
                    </ul>
                  )}
                  {report.skipped.length > 0 && (
                    <p className="skipped-note">
                      {report.skipped.length} coding session
                      {report.skipped.length > 1 ? "s" : ""} — open each with its
                      “Resume” button.
                    </p>
                  )}
                </div>
              )}
            </section>
            );
          })}
        </div>
      </main>
      </div>

      <footer className="console">
        <div className="console-head">
          <span className="console-title">Prompt</span>
          <span className="console-hint">
            {targetName ? (
              <>
                대상 <strong>@{targetName}</strong> · 다른 그룹은{" "}
                <code>@그룹명</code>
              </>
            ) : (
              <>
                <code>@그룹명</code> 으로 대상 그룹을 지정해 프롬프트를 보냅니다
              </>
            )}
          </span>
          <select
            className="console-model"
            value={claude?.model || DEFAULT_MODEL}
            onChange={(e) => handleSelectModel(e.target.value)}
            title="요약에 쓰는 모델"
          >
            {CLAUDE_MODELS.map((m) => (
              <option key={m.id} value={m.id}>
                {m.label}
              </option>
            ))}
          </select>
          <div className="console-head-actions">
            <button className="mini" onClick={openClaudeModal}>
              {claude?.configured ? "API Key" : "Connect"}
            </button>
          </div>
        </div>

        {promptError && <div className="console-error">{promptError}</div>}

        <form
          className="console-bar"
          onSubmit={(e) => {
            e.preventDefault();
            handleSendPrompt();
          }}
        >
          <div className="prompt-input-wrap">
            {showMentions && (
              <ul className="mention-menu">
                {mentionSuggestions.map((b, i) => (
                  <li key={b.id}>
                    <button
                      type="button"
                      className={`mention-item ${
                        i === activeMention ? "active" : ""
                      }`}
                      onMouseEnter={() => setMentionActive(i)}
                      onClick={() => applyMention(b)}
                    >
                      <span
                        className={`mention-dot ${
                          groupSessionRef(b) ? "live" : ""
                        }`}
                        title={groupSessionRef(b) ? "세션 있음" : "세션 없음"}
                      />
                      <span className="mention-name">@{b.name}</span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
            <textarea
              className="console-input"
              placeholder="@그룹명 메시지…  (Enter 전송 · Shift+Enter 줄바꿈)"
              value={promptInput}
              rows={1}
              onChange={(e) => setPromptInput(e.target.value)}
              onKeyDown={(e) => {
                // While the mention menu is open, arrows move the highlight and
                // Enter/Tab pick it (Esc dismisses) — so those keys don't send.
                if (showMentions) {
                  // The menu now flows left-to-right, so Right/Down advance and
                  // Left/Up go back (both directions kept for muscle memory).
                  if (e.key === "ArrowRight" || e.key === "ArrowDown") {
                    e.preventDefault();
                    setMentionActive(
                      (i) => (i + 1) % mentionSuggestions.length
                    );
                    return;
                  }
                  if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
                    e.preventDefault();
                    setMentionActive(
                      (i) =>
                        (i - 1 + mentionSuggestions.length) %
                        mentionSuggestions.length
                    );
                    return;
                  }
                  if (e.key === "Enter" || e.key === "Tab") {
                    e.preventDefault();
                    applyMention(mentionSuggestions[activeMention]);
                    return;
                  }
                  if (e.key === "Escape") {
                    e.preventDefault();
                    setMentionDismissed(true);
                    return;
                  }
                }
                if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  handleSendPrompt();
                }
              }}
            />
          </div>
          <button
            className="rec send"
            type="submit"
            disabled={promptSending || !promptInput.trim()}
          >
            {promptSending ? "…" : "Send"}
          </button>
        </form>
      </footer>

      {showGroupModal && (
        <div className="modal-backdrop" onClick={closeGroupModal}>
          <div
            className="modal"
            role="dialog"
            aria-modal="true"
            aria-label="New group"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="modal-title">New Group</h3>
            <input
              autoFocus
              className="group-input modal-input"
              placeholder="Group name"
              value={newGroupName}
              onChange={(e) => setNewGroupName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") handleCreateGroup();
                else if (e.key === "Escape") closeGroupModal();
              }}
            />
            <div className="modal-actions">
              <button className="ghost" onClick={closeGroupModal}>
                Cancel
              </button>
              <button onClick={handleCreateGroup}>Create</button>
            </div>
          </div>
        </div>
      )}

      {pendingPrompt &&
        (() => {
          const bundle = bundles.find((b) => b.id === pendingPrompt.bundleId);
          return (
            <div className="modal-backdrop" onClick={cancelPendingPrompt}>
              <div
                className="modal"
                role="dialog"
                aria-modal="true"
                aria-label="새 claude 세션"
                onClick={(e) => e.stopPropagation()}
              >
                <h3 className="modal-title">새 claude 세션</h3>
                <p className="modal-note">
                  ‘{bundle?.name}’ 그룹의 작업 폴더를 지정하면 그 폴더에서 새
                  세션(터미널)을 시작합니다
                  {pendingPrompt.text ? " · 아래 내용을 첫 메시지로 보냅니다." : "."}
                </p>
                <input
                  autoFocus
                  className="group-input modal-input"
                  placeholder="작업 폴더 경로 (예: C:\\Users\\me\\project)"
                  value={sessionCwd}
                  onChange={(e) => setSessionCwd(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") createSessionAndSend();
                    else if (e.key === "Escape") cancelPendingPrompt();
                  }}
                  disabled={creatingSession}
                />
                {pendingPrompt.text && (
                  <p
                    className="modal-note pending-text"
                    title={pendingPrompt.text}
                  >
                    보낼 내용: {pendingPrompt.text}
                  </p>
                )}
                <div className="modal-actions">
                  <button
                    className="ghost"
                    onClick={cancelPendingPrompt}
                    disabled={creatingSession}
                  >
                    취소
                  </button>
                  <button
                    onClick={createSessionAndSend}
                    disabled={creatingSession || !sessionCwd.trim()}
                  >
                    {creatingSession
                      ? "시작 중…"
                      : pendingPrompt.text
                        ? "시작하고 보내기"
                        : "세션 시작"}
                  </button>
                </div>
              </div>
            </div>
          );
        })()}

      {showClaudeModal && (
        <div className="modal-backdrop" onClick={closeClaudeModal}>
          <div
            className="modal"
            role="dialog"
            aria-modal="true"
            aria-label="Connect Claude"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="modal-title">Connect Claude</h3>
            <p className="modal-note">
              {claude?.source === "env"
                ? `Using AWS_BEARER_TOKEN_BEDROCK from your environment${
                    claude?.region ? ` (${claude.region})` : ""
                  }. Enter a key below to save one in the app instead.`
                : claude?.configured
                  ? "A Bedrock key is saved on this machine. Enter a new key to replace it — or leave blank and save to remove it."
                  : "Paste your AWS Bedrock API key (ABSK…). It's stored locally on this machine only — never in the repo, sent only to the AWS Bedrock runtime as a Bearer token."}
            </p>
            <input
              autoFocus
              type="password"
              className="group-input modal-input"
              placeholder="ABSK…"
              value={keyInput}
              onChange={(e) => setKeyInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") handleSaveKey();
                else if (e.key === "Escape") closeClaudeModal();
              }}
            />
            <label className="modal-field">
              <span className="modal-field-label">Model</span>
              <select
                className="group-input modal-input"
                value={claude?.model || DEFAULT_MODEL}
                onChange={(e) => handleSelectModel(e.target.value)}
              >
                {CLAUDE_MODELS.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.label} — {m.id}
                  </option>
                ))}
              </select>
            </label>
            <div className="modal-actions">
              <button className="ghost" onClick={closeClaudeModal}>
                Cancel
              </button>
              <button onClick={handleSaveKey}>Save</button>
            </div>
          </div>
        </div>
      )}

      {convSession && (
        <ConversationModal
          sessionRef={convSession.ref}
          title={convSession.title}
          onClose={() => setConvSession(null)}
        />
      )}
    </div>
  );
}
