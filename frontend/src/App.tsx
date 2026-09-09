import { Fragment, useEffect, useMemo, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type {
  WorkBundle,
  Resource,
  RestoreReport,
  RunningApp,
  ClaudeStatus,
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
  setClaudeApiKey,
  setClaudeModel,
  startInteractiveSession,
  startNewInteractive,
  submitInteractiveLine,
  interactiveScreen,
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
  kind === "AppLaunch" || kind === "WindowRef";

/** A group's live-terminal target: the descriptor of its (first) Claude Code
 *  session, or null when the group has no session to drive. */
const groupSessionRef = (b: WorkBundle): string | null =>
  b.resources.find((r) => r.kind === "CodingSession")?.identity.descriptor ??
  null;

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

// Detect macOS so the frameless-window custom controls (below) render on
// Windows ONLY — macOS keeps its native traffic lights (FR-8.11 / AC-22).
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

export default function App() {
  const [bundles, setBundles] = useState<WorkBundle[]>([]);
  const [runningApps, setRunningApps] = useState<RunningApp[]>([]);
  const [filter, setFilter] = useState("");
  // Names of app groups the user has expanded to reveal their windows (FR-2.8).
  // Keyed by app name so the expansion survives the 1s poll replacing the list.
  const [expandedApps, setExpandedApps] = useState<Set<string>>(new Set());
  const [refreshing, setRefreshing] = useState(false);

  // Boot splash: `booting` keeps the overlay mounted (blocking input);
  // `splashOut` triggers its fade just before we unmount it.
  const [booting, setBooting] = useState(true);
  const [splashOut, setSplashOut] = useState(false);

  const [newGroupName, setNewGroupName] = useState("");
  const [showGroupModal, setShowGroupModal] = useState(false);
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);
  const [dragOverId, setDragOverId] = useState<string | null>(null);
  const draggedApp = useRef<RunningApp | null>(null);

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

  const refreshRunning = async (silent = false) => {
    // Never re-render the running list mid-drag: replacing/reordering the
    // items cancels the in-flight native HTML5 drag before it can drop. The
    // background poll yields to an active drag; a manual ↻ can't collide with
    // a drag (one pointer) so it isn't gated.
    if (silent && draggedApp.current) return;
    if (!silent) setRefreshing(true);
    try {
      const apps = await listRunningApps();
      if (silent && draggedApp.current) return; // a drag began while fetching
      setRunningApps(apps);
    } catch (e) {
      // A background poll shouldn't flash the error banner every tick — only
      // surface failures from an explicit refresh.
      if (!silent) setError(String(e));
    } finally {
      if (!silent) setRefreshing(false);
    }
  };

  // Live status: poll running apps every second so the left panel stays current
  // without the user pressing ↻. Silent (no spinner / no error banner).
  useEffect(() => {
    const id = setInterval(() => refreshRunning(true), 1000);
    return () => clearInterval(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
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

  const toggleExpanded = (name: string) =>
    setExpandedApps((cur) => {
      const next = new Set(cur);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });

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
    e.dataTransfer.effectAllowed = "copy";
    e.dataTransfer.setData("text/plain", app.name);
  };

  const onDropToBundle = async (e: React.DragEvent, bundle: WorkBundle) => {
    e.preventDefault();
    setDragOverId(null);
    const app = draggedApp.current;
    draggedApp.current = null;
    if (!app) return;
    setError(null);
    try {
      const target = app.bundle_id ?? app.name;
      setBundles(await addAppResource(bundle.id, app.name, target));
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
      await startInteractiveSession(ref); // idempotent: reuses a live PTY
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
          descriptor: sessionId,
          hint: null,
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
      // the terminal is already live so the user sees it type + send).
      void submitWhenReady(sessionId, text);
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
      <aside className="sidebar">
        <header className="sidebar-header">
          <h1>Running Apps</h1>
          <button
            className="ghost icon"
            onClick={() => refreshRunning()}
            disabled={refreshing}
            title="Refresh"
          >
            ↻
          </button>
        </header>
        <input
          className="search"
          placeholder="Search apps…"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
        <ul className="running-list">
          {filteredApps.map((app) => {
            const wins = app.windows ?? [];
            const multi = wins.length > 1;
            const isExpanded = expandedApps.has(app.name);
            // Clicking an app: >1 window → expand/collapse; exactly 1 → activate
            // that window directly; 0 (only app-level info) → activate the app.
            const onAppClick = () => {
              if (multi) toggleExpanded(app.name);
              else if (wins.length === 1) activateWin(wins[0].handle);
              else activate(app.bundle_id ?? app.name);
            };
            return (
              <Fragment key={app.name}>
                <li
                  className={`running-item${multi ? " has-windows" : ""}${
                    multi && isExpanded ? " expanded" : ""
                  }`}
                  draggable
                  onDragStart={(e) => onDragStartApp(e, app)}
                  onDragEnd={() => {
                    // Clear the drag ref even when the drag is cancelled (dropped
                    // outside a group), so the paused poll resumes.
                    draggedApp.current = null;
                    setDragOverId(null);
                  }}
                  onClick={onAppClick}
                  title={
                    multi
                      ? "Click: show windows · Drag: add to a group"
                      : "Click: bring to front · Drag: add to a group"
                  }
                >
                  <AppIcon target={app.bundle_id ?? app.name} />
                  <span className="running-name">{app.name}</span>
                  {multi && (
                    <span className="running-count" aria-hidden>
                      {isExpanded ? "▾" : "▸"} {wins.length}
                    </span>
                  )}
                </li>
                {multi &&
                  isExpanded &&
                  wins.map((w, i) => (
                    <li
                      key={`${app.name}-${w.handle}-${i}`}
                      className="running-window"
                      onClick={() => activateWin(w.handle)}
                      title="Click: bring this window to front"
                    >
                      <span
                        className={`win-dot${w.is_focused ? " active" : ""}`}
                        aria-hidden
                      />
                      <span className="win-title">{w.title}</span>
                    </li>
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
        <header className="main-header">
          <h2>vibe-control</h2>
          <div className="header-actions">
            <button className="rec add-group" onClick={openGroupModal}>
              Add Group
            </button>
          </div>
        </header>

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
                    <button
                      className="mini"
                      onClick={() => setConfirmDeleteId(b.id)}
                    >
                      Delete
                    </button>
                  )}
                </div>
              </header>

              <ul className="resource-list">
                {appResources.length === 0 && (
                  <li className="empty drop-hint">Drag apps here</li>
                )}
                {appResources.map((r) => (
                  <li
                    key={r.id}
                    className={`resource ${isActivatable(r.kind) ? "activatable" : ""}`}
                    onDoubleClick={() =>
                      isActivatable(r.kind) &&
                      activate(r.identity.reopen_info ?? r.identity.descriptor)
                    }
                    title={
                      isActivatable(r.kind)
                        ? "Double-click: bring to front · opens it if closed"
                        : undefined
                    }
                  >
                    {isActivatable(r.kind) && (
                      <AppIcon
                        target={r.identity.reopen_info ?? r.identity.descriptor}
                        className="app-icon app-icon--sm"
                      />
                    )}
                    <span className="badge">{kindLabel[r.kind] ?? r.kind}</span>
                    <span className="resource-name">{r.display_name}</span>
                  </li>
                ))}
              </ul>

              {(() => {
                const ref = groupSessionRef(b);
                if (!ref) return null;
                const open = openTerms.has(b.id);
                return (
                  <div className="group-live">
                    <div className="group-live-bar">
                      <span className="group-live-title">터미널</span>
                      <button
                        className="mini"
                        onClick={() =>
                          open ? closeGroupTerminal(b.id) : openGroupTerminal(b.id)
                        }
                        title={
                          open
                            ? "터미널 뷰를 접습니다 (claude는 계속 실행)"
                            : "이 그룹의 claude 터미널을 엽니다"
                        }
                      >
                        {open ? "접기" : "열기"}
                      </button>
                      {promptTarget === b.id && (
                        <span className="group-live-target" title="프롬프트 바 기본 대상">
                          ◀ 프롬프트 대상
                        </span>
                      )}
                    </div>
                    {open && <GroupTerminal sessionRef={ref} />}
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
                      @{b.name}
                      <span className="mention-sub">
                        {groupSessionRef(b) ? "세션 있음" : "세션 없음"}
                      </span>
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
                  if (e.key === "ArrowDown") {
                    e.preventDefault();
                    setMentionActive(
                      (i) => (i + 1) % mentionSuggestions.length
                    );
                    return;
                  }
                  if (e.key === "ArrowUp") {
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
                  ‘{bundle?.name}’ 그룹에 아직 claude 세션이 없습니다. 작업 폴더를
                  지정하면 그 폴더에서 새 세션(터미널)을 시작하고 아래 내용을
                  보냅니다.
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
                <p className="modal-note pending-text" title={pendingPrompt.text}>
                  보낼 내용: {pendingPrompt.text}
                </p>
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
                    {creatingSession ? "시작 중…" : "시작하고 보내기"}
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
    </div>
  );
}
