import { useEffect, useRef, useState } from "react";
import type {
  WorkBundle,
  Resource,
  SessionSnapshot,
  SessionCompletion,
  RestoreReport,
  RunningApp,
} from "./types";
import {
  getBundles,
  getSessionSnapshot,
  restoreBundle,
  resumeCodingSession,
  activateCodingSession,
  listRunningApps,
  activateApp,
  getAppIcon,
  createBundle,
  deleteBundle,
  addAppResource,
} from "./api";

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
  const [refreshing, setRefreshing] = useState(false);

  const [newGroupName, setNewGroupName] = useState("");
  const [showGroupModal, setShowGroupModal] = useState(false);
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);
  const [dragOverId, setDragOverId] = useState<string | null>(null);
  const draggedApp = useRef<RunningApp | null>(null);

  // Bumped by ↻ to force every visible SessionStatus to re-fetch its snapshot.
  const [sessionNonce, setSessionNonce] = useState(0);
  const [report, setReport] = useState<RestoreReport | null>(null);
  const [reportBundleId, setReportBundleId] = useState<string | null>(null);
  const [restoringId, setRestoringId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getBundles()
      .then(setBundles)
      .catch((e) => setError(String(e)));
    // Defer the running-apps scan (a comparatively slow OS call) to the next
    // frame so the app shell + saved groups paint immediately instead of the
    // window sitting blank until the scan returns on cold start.
    const raf = requestAnimationFrame(() => refreshRunning());
    return () => cancelAnimationFrame(raf);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const refreshRunning = async (silent = false) => {
    if (!silent) setRefreshing(true);
    setSessionNonce((n) => n + 1); // also refresh inline coding-session statuses
    try {
      setRunningApps(await listRunningApps());
    } catch (e) {
      // A background poll shouldn't flash the error banner every tick — only
      // surface failures from an explicit refresh.
      if (!silent) setError(String(e));
    } finally {
      if (!silent) setRefreshing(false);
    }
  };

  // Live status: poll running apps + coding-session snapshots every second so
  // the left panel and each group's session status stay current without the
  // user pressing ↻. Silent (no spinner / no error banner) to avoid flicker.
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

  return (
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
          {filteredApps.map((app) => (
            <li
              key={app.name}
              className="running-item"
              draggable
              onDragStart={(e) => onDragStartApp(e, app)}
              onDoubleClick={() => activate(app.bundle_id ?? app.name)}
              title="Double-click: bring to front · Drag: add to a group"
            >
              <AppIcon target={app.bundle_id ?? app.name} />
              <span className="running-name">{app.name}</span>
            </li>
          ))}
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
    </div>
  );
}
