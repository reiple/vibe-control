import { Fragment, useEffect, useRef, useState } from "react";
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
const INTRO_HTML = `<main id="hero" class="hero" hidden><div class="hero-top">SP●DEV</div><div class="hero-grid"></div></main><div class="loader-loader" id="loader" role="status" aria-label="Loading"><div class="loader-wrapper"><div class="loader-empty loader-empty1"><p>S</p></div><div class="loader-empty loader-empty2"><p>P</p></div><div class="loader-text"><h2>Fake loading...</h2></div><div class="loader-slash"><p>/</p></div><div class="loader-number loader-number1"><p>0</p></div><div class="loader-number loader-number2"><p>0</p></div><div class="loader-number loader-number3"><p>0</p></div><div class="loader-percent"><p>%</p></div><div class="loader-logo"><svg width="45" height="10" viewBox="0 0 45 10" fill="none" xmlns="http://www.w3.org/2000/svg"><g clip-path="url(#clip0_517_15380)"><path d="M30.0915 4.98203C30.0915 6.13403 29.9835 7.20203 29.5035 7.97003C29.0235 8.75003 28.1235 9.25403 26.7915 9.25403H24.2595V0.746033H26.7915C28.1235 0.746033 29.0355 1.28603 29.5155 2.06603C29.9955 2.83403 30.0915 3.83003 30.0915 4.98203ZM28.9995 4.98203C28.9995 3.89003 28.9515 2.89403 28.3635 2.29403C28.0395 1.95803 27.5235 1.73003 26.7915 1.73003H25.3275V8.27003H26.7915C27.5595 8.27003 28.0755 8.01803 28.3995 7.67003C28.9635 7.05803 28.9995 6.03803 28.9995 4.98203Z" fill="#1A1A1A"></path><path d="M37.2569 9.25403H31.6289V0.746033H37.2569V1.73003H32.6969V4.32203H36.1409V5.33003H32.6969V8.27003H37.2569V9.25403Z" fill="#1A1A1A"></path><path d="M44.6263 0.746033L41.9863 9.25403H40.7143L38.0743 0.746033H39.2143L41.3743 8.01803L43.4983 0.746033H44.6263Z" fill="#1A1A1A"></path><rect x="16.2595" y="0.746033" width="6" height="6" rx="3" fill="#1A1A1A"></rect><path d="M6.78178 6.91403C6.78178 7.62203 6.49378 8.16203 6.07378 8.57003C5.48578 9.14603 4.58578 9.41003 3.67378 9.41003C2.64178 9.41003 1.83778 9.13403 1.26178 8.60603C0.721779 8.10203 0.373779 7.37003 0.373779 6.56603H1.48978C1.48978 7.07003 1.72978 7.57403 2.07778 7.91003C2.46178 8.28203 3.07378 8.42603 3.67378 8.42603C4.32178 8.42603 4.84978 8.29403 5.23378 7.93403C5.49778 7.69403 5.66578 7.39403 5.66578 6.93803C5.66578 6.27803 5.26978 5.72603 4.26178 5.57003C3.79378 5.49803 3.40978 5.43803 2.95378 5.36603C1.68178 5.17403 0.709779 4.46603 0.709779 3.11003C0.709779 2.47403 0.973779 1.86203 1.42978 1.43003C2.01778 0.878027 2.73778 0.590027 3.62578 0.590027C4.45378 0.590027 5.24578 0.842027 5.80978 1.38203C6.32578 1.87403 6.58978 2.49803 6.61378 3.21803H5.49778C5.47378 2.79803 5.32978 2.45003 5.08978 2.17403C4.76578 1.80203 4.27378 1.57403 3.61378 1.57403C3.00178 1.57403 2.50978 1.75403 2.13778 2.17403C1.92178 2.42603 1.81378 2.70203 1.81378 3.08603C1.81378 3.85403 2.42578 4.20203 3.06178 4.28603C3.54178 4.34603 3.97378 4.43003 4.44178 4.50203C5.85778 4.70603 6.78178 5.57003 6.78178 6.91403Z" fill="#1A1A1A"></path><path d="M14.2592 3.31403C14.2592 4.14203 13.9952 4.75403 13.5392 5.21003C13.0832 5.66603 12.2792 5.95403 11.3432 5.95403H9.33919V9.25403H8.27119V0.746027H11.3312C12.3272 0.746027 13.1432 1.05803 13.5992 1.55003C14.0072 1.99403 14.2592 2.57003 14.2592 3.31403ZM13.1552 3.31403C13.1552 2.23403 12.3632 1.71803 11.3072 1.71803H9.33919V4.98203H11.3192C12.4592 4.98203 13.1552 4.45403 13.1552 3.31403Z" fill="#1A1A1A"></path></g><defs><clipPath id="clip0_517_15380"><rect width="45" height="10" fill="white"></rect></clipPath></defs></svg></div><div class="loader-arrow loader-arrow1"><p>&gt;</p></div></div></div><button class="replay" id="replay" hidden>다시 재생 ↗</button>`;

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
    // Floor the visible time to the length of the ported intro animation so the
    // full sequence (fake-loading counter → "DEV" morph → hero letters decoding
    // into the wordmark, ~4.9s at the sped-up timing) plays before the fade,
    // rather than being cut short the instant the (usually faster) boot data
    // lands. The hard cap still guarantees the splash lifts if boot stalls.
    const MIN_SPLASH_MS = 5100;
    const MAX_SPLASH_MS = 8000;
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
