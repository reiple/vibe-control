import { Fragment, useEffect, useRef, useState } from "react";
import type {
  WorkBundle,
  Resource,
  SessionSnapshot,
  SessionCompletion,
  RestoreReport,
  RunningApp,
  ChatMsg,
  ClaudeStatus,
} from "./types";
import {
  getBundles,
  getSessionSnapshot,
  restoreBundle,
  resumeCodingSession,
  activateCodingSession,
  listRunningApps,
  activateApp,
  activateWindow,
  getAppIcon,
  createBundle,
  deleteBundle,
  addAppResource,
  claudeStatus,
  setClaudeApiKey,
  setClaudeModel,
  sendClaudeMessage,
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

// Full-viewport boot splash. Rendered by React (not static index.html markup)
// so it paints at the correct DPI on Windows/WebView2, and it blocks all input
// underneath until the app has finished its first load. `hiding` fades it out
// just before it unmounts. The EP-133 device motif — charcoal pad, orange
// record dot, an LCD segment chase — reads as the hardware "powering on".
function Splash({ hiding }: { hiding: boolean }) {
  return (
    <div
      className={`splash ${hiding ? "splash--hidden" : ""}`}
      role="progressbar"
      aria-label="Loading vibe-control"
      aria-busy="true"
    >
      <div className="splash-card">
        <div className="splash-badge">
          <span className="splash-rec" />
        </div>
        <div className="splash-word">vibe-control</div>
        <div className="splash-leds" aria-hidden>
          <span />
          <span />
          <span />
          <span />
          <span />
        </div>
        <div className="splash-cap">Initializing</div>
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
    const MIN_SPLASH_MS = 650;
    const MAX_SPLASH_MS = 8000;
    const FADE_MS = 400; // must match the .splash opacity transition

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
    // FR-7.6 concurrency guard: if the previous poll is still fetching, skip
    // this tick rather than stacking another OS enumeration on top of it. Only
    // background polls are gated — an explicit ↻ is a deliberate user action.
    if (silent && pollInFlight.current) return;
    if (silent) pollInFlight.current = true;
    if (!silent) setRefreshing(true);
    setSessionNonce((n) => n + 1); // also refresh inline coding-session statuses
    try {
      const apps = await listRunningApps();
      if (silent && draggedApp.current) return; // a drag began while fetching
      setRunningApps(apps);
    } catch (e) {
      // A background poll shouldn't flash the error banner every tick — only
      // surface failures from an explicit refresh.
      if (!silent) setError(String(e));
    } finally {
      if (silent) pollInFlight.current = false;
      if (!silent) setRefreshing(false);
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

  // "View": bring the session's already-open Claude Code terminal to the front
  // so the user reads/continues the real conversation there. Does NOT spawn a
  // new terminal (that's "Resume" → resumeCodingSession).
  const handleActivateSession = (resource: Resource) => {
    setError(null);
    activateCodingSession(resource.identity.descriptor).catch((e) =>
      setError(String(e))
    );
  };

  const handleResumeSession = (resource: Resource) => {
    setError(null);
    resumeCodingSession(resource.identity.descriptor).catch((e) =>
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
    } catch (e) {
      setChatError(errText(e)); // keep the user's message so they can retry
    } finally {
      setChatSending(false);
    }
  };

  return (
    <div className="app-shell">
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
                      key={`${app.name} ${w.handle} ${i}`}
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
                {b.resources.length === 0 && (
                  <li className="empty drop-hint">Drag apps here</li>
                )}
                {b.resources.map((r) => (
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
                    {r.kind === "CodingSession" && (
                      <>
                        <button
                          className="mini"
                          onClick={() => handleActivateSession(r)}
                          title="Bring the open Claude Code terminal to front"
                        >
                          View
                        </button>
                        <button
                          className="mini resume-button"
                          onClick={() => handleResumeSession(r)}
                          title="Resume this session in a new terminal"
                        >
                          Resume
                        </button>
                        <SessionStatus
                          sessionRef={r.identity.descriptor}
                          nonce={sessionNonce}
                        />
                      </>
                    )}
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
                      {report.skipped.length > 1 ? "s" : ""} — open each with its
                      “Resume” button.
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
