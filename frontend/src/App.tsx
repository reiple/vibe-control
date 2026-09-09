import { Fragment, useEffect, useMemo, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type {
  WorkBundle,
  Resource,
  SessionSnapshot,
  SessionCompletion,
  RestoreReport,
  RunningApp,
  RunningWindow,
  ChatMsg,
  ClaudeStatus,
  ClaudeUsage,
} from "./types";
import {
  getBundles,
  getSessionSnapshot,
  restoreBundle,
  activateCodingSession,
  listRunningApps,
  listAppChildren,
  activateApp,
  activateWindow,
  activateChild,
  listBrowserTabs,
  activateTab,
  activateTabResource,
  addTabResource,
  addWindowResource,
  activateWindowResource,
  getAppIcon,
  createBundle,
  deleteBundle,
  removeResource,
  addAppResource,
  addChildResource,
  getLayout,
  savePanelLayout,
  claudeStatus,
  claudeUsage,
  setClaudeApiKey,
  setClaudeModel,
  sendClaudeMessage,
  requestAccessibility,
} from "./api";

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
  kind === "AppLaunch" ||
  kind === "WindowRef" ||
  kind === "BrowserTab" ||
  kind === "Folder";

// A saved live browser tab (FR-9.6 / AC-20): focus-only, no URL — activated by
// its stored handle (re-matched by title), never re-opened as a URL (FR-9.7).
const isSavedTab = (kind: string) => kind === "BrowserTabLive";

// Apps whose expanded list shows browser TABS (via UI Automation) rather than
// OS windows — matched case-insensitively against the app-group name, which on
// Windows is the browser's exe stem (FR-9.6 / FR-10.12). A browser group is
// always expandable so its tabs can be revealed even when it has one window.
// Tabs are fetched lazily on expand, never on the 1-second poll (FR-10.7).
const BROWSER_APPS = new Set(["chrome", "msedge", "brave", "whale"]);

// The left-panel expansion has two OS-specific models that must not regress each
// other: Windows uses HEAD's UI-Automation tab/window enumeration (per-tab via
// listBrowserTabs, per-window from the polled app.windows), while macOS uses the
// generic children model (listAppChildren → Safari/Chrome tabs, Finder folders,
// per-instance windows). One flag switches the expand/fetch/render/drag paths.
const IS_MACOS =
  /Mac|iP(hone|ad|od)/.test(navigator.platform) ||
  /Macintosh|Mac OS X/.test(navigator.userAgent);

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

// Apps are keyed by (name + bundle id): two distinct apps can share a display
// name with different bundle ids, so keying on name alone would collide React
// keys and merge their expand/children/loading state. NUL (U+0000) can't occur
// in a name or bundle id, so it's an unambiguous separator.
const APP_KEY_SEP = String.fromCharCode(0);
const appKey = (a: { name: string; bundle_id?: string | null }) =>
  `${a.name}${APP_KEY_SEP}${a.bundle_id ?? ""}`;

// A WindowRef's descriptor/reopen_info is an opaque window handle
// (`app<U+001F>title<U+001F>idx`); its app-name part (before the first U+001F
// unit separator) resolves the app icon. A no-op for handle-less targets.
const UNIT_SEP = String.fromCharCode(31);
const appNameOf = (handle: string) => handle.split(UNIT_SEP)[0];

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

// Faint tool glyphs tucked into a few of the larger scattered tiles, so the
// field reads as real applications being gathered — terminal, browser, folder,
// code, settings — rather than abstract squares. Kept low-contrast and softly
// blurred (see .intro-icon-glyph) so they stay atmospheric, not literal.
const TOOL_GLYPHS: React.ReactElement[] = [
  // terminal prompt
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M5 7l4 4-4 4" /><path d="M12 16h7" /></svg>,
  // browser / globe
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="8" /><path d="M4 12h16" /><path d="M12 4c2.6 2.6 2.6 13.4 0 16" /><path d="M12 4c-2.6 2.6-2.6 13.4 0 16" /></svg>,
  // folder
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><path d="M4 7h5l2 2h9v9H4z" /></svg>,
  // code brackets
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M9 8l-4 4 4 4" /><path d="M15 8l4 4-4 4" /></svg>,
  // settings gear
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="3" /><path d="M12 3v3M12 18v3M3 12h3M18 12h3M5.6 5.6l2.1 2.1M16.3 16.3l2.1 2.1M18.4 5.6l-2.1 2.1M7.7 16.3l-2.1 2.1" /></svg>,
];

// Full-viewport boot splash / intro animation. Rendered by React (not static
// index.html markup) so it paints at the correct DPI on Windows/WebView2, and
// it blocks all input underneath until the app has finished its first load.
// `hiding` fades it out just before it unmounts.
//
// The choreography tells the product's core story (intro-animation.md):
// many independent apps, scattered and drifting, are pulled by a central
// signal toward one point, organise into a network, and collapse into a single
// glowing control core — then the title resolves. Chaos → signal → attraction
// → organisation → unification → VIBE CONTROL. Dark, holographic, and calm,
// with the TE hero-orange carried through as the "control core" so the intro
// stays of a piece with the product. The whole sequence is driven by CSS
// keyframes; per-icon scatter positions, curved mid-points, and staggered
// (negative-delay) timing are generated once on mount so every launch differs.
function Splash({ hiding }: { hiding: boolean }) {
  const icons = useMemo(() => {
    const N = 15;
    // A few tiles glow hero-orange among the neutral glass ones, so the field
    // reads as varied applications rather than a uniform grid.
    const accent = new Set([2, 7, 11]);
    return Array.from({ length: N }, (_, i) => {
      // Every tile carries a faint tool glyph, cycling through the set so the
      // whole field reads as real applications being gathered.
      const glyph = i % TOOL_GLYPHS.length;
      // Sizes still vary widely so the field stays organic, not a uniform grid.
      const size = 30 + Math.random() * 26;
      // Scatter across the viewport as offsets from centre (chaos).
      const x = (Math.random() * 2 - 1) * 42; // vw
      const y = (Math.random() * 2 - 1) * 40; // vh
      // A perpendicular kick on the mid-point bends each path into an arc, so
      // icons sweep in on curved, coordinated trajectories rather than straight
      // radial lines.
      const swirl = Math.random() * 2 - 1;
      const mx = x * 0.5 - y * 0.2 * swirl;
      const my = y * 0.5 + x * 0.2 * swirl;
      return {
        id: i,
        isAccent: accent.has(i),
        glyph,
        size: `${size.toFixed(0)}px`,
        x: `${x.toFixed(1)}vw`,
        y: `${y.toFixed(1)}vh`,
        mx: `${mx.toFixed(1)}vw`,
        my: `${my.toFixed(1)}vh`,
        // Negative delay staggers when each icon reaches convergence while
        // keeping them all visibly scattered from the first frame.
        conv: -(Math.random() * 0.7),
        drift: -(Math.random() * 3),
      };
    });
  }, []);

  return (
    <div
      className={`splash ${hiding ? "splash--hidden" : ""}`}
      role="progressbar"
      aria-label="Loading vibe-control"
      aria-busy="true"
    >
      <div className="intro">
        <div className="intro-field" aria-hidden>
          {icons.map((ic) => (
            <span
              key={ic.id}
              className={`intro-icon${ic.isAccent ? " is-accent" : ""}`}
              style={
                {
                  "--x": ic.x,
                  "--y": ic.y,
                  "--mx": ic.mx,
                  "--my": ic.my,
                  "--size": ic.size,
                  animationDelay: `${ic.conv.toFixed(2)}s`,
                } as React.CSSProperties
              }
            >
              <span
                className="intro-icon-face"
                style={{ animationDelay: `${ic.drift.toFixed(2)}s` }}
              >
                {ic.glyph !== null && (
                  <span className="intro-icon-glyph">{TOOL_GLYPHS[ic.glyph]}</span>
                )}
              </span>
            </span>
          ))}
        </div>
        <span className="intro-pulse" aria-hidden />
        <span className="intro-pulse intro-pulse--2" aria-hidden />
        <span className="intro-ring" aria-hidden />
        <span className="intro-core" aria-hidden />
        <div className="intro-title-wrap">
          <div className="intro-title">VIBE CONTROL</div>
          <div className="intro-sub">Unified Control System</div>
        </div>
      </div>
    </div>
  );
}

const completionLabel: Record<SessionCompletion, string> = {
  Waiting: "Awaiting reply",
  NotWaiting: "Working",
  Unknown: "Unknown",
};

// Inline live status for a coding session, shown right inside the context-group
// card (no separate conversation panel). Shows a completion chip plus the last
// question Claude is waiting on. Re-fetches whenever `nonce` changes (the ↻
// button bumps it) so the status stays current without polling. This never
// surfaces full conversation content — only the single last-question line the
// snapshot already exposes — keeping with the no-content-logging constraint.
function SessionStatus({
  sessionRef,
  nonce,
}: {
  sessionRef: string;
  nonce: number;
}) {
  const [snap, setSnap] = useState<SessionSnapshot | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    getSessionSnapshot("claude-code", sessionRef)
      .then((s) => {
        if (!cancelled) setSnap(s);
      })
      .catch(() => {
        // Transient read failure during a 1s poll — keep the last good
        // snapshot instead of blanking the status every tick.
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true; // ignore late resolves after unmount/ref change
    };
  }, [sessionRef, nonce]);

  if (!snap || !snap.available) {
    return loading ? (
      <div className="session-status loading">Checking…</div>
    ) : null;
  }

  return (
    <div className="session-status">
      <span className={`completion ${snap.completion.toLowerCase()}`}>
        {completionLabel[snap.completion]}
      </span>
      {snap.last_question && (
        <p className="last-q" title={snap.last_question}>
          {snap.last_question}
        </p>
      )}
    </div>
  );
}

// Full-conversation viewer (FR-12.3 / AC-17). Restores in-app scrollback of a
// coding session's transcript — the backend already returns up to 60 recent
// turns in `SessionSnapshot.conversation`; this renders them in a modal. Read
// -only, no logging: it only displays what the snapshot already exposes.
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
  // App groups the user has expanded to reveal their windows (FR-2.8). Keyed by
  // appKey (name + bundle id) so the expansion survives the 1s poll replacing the
  // list and two same-named apps don't share expansion state.
  const [expandedApps, setExpandedApps] = useState<Set<string>>(new Set());
  // Lazily-fetched children (browser tabs / Finder folders / windows) per app,
  // keyed by appKey (FR-2.2). `undefined` = not yet fetched; `[]` = fetched,
  // none. Mirrored into `expandedRef` so the 1s poll can refresh them without a
  // stale closure.
  const [appChildren, setAppChildren] = useState<Record<string, RunningWindow[]>>(
    {}
  );
  const [loadingApp, setLoadingApp] = useState<Set<string>>(new Set());
  // True when macOS Accessibility permission is missing: the System Events pass
  // behind per-instance window listing (VS Code / Terminal / other non-browser
  // apps) returns nothing without it, so an empty window list means "grant
  // permission", not "no windows". Browser tabs / Finder folders use Apple
  // Events and are unaffected.
  const [accessNeeded, setAccessNeeded] = useState(false);
  const expandedRef = useRef<Set<string>>(new Set());
  // Monotonic per-app fetch sequence: the 1s poll can start a second
  // listAppChildren for an app before the first resolves, and the two osascript
  // calls can finish out of order. Each fetch captures its sequence number and
  // only writes if it's still the latest, so a slow earlier response can't clobber
  // fresher children. Keyed by appKey.
  const childFetchSeq = useRef<Record<string, number>>({});
  // A child being dragged into a group (a tab/folder/window), distinct from a
  // whole-app drag. Kept in a ref so the poll can yield to an in-flight drag.
  const draggedChild = useRef<{
    kind: string;
    name: string;
    target: string;
    // Opaque activation token (`app\u{1f}url` for a tab, `Finder\u{1f}name` for a
    // Finder window). Preserved so the registered resource can select the exact
    // tab/window, not just re-open the bare target.
    handle: string;
  } | null>(null);
  // Windows-only (UIA) expansion state, keyed by app.name. Browser tabs are
  // fetched lazily on expand (never on the poll, FR-10.7); a present-but-empty
  // entry means "read, none" → fall back to the polled OS windows.
  const [tabsByApp, setTabsByApp] = useState<Record<string, RunningWindow[]>>(
    {}
  );
  const [tabsLoadingApps, setTabsLoadingApps] = useState<Set<string>>(new Set());
  // The live browser tab being dragged (Windows): browser group name + opaque
  // handle + title, so a drop registers it as a focus-only resource (FR-9.6).
  const draggedTab = useRef<{
    browser: string;
    handle: string;
    title: string;
  } | null>(null);
  // The individual OS window being dragged (Windows): owning app-group name +
  // opaque handle + title, so a drop registers it as a WindowRef (FR-2.8).
  const draggedWin = useRef<{
    app: string;
    handle: string;
    title: string;
  } | null>(null);

  // Boot splash: `booting` keeps the overlay mounted (blocking input);
  // `splashOut` triggers its fade just before we unmount it.
  const [booting, setBooting] = useState(true);
  const [splashOut, setSplashOut] = useState(false);

  const [newGroupName, setNewGroupName] = useState("");
  const [showGroupModal, setShowGroupModal] = useState(false);
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);
  const [dragOverId, setDragOverId] = useState<string | null>(null);
  const draggedApp = useRef<RunningApp | null>(null);

  // Persisted sidebar width (E2). Applied imperatively to the aside via a ref so
  // React never re-renders it and fights the CSS `resize` drag; the ResizeObserver
  // persists changes (debounced). cardHeight is carried through unchanged for now.
  const sidebarRef = useRef<HTMLElement | null>(null);
  const [savedPanelWidth, setSavedPanelWidth] = useState<number | null>(null);
  const cardHeightRef = useRef<number>(150);

  // Full-conversation viewer (FR-12.3 / AC-17): the session whose transcript is open.
  const [convSession, setConvSession] = useState<{
    ref: string;
    title: string;
  } | null>(null);

  // Polling economy (FR-7.5 / FR-7.6 / NFR-Pf3). `pollInFlight` keeps a slow
  // refresh from overlapping the next tick; `resizingUntil` suppresses polls
  // for a short window after each window resize, so dragging the app's edges
  // isn't competing with a 1s OS enumeration.
  const pollInFlight = useRef(false);
  const resizingUntil = useRef(0);

  // Bumped by ↻ to force every visible SessionStatus to re-fetch its snapshot.
  const [sessionNonce, setSessionNonce] = useState(0);
  const [report, setReport] = useState<RestoreReport | null>(null);
  const [reportBundleId, setReportBundleId] = useState<string | null>(null);
  const [restoringId, setRestoringId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // ── Claude prompt console ──────────────────────────────────────────
  const [claude, setClaude] = useState<ClaudeStatus | null>(null);
  // Live Bedrock token usage for the KO-II meter (updated on load + each send).
  const [usage, setUsage] = useState<ClaudeUsage | null>(null);
  const [chat, setChat] = useState<ChatMsg[]>([]);
  const [chatInput, setChatInput] = useState("");
  const [chatSending, setChatSending] = useState(false);
  const [chatError, setChatError] = useState<string | null>(null);
  const [showClaudeModal, setShowClaudeModal] = useState(false);
  const [keyInput, setKeyInput] = useState("");
  const transcriptRef = useRef<HTMLDivElement | null>(null);

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
    // Floor the visible time to the length of the intro choreography so the
    // full sequence (convergence → core → title hold) plays before the fade,
    // rather than being cut short the instant the (usually faster) boot data
    // lands. The hard cap still guarantees the splash lifts if boot stalls.
    const MIN_SPLASH_MS = 6000;
    const MAX_SPLASH_MS = 9000;
    const FADE_MS = 500; // must match the .splash opacity transition

    // Fade the splash out, then unmount it. Idempotent: whichever of the boot
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
        claudeUsage()
          .then((u) => {
            if (!cancelled) setUsage(u);
          })
          .catch(() => {
            if (!cancelled) setUsage(null);
          }),
        // Trigger the one-time Accessibility prompt at launch and record whether
        // it's granted, so per-instance window lists work without the user
        // hunting through System Settings.
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

  // Keep the account-wide "today" usage fresh: CloudWatch totals move as the
  // whole account keeps working, independent of this app's own calls. A light
  // GetMetricData poll every few minutes (boot did the first fetch).
  useEffect(() => {
    const id = window.setInterval(() => {
      claudeUsage()
        .then(setUsage)
        .catch(() => {});
    }, 180_000);
    return () => window.clearInterval(id);
  }, []);

  const refreshRunning = async (silent = false) => {
    // Never re-render the running list mid-drag: replacing/reordering the
    // items cancels the in-flight native HTML5 drag before it can drop. The
    // background poll yields to an active drag; a manual ↻ can't collide with
    // a drag (one pointer) so it isn't gated.
    if (
      silent &&
      (draggedApp.current ||
        draggedChild.current ||
        draggedTab.current ||
        draggedWin.current)
    )
      return;
    // FR-7.6 concurrency guard: if the previous poll is still fetching, skip
    // this tick rather than stacking another OS enumeration on top of it. Only
    // background polls are gated — an explicit ↻ is a deliberate user action.
    if (silent && pollInFlight.current) return;
    if (silent) pollInFlight.current = true;
    setSessionNonce((n) => n + 1); // also refresh inline coding-session statuses
    try {
      const apps = await listRunningApps();
      if (
        silent &&
        (draggedApp.current ||
          draggedChild.current ||
          draggedTab.current ||
          draggedWin.current)
      )
        return; // a drag began while fetching
      setRunningApps(apps);
      // macOS only: keep expanded apps' children live (a browser's tabs change
      // as the user navigates). Windows never re-reads tabs on the poll
      // (FR-10.7); its window rows come from the polled app.windows directly.
      if (IS_MACOS) {
        for (const key of expandedRef.current) {
          const a = apps.find((x) => appKey(x) === key);
          if (a) void fetchChildren(a, true);
        }
      }
    } catch (e) {
      // A background poll shouldn't flash the error banner every tick — only
      // surface failures from an explicit refresh.
      if (!silent) setError(String(e));
    } finally {
      if (silent) pollInFlight.current = false;
    }
  };

  // Live status: poll running apps + coding-session snapshots every second so
  // the left panel and each group's session status stay current without the
  // user pressing ↻. Silent (no spinner / no error banner) to avoid flicker.
  //
  // Economy (FR-7.5 / NFR-Pf3): the poll drives a real OS window enumeration,
  // so it is suppressed whenever the result can't be seen or would compete with
  // the user — while the window is hidden/minimised, and briefly after each
  // resize. Coming back into view refreshes immediately rather than waiting out
  // the remaining tick, so the panel is never shown stale.
  useEffect(() => {
    const RESIZE_QUIET_MS = 400;

    const onResize = () => {
      resizingUntil.current = performance.now() + RESIZE_QUIET_MS;
    };
    const onVisibility = () => {
      if (!document.hidden) refreshRunning(true); // catch up on return
    };

    window.addEventListener("resize", onResize);
    document.addEventListener("visibilitychange", onVisibility);

    const id = setInterval(() => {
      if (document.hidden) return; // nothing on screen to update
      if (performance.now() < resizingUntil.current) return; // mid-resize
      refreshRunning(true);
    }, 1000);

    return () => {
      clearInterval(id);
      window.removeEventListener("resize", onResize);
      document.removeEventListener("visibilitychange", onVisibility);
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

  // Keep the console transcript pinned to the newest message.
  useEffect(() => {
    const el = transcriptRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [chat, chatSending]);

  const filteredApps = runningApps.filter((a) =>
    a.name.toLowerCase().includes(filter.trim().toLowerCase())
  );

  const activate = (target: string) => {
    setError(null);
    activateApp(target).catch((e) => setError(String(e)));
  };

  const toggleExpanded = (key: string) =>
    setExpandedApps((cur) => {
      const next = new Set(cur);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      expandedRef.current = next; // mirror for the poll's live refresh
      return next;
    });

  // Fetch one app's children (tabs/folders/windows). `background` = a poll-tick
  // refresh of an already-open group: no spinner, and it bails if a drag starts.
  const fetchChildren = async (app: RunningApp, background = false) => {
    const key = appKey(app);
    // Claim this fetch's sequence number; a later fetch bumps it, marking any
    // still-in-flight earlier fetch as stale so it won't overwrite fresh data.
    const seq = (childFetchSeq.current[key] ?? 0) + 1;
    childFetchSeq.current[key] = seq;
    if (!background) setLoadingApp((s) => new Set(s).add(key));
    try {
      const kids = await listAppChildren(app.name, app.bundle_id);
      if (draggedChild.current || draggedApp.current) return; // don't yank a drag
      if (childFetchSeq.current[key] !== seq) return; // superseded by a newer fetch
      setAppChildren((c) => ({ ...c, [key]: kids }));
    } catch {
      if (!background && childFetchSeq.current[key] === seq)
        setAppChildren((c) => ({ ...c, [key]: [] }));
    } finally {
      if (!background)
        setLoadingApp((s) => {
          const n = new Set(s);
          n.delete(key);
          return n;
        });
    }
  };

  // Expand/collapse an app; on first expand, lazily fetch its children (FR-2.2).
  const handleToggleApp = (app: RunningApp) => {
    const key = appKey(app);
    const willExpand = !expandedApps.has(key);
    toggleExpanded(key);
    if (willExpand && appChildren[key] === undefined) void fetchChildren(app);
  };

  // Activate a specific child: focus a browser tab, open a folder, or raise a
  // window (FR-2.8 / FR-4.1). Surface failures (e.g. the tab/window was closed).
  const activateChildItem = (w: RunningWindow) => {
    setError(null);
    activateChild(w.kind, w.target, w.handle).catch((e) => setError(errText(e)));
  };

  // Double-click a group resource: a WindowRef's target is an opaque window
  // handle, so raise that exact window (activateWindow splits the handle);
  // everything else is an app-level launch/focus. Passing a window handle to
  // activateApp would fail — open_app can't parse `app\u{1f}title\u{1f}idx`.
  const activateResource = (r: Resource) => {
    setError(null);
    const descriptor = r.identity.descriptor;
    const reopen = r.identity.reopen_info ?? descriptor;
    let p: Promise<void>;
    switch (r.kind) {
      // A Windows live tab (no URL, FR-9.7): re-focus the exact tab by its stored
      // handle, re-matched by title if stale — never open a URL.
      case "BrowserTabLive":
        p = activateTabResource(r.identity.hint ?? "", descriptor);
        break;
      // reopen is `app\u{1f}url`; select that exact tab in that browser (falls back
      // in the backend to opening the url if it's a legacy url-only resource).
      case "BrowserTab":
        p = activateChild("tab", descriptor, reopen);
        break;
      // descriptor is a POSIX path, or a `Finder\u{1f}name` handle for a path-less
      // window (Recents / saved search) → raise it by name (empty path target).
      case "Folder":
        p = descriptor.includes(UNIT_SEP)
          ? activateChild("folder", "", descriptor)
          : activateChild("folder", descriptor, "");
        break;
      // A window ref: on Windows re-focus the exact HWND (verified by title,
      // scoped to the owning app); on macOS the reopen_info is the opaque window
      // handle raised directly.
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

  // ── Windows (UIA) expansion: per-tab + per-window ───────────────────────
  // Activate exactly one window/tab by its opaque handle (FR-2.8/AC-20). Surface
  // failures (e.g. the window was closed between polls) in the error banner.
  const activateWin = (handle: string) => {
    setError(null);
    activateWindow(handle).catch((e) => setError(errText(e)));
  };

  // Lazily read a browser's open tabs when its group is expanded (FR-9.6 /
  // FR-10.12) — never on the poll (FR-10.7). An empty result (background/
  // unreadable window) leaves the group falling back to its OS windows.
  // `reveal=false` (expand) never foregrounds the browser; `reveal=true` (the
  // explicit "앞으로 가져와 다시 읽기" button) foregrounds each window first so a
  // background window's lazily-built tab tree becomes readable.
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
  // re-read the strip so the active-tab dot reflects the switch. On-demand, so
  // the poll (FR-10.7) is untouched.
  const activateTabRow = (handle: string, appName: string) => {
    setError(null);
    activateTab(handle)
      .then(() => fetchTabs(appName))
      .catch((e) => setError(errText(e)));
  };

  // Note: saved live tabs (BrowserTabLive) and individual windows (WindowRef)
  // are re-focused via the OS-aware `activateResource` dispatch above
  // (FR-4.1/4.2, AC-20) — no URL is ever opened for a live tab (FR-9.7).

  // Expand/collapse a Windows browser group and lazily read its tabs on open
  // (never on the poll, FR-10.7). Window rows come from the polled app.windows.
  const handleToggleTabApp = (app: RunningApp, isBrowser: boolean) => {
    const willExpand = !expandedApps.has(appKey(app));
    toggleExpanded(appKey(app));
    if (willExpand && isBrowser) void fetchTabs(app.name);
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

  const handleRemoveResource = async (bundleId: string, resourceId: string) => {
    setError(null);
    try {
      setBundles(await removeResource(bundleId, resourceId));
    } catch (e) {
      setError(errText(e));
    }
  };

  const onDragStartApp = (e: React.DragEvent, app: RunningApp) => {
    draggedApp.current = app;
    draggedChild.current = null; // symmetric to onDragStartChild: never drop with a stale child ref
    draggedTab.current = null;
    draggedWin.current = null;
    e.dataTransfer.effectAllowed = "copy";
    e.dataTransfer.setData("text/plain", app.name);
  };

  // Windows: dragging an individual browser tab row registers just that tab (not
  // the whole browser) on drop as a focus-only live tab (FR-9.6 / AC-20).
  const onDragStartTab = (e: React.DragEvent, browser: string, w: RunningWindow) => {
    draggedTab.current = { browser, handle: w.handle, title: w.title };
    draggedApp.current = null;
    draggedChild.current = null;
    draggedWin.current = null;
    e.dataTransfer.effectAllowed = "copy";
    e.dataTransfer.setData("text/plain", w.title);
  };

  // Windows: dragging an individual OS window row registers just that window (not
  // the whole app) on drop as a WindowRef (FR-2.8 / FR-3.5 / AC-20).
  const onDragStartWin = (e: React.DragEvent, app: string, w: RunningWindow) => {
    draggedWin.current = { app, handle: w.handle, title: w.title };
    draggedApp.current = null;
    draggedChild.current = null;
    draggedTab.current = null;
    e.dataTransfer.effectAllowed = "copy";
    e.dataTransfer.setData("text/plain", w.title);
  };

  const onDropToBundle = async (e: React.DragEvent, bundle: WorkBundle) => {
    e.preventDefault();
    setDragOverId(null);
    const child = draggedChild.current;
    const tab = draggedTab.current;
    const win = draggedWin.current;
    const app = draggedApp.current;
    draggedChild.current = null;
    draggedTab.current = null;
    draggedWin.current = null;
    draggedApp.current = null;
    setError(null);
    try {
      // A macOS child (tab/folder/window) registers as its own resource kind; a
      // Windows tab → focus-only live tab; a Windows window → WindowRef; a whole
      // app → app launch (FR-2.2 / FR-9.6 / §13.3).
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

  // "View": bring the session's already-open Claude Code terminal to the front
  // so the user reads/continues the real conversation there. Does NOT spawn a
  // new terminal (that's "Resume" → resumeCodingSession).
  const handleActivateSession = (resource: Resource) => {
    setError(null);
    activateCodingSession(resource.identity.descriptor).catch((e) =>
      setError(String(e))
    );
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
      setChatError(null);
    } catch (e) {
      setChatError(errText(e));
    }
  };

  const handleSelectModel = async (model: string) => {
    try {
      setClaude(await setClaudeModel(model));
    } catch (e) {
      setChatError(errText(e));
    }
  };

  const handleSendChat = async () => {
    const text = chatInput.trim();
    if (!text || chatSending) return;
    if (!claude?.configured) {
      openClaudeModal(); // no key yet → prompt for one instead of failing
      return;
    }
    const next: ChatMsg[] = [...chat, { role: "user", content: text }];
    setChat(next);
    setChatInput("");
    setChatError(null);
    setChatSending(true);
    try {
      const reply = await sendClaudeMessage(next);
      setChat((cur) => [...cur, { role: "assistant", content: reply }]);
      // Refresh the usage meter with this call's tokens.
      claudeUsage()
        .then(setUsage)
        .catch(() => {});
    } catch (e) {
      setChatError(errText(e)); // keep the user's message so they can retry
    } finally {
      setChatSending(false);
    }
  };

  // Derived usage figures for the KO-II meter.
  const inTok = usage?.input_tokens ?? 0;
  const outTok = usage?.output_tokens ?? 0;

  return (
    <div className={`app-shell${IS_MACOS ? "" : " wc-chrome"}`}>
    {!IS_MACOS && <WindowControls />}
    {booting && <Splash hiding={splashOut} />}
    <div className="app">
      <aside className="sidebar" ref={sidebarRef}>
        {/* The macOS traffic lights float over this header (transparent Overlay
            title bar); it doubles as the window drag region. */}
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
                    // Clear the drag ref even when the drag is cancelled (dropped
                    // outside a group), so the paused poll resumes.
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
                    title={
                      isExpanded
                        ? "Hide windows / tabs"
                        : "Show windows / tabs"
                    }
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

        {/* KO II-style display: a wide black LCD recessed into the chassis,
            showing TODAY's Claude token usage as a graphical meter (à la
            Teenage Engineering K.O. II) — glossy glare sweep baked on top. */}
        <div className="ko-screen">
          <div className="ko-screen-glass">
            {/* Left: today's token counter (status LED + model pill inline) + legend. */}
            <div className="ko-usage">
              <div className="ko-usage-total">
                <span
                  className={`ko-led ${claude?.configured ? "on" : "off"}`}
                  aria-hidden
                />
                <b>{fmtFull(usage?.total_tokens ?? 0)}</b>
                <span className="ko-usage-model">
                  {modelLabel(usage?.model ?? claude?.model ?? DEFAULT_MODEL)}
                </span>
              </div>
              <div className="ko-legend">
                <span className="ko-chip in">
                  <em />IN {fmtTokens(inTok)}
                </span>
                <span className="ko-chip out">
                  <em />OUT {fmtTokens(outTok)}
                </span>
                <span className="ko-chip req">
                  <em />
                  {usage?.requests ?? 0} CALLS
                </span>
              </div>
            </div>

            {/* Right: last-call readout + small group/app counts. */}
            <div className="ko-side">
              <div className="ko-last" title="Tokens in the most recent call">
                <b>{fmtTokens(usage?.last_total ?? 0)}</b>
                <i>last call</i>
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

        {error && <div className="error">{error}</div>}

        <div className="group-grid">
          {bundles.length === 0 && (
            <div className="placeholder">
              No groups yet. Create one with “Add Group”, then drag running apps
              from the left into it.
            </div>
          )}
          {bundles.map((b) => (
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
                {b.resources.length === 0 && (
                  <li className="empty drop-hint">Drag apps here</li>
                )}
                {b.resources.map((r) => (
                  <li
                    key={r.id}
                    className={`resource ${
                      isActivatable(r.kind) || isSavedTab(r.kind)
                        ? "activatable"
                        : ""
                    }`}
                    onDoubleClick={() =>
                      (isActivatable(r.kind) || isSavedTab(r.kind)) &&
                      activateResource(r)
                    }
                    title={
                      isSavedTab(r.kind)
                        ? "Double-click: switch to this tab (never opens a URL)"
                        : isActivatable(r.kind)
                        ? "Double-click: bring to front · opens it if closed"
                        : undefined
                    }
                  >
                    {isActivatable(r.kind) && (
                      <AppIcon
                        target={
                          r.kind === "WindowRef"
                            ? appNameOf(
                                r.identity.reopen_info ?? r.identity.descriptor
                              )
                            : r.identity.reopen_info ?? r.identity.descriptor
                        }
                        className="app-icon app-icon--sm"
                      />
                    )}
                    <span className="badge">{kindLabel[r.kind] ?? r.kind}</span>
                    <span className="resource-name">{r.display_name}</span>
                    {r.kind === "CodingSession" && (
                      <>
                        <button
                          className="mini"
                          onClick={() =>
                            setConvSession({
                              ref: r.identity.descriptor,
                              title: r.display_name,
                            })
                          }
                          title="Read the full conversation in-app"
                        >
                          Log
                        </button>
                        <button
                          className="mini"
                          onClick={() => handleActivateSession(r)}
                          title="Bring the open Claude Code terminal to front"
                        >
                          View
                        </button>
                        <SessionStatus
                          sessionRef={r.identity.descriptor}
                          nonce={sessionNonce}
                        />
                      </>
                    )}
                    <button
                      className="resource-remove"
                      onClick={(e) => {
                        e.stopPropagation();
                        void handleRemoveResource(b.id, r.id);
                      }}
                      onDoubleClick={(e) => e.stopPropagation()}
                      title="Remove from this group"
                      aria-label="Remove from group"
                    >
                      ×
                    </button>
                  </li>
                ))}
              </ul>

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
                      {report.skipped.length > 1 ? "s" : ""} skipped — focus an
                      open one with its “View” button.
                    </p>
                  )}
                </div>
              )}
            </section>
          ))}
        </div>
      </main>
      </div>

      <footer className="console">
        <div className="console-head">
          <span className="console-title">Claude</span>
          <span
            className={`console-conn ${claude?.configured ? "on" : "off"}`}
            title={
              claude?.source === "env"
                ? `Using AWS_BEARER_TOKEN_BEDROCK from the environment${
                    claude?.region ? ` · ${claude.region}` : ""
                  }`
                : claude?.configured
                  ? `Using the Bedrock key saved in this app${
                      claude?.region ? ` · ${claude.region}` : ""
                    }`
                  : "No Bedrock key yet"
            }
          >
            {claude?.configured
              ? claude.source === "env"
                ? "connected · env key"
                : "connected · saved key"
              : "not connected"}
          </span>
          <select
            className="console-model"
            value={claude?.model || DEFAULT_MODEL}
            onChange={(e) => handleSelectModel(e.target.value)}
            title="Model"
          >
            {CLAUDE_MODELS.map((m) => (
              <option key={m.id} value={m.id}>
                {m.label}
              </option>
            ))}
          </select>
          <div className="console-head-actions">
            {chat.length > 0 && (
              <button
                className="mini"
                onClick={() => {
                  setChat([]);
                  setChatError(null);
                }}
                title="Clear the conversation"
              >
                Clear
              </button>
            )}
            <button className="mini" onClick={openClaudeModal}>
              {claude?.configured ? "API Key" : "Connect"}
            </button>
          </div>
        </div>

        {(chat.length > 0 || chatSending || chatError) && (
          <div className="console-transcript" ref={transcriptRef}>
            {chat.map((m, i) => (
              <div key={i} className={`bubble ${m.role}`}>
                <span className="bubble-role">
                  {m.role === "user" ? "You" : "Claude"}
                </span>
                <p className="bubble-text">{m.content}</p>
              </div>
            ))}
            {chatSending && (
              <div className="bubble assistant pending">
                <span className="bubble-role">Claude</span>
                <p className="bubble-text typing">thinking…</p>
              </div>
            )}
            {chatError && <div className="console-error">{chatError}</div>}
          </div>
        )}

        <form
          className="console-bar"
          onSubmit={(e) => {
            e.preventDefault();
            handleSendChat();
          }}
        >
          <textarea
            className="console-input"
            placeholder={
              claude?.configured
                ? "Message Claude…  (Enter to send · Shift+Enter for newline)"
                : "Connect your Claude API key to start a conversation…"
            }
            value={chatInput}
            rows={1}
            onChange={(e) => setChatInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                handleSendChat();
              }
            }}
          />
          <button
            className="rec send"
            type="submit"
            disabled={chatSending || !chatInput.trim()}
          >
            {chatSending ? "…" : "Send"}
          </button>
        </form>
      </footer>

      {convSession && (
        <ConversationModal
          sessionRef={convSession.ref}
          title={convSession.title}
          onClose={() => setConvSession(null)}
        />
      )}

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
    </div>
  );
}
