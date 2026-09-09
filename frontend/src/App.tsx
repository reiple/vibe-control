import { Fragment, useEffect, useMemo, useRef, useState } from "react";
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
  getAppIcon,
  createBundle,
  deleteBundle,
  removeResource,
  addAppResource,
  addChildResource,
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
// The choreography tells the product's story (intro-animation.md) in the KO-II
// hardware sampler's motion language — tactile pads, step-sequencer timing,
// pulse feedback (NOT a music UI). The tool pads are laid out as a calm,
// organised 5-row control grid centred on screen (NOT randomly scattered): a
// central activation pulse ripples outward, lighting the pads row-by-ring in
// rhythm, then the "vibe control" logo resolves in the exact centre as the
// unifying core the pads are arranged around. The grid is held faded (low
// opacity) so it reads as an atmospheric control surface and never competes
// with the logo — the logo is always the clear focal point. Pad glyphs and the
// shuffled tie-break are generated once on mount; the motion is CSS-keyframe
// driven.
const SWEEP_S = 2.6; // total time the activation ripple takes to cross the grid
const GRID_ROWS = 5; // requirement: five centred rows
const CELL_PX = 40; // pad size
const GAP_PX = 16; // gap between pads
// Harmonious accent hues for the pad-press flash (driven via --hue in the
// activation keyframe). Anchored on the brand orange and spread across warm →
// magenta → violet → blue → teal so the field lights up in varied but
// coordinated colour rather than a single orange.
const FLASH_HUES = [16, 34, 350, 320, 275, 210, 172, 150];

function Splash({ hiding }: { hiding: boolean }) {
  const pads = useMemo(() => {
    const rows = GRID_ROWS;
    const stride = CELL_PX + GAP_PX;
    // Fill the viewport width: enough columns so the five rows span edge to
    // edge (with a small side margin), then centre the whole grid on the
    // anchor so it stays symmetric. Computed once on mount.
    const vw = typeof window !== "undefined" ? window.innerWidth : 1280;
    const cols = Math.max(7, Math.floor((vw - GAP_PX) / stride));
    const N = cols * rows;
    const originX = ((cols - 1) / 2) * stride;
    const originY = ((rows - 1) / 2) * stride;
    const cx = (cols - 1) / 2;
    const cy = (rows - 1) / 2;

    const cells = Array.from({ length: N }, (_, i) => {
      const col = i % cols;
      const row = Math.floor(i / cols);
      // Activation order ripples out from the centre (where the logo lights
      // up), with a small random tie-break so same-distance pads feel tactile
      // rather than mechanically simultaneous.
      const dist = Math.hypot(col - cx, row - cy) + Math.random() * 0.35;
      return {
        id: i,
        glyph: i % TOOL_GLYPHS.length,
        x: `${(col * stride - originX).toFixed(1)}px`,
        y: `${(row * stride - originY).toFixed(1)}px`,
        hue: FLASH_HUES[Math.floor(Math.random() * FLASH_HUES.length)],
        dist,
      };
    });
    // Rank by ripple distance, then normalise so the sweep lasts SWEEP_S no
    // matter how many columns the viewport fits.
    const byDist = [...cells].sort((a, b) => a.dist - b.dist);
    const delay = new Map(
      byDist.map((c, rank) => [c.id, 0.6 + (rank / Math.max(1, N - 1)) * SWEEP_S]),
    );
    return cells.map((c) => ({ ...c, delay: delay.get(c.id) ?? 0.6 }));
  }, []);

  return (
    <div
      className={`splash ${hiding ? "splash--hidden" : ""}`}
      role="progressbar"
      aria-label="Loading vibe-control"
      aria-busy="true"
    >
      <div className="intro">
        {/* Faded control grid: five centred rows of tool pads, held quietly
            behind the logo (low opacity) so they never out-shout it. */}
        <div className="intro-field" aria-hidden>
          {pads.map((p) => (
            <span
              key={p.id}
              className="intro-pad"
              style={
                {
                  "--x": p.x,
                  "--y": p.y,
                  "--size": `${CELL_PX}px`,
                } as React.CSSProperties
              }
            >
              <span
                className="intro-pad-face"
                // Each pad snaps "on" at its step in the outward ripple, in its
                // own accent hue.
                style={
                  {
                    animationDelay: `${p.delay.toFixed(2)}s`,
                    "--hue": p.hue,
                  } as React.CSSProperties
                }
              >
                <span className="intro-pad-glyph">{TOOL_GLYPHS[p.glyph]}</span>
              </span>
            </span>
          ))}
        </div>

        {/* The logo — the clear focal point. A soft spotlight lifts it off the
            faded grid, then the record-dot mark + wordmark resolve. */}
        <div className="intro-logo">
          <span className="intro-logo-mark" aria-hidden />
          <div className="intro-logo-word">vibe control</div>
        </div>

        {/* Final confirmation pulse — "centralised control" locked in. */}
        <span className="intro-confirm" aria-hidden />
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

  // Boot splash: `booting` keeps the overlay mounted (blocking input);
  // `splashOut` triggers its fade just before we unmount it.
  const [booting, setBooting] = useState(true);
  const [splashOut, setSplashOut] = useState(false);

  const [newGroupName, setNewGroupName] = useState("");
  const [showGroupModal, setShowGroupModal] = useState(false);
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);
  const [dragOverId, setDragOverId] = useState<string | null>(null);
  const draggedApp = useRef<RunningApp | null>(null);

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
    // full sequence (activation ripple across the grid → logo mark → wordmark →
    // confirmation) plays before the fade, rather than being cut short the instant the
    // (usually faster) boot data lands. The hard cap still guarantees the
    // splash lifts if boot stalls.
    const MIN_SPLASH_MS = 5800;
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
    if (silent && (draggedApp.current || draggedChild.current)) return;
    // FR-7.6 concurrency guard: if the previous poll is still fetching, skip
    // this tick rather than stacking another OS enumeration on top of it. Only
    // background polls are gated — an explicit ↻ is a deliberate user action.
    if (silent && pollInFlight.current) return;
    if (silent) pollInFlight.current = true;
    setSessionNonce((n) => n + 1); // also refresh inline coding-session statuses
    try {
      const apps = await listRunningApps();
      if (silent && (draggedApp.current || draggedChild.current)) return; // a drag began while fetching
      setRunningApps(apps);
      // Keep expanded apps' tab/window lists live (a browser's tabs change as
      // the user navigates). Background refresh: no spinner, yields to drags.
      for (const key of expandedRef.current) {
        const a = apps.find((x) => appKey(x) === key);
        if (a) void fetchChildren(a, true);
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
      // A window ref's reopen_info is the opaque window handle; raise that window.
      case "WindowRef":
        p = activateWindow(reopen);
        break;
      default:
        p = activateApp(reopen);
    }
    p.catch((e) => setError(errText(e)));
  };

  const onDragStartChild = (e: React.DragEvent, w: RunningWindow) => {
    draggedChild.current = {
      kind: w.kind,
      name: w.title,
      target: w.target,
      handle: w.handle,
    };
    draggedApp.current = null;
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
    e.dataTransfer.effectAllowed = "copy";
    e.dataTransfer.setData("text/plain", app.name);
  };

  const onDropToBundle = async (e: React.DragEvent, bundle: WorkBundle) => {
    e.preventDefault();
    setDragOverId(null);
    const child = draggedChild.current;
    const app = draggedApp.current;
    draggedChild.current = null;
    draggedApp.current = null;
    setError(null);
    try {
      // A child (tab/folder/window) registers as its own resource kind; a whole
      // app registers as an app launch (FR-2.2 / §13.3).
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
    <div className="app-shell">
    {booting && <Splash hiding={splashOut} />}
    <div className="app">
      <aside className="sidebar">
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
                    className={`resource ${isActivatable(r.kind) ? "activatable" : ""}`}
                    onDoubleClick={() =>
                      isActivatable(r.kind) && activateResource(r)
                    }
                    title={
                      isActivatable(r.kind)
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
