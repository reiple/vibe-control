# AI-DLC State Tracking

## Project Information
- **Project Type**: Greenfield (built out; now in brownfield doc-reconciliation)
- **Start Date**: 2026-09-07T07:32:51Z
- **Current Stage**: CONSTRUCTION built + post-construction fixes/features landed → Documentation Reconciliation FINALIZED (2026-09-08) → **NEW FEATURE in INCEPTION doc-reflection (2026-09-08): per-window expansion UX** — realign running-apps panel to the already-designed per-window model (app-grouped list, click-to-expand window/tab sub-list, exact-window activation) + new expand/collapse affordance. Units: U7/U6/U4/U3 primary, U1 supporting; U5/U2 out of scope. Requirements/design docs updated (FR-2.8, AC-20, REQUIREMENTS.ko §13.1/13.4/13.6, known-deviations G1–G3, new vc-os-windows functional-design). **G1/G2/G3 all implemented + verified on real Windows (G1 enumeration replaced with `EnumWindows` native FFI, 2026-09-09) — feature complete.** **Supplement Bolt (2026-09-09): Windows browser TABS live view + exact-tab activation via UI Automation (`#B3` live portion) — implemented + verified on real Windows.** **Supplement Bolt 2 (2026-09-09): live tabs registerable as INDIVIDUAL bundle resources (new focus-only `ResourceKind::BrowserTabLive`, title-based, no URL per FR-9.7) — drag-to-add, per-tab exact activation, full-hint dedup; implemented + verified on real Windows (`#B3`/`#H2`).** Only capture-time tab URL persistence remains (needs DevTools protocol). Operations still placeholder.
- **Project Name**: vibe-control (작업 맥락 전환 데스크톱 앱)

## Workspace State
- **Existing Code**: Yes — full Rust Cargo workspace + React/Vite frontend implemented
- **Programming Languages**: Rust (workspace crates) + TypeScript/React (frontend)
- **Build System**: Cargo workspace + Vite (Tauri v2)
- **Project Structure**: `crates/{vc-core,vc-store,vc-os-macos,vc-os-windows,vc-sessions,vc-app}` + `frontend/` + `aidlc-docs/`
- **Reverse Engineering Needed**: No (docs authored during construction; reconciled 2026-09-08)
- **Workspace Root**: C:\Users\gayeon\Documents\coding\vibe-control (dev on Windows; also built on macOS)

## Code Location Rules
- **Application Code**: Workspace root (NEVER in aidlc-docs/)
- **Documentation**: aidlc-docs/ only
- **Structure patterns**: See code-generation.md Critical Rules

## Extension Configuration
| Extension | Enabled | Mode | Decided At |
|---|---|---|---|
| Security Baseline | Yes | Full (all rules blocking) | Requirements Analysis |
| Resiliency Baseline | No | — (skipped, rules not loaded) | Requirements Analysis |
| Property-Based Testing | Yes | Partial (only PBT-02, PBT-03, PBT-07, PBT-08, PBT-09 blocking; others advisory) | Requirements Analysis |

## Technical Decisions (from Requirements Analysis)
| Decision | Choice |
|---|---|
| Framework | Tauri (Rust core + web frontend) |
| Target OS (this iteration) | macOS + Windows simultaneously |
| Browser support | macOS: Safari + Chrome / Windows: Edge + Chrome |
| Data storage | Single JSON file (atomic temp-file swap + version-based migration) |
| Test scope | Domain unit tests + mocked OS-adapter integration tests + real-OS E2E/manual checklist |
| Coding-agent sessions | Claude Code first (extensible adapters); read local session files; full conversation view; new "coding session" resource type |

## Execution Plan Summary
- **Stages to Execute**: Application Design, Units Generation, Functional Design, NFR Requirements, NFR Design, Code Generation, Build and Test
- **Stages to Skip**: Reverse Engineering (greenfield), Infrastructure Design (local desktop app — no cloud infra)
- **Risk Level**: High (OS-native automation + browser/session parsing dependencies)

## Stage Progress
### 🔵 INCEPTION PHASE
- [x] Workspace Detection
- [x] Reverse Engineering (SKIP — greenfield)
- [x] Requirements Analysis (approved)
- [x] User Stories (approved — personas.md + stories.md)
- [x] Workflow Planning (execution-plan.md — approved)
- [x] Application Design — EXECUTE (approved)
- [x] Units Generation — EXECUTE (approved — 7 units)

### 🟢 CONSTRUCTION PHASE (per-unit loop, foundation-first: U1→U2→U3‖U4→U5→U6→U7)
Per-unit stages: Functional Design → NFR Requirements → NFR Design → (Infrastructure Design SKIP) → Code Generation

**U1 vc-core**
- [x] Functional Design (approved)
- [x] NFR Requirements (approved)
- [x] NFR Design (approved)
- [x] Code Generation (approved — models, matching, restore, evaluate, normalize, migrate, error, lib.rs)

**U2 vc-store**
- [x] Functional Design (atomic save, store-contract, atomic-operations)
- [x] NFR Requirements (reliability, performance, security)
- [x] NFR Design (patterns, tech-stack)
- [x] Code Generation (lib.rs, Cargo.toml with BundleStore impl)

**U3 vc-os-macos**
- [x] Functional Design (Accessibility API)
- [x] NFR Requirements (auto)
- [x] NFR Design (auto)
- [x] Code Generation (lib.rs with WindowEnumerator)

**U4 vc-os-windows**
- [x] Functional Design (design said UI Automation; **actual code uses PowerShell `Get-Process` filtered on `MainWindowHandle`/`MainWindowTitle`** — user-launched visible-window apps only — see `known-deviations.md#B1`. App icons now implemented via `WinIconReader` (commit `6039456`, `known-deviations.md#B4`). Still NOT implemented: browser-tab reading (`#B3`), tray/global-hotkey (`#B5`))
- [x] NFR Requirements (auto)
- [x] NFR Design (auto)
- [x] Code Generation (lib.rs with WinWindowEnumerator)

**U5 vc-sessions**
- [x] Functional Design (SessionProvider + Registry)
- [x] NFR Requirements (auto)
- [x] NFR Design (auto)
- [x] Code Generation (lib.rs with ClaudeCodeSessionProvider)

**U6 vc-app**
- [x] Functional Design (AppState + Services)
- [x] NFR Requirements (auto)
- [x] NFR Design (auto)
- [x] Code Generation (lib.rs with AppState + Tauri stubs)

**U7 frontend** (React + Vite + TS, Tauri v2)
- [x] Frontend scaffold (frontend/: React + Vite + TS, types/api/App/styles)
- [x] Tauri backend (vc-app: main.rs, lib.rs commands, build.rs, tauri.conf.json, capabilities, icons)
- [x] Tauri commands (actual: **17** registered in `vc-app/src/lib.rs` invoke_handler): get_bundles, save_bundles, capture_current, get_session_snapshot, restore_bundle, resume_coding_session, activate_coding_session, list_running_apps, activate_app, get_app_icon, create_bundle, delete_bundle, add_app_resource, claude_status, set_claude_api_key, set_claude_model, send_claude_message. (`capture_current`/`save_bundles` exist but the UI no longer calls them — `known-deviations.md#E3`)
- [x] `npm run build` succeeds; `cargo build -p vc-app` succeeds
- [x] App binary launches (window created, no crash)

### Build & Test verification (U1–U6)
- [x] Root workspace Cargo.toml created; missing crate manifests added
- [x] `cargo build --workspace` succeeds
- [x] `cargo test --workspace` — **33 test fns present** (grep: vc-core 22 / vc-sessions 8 / vc-store 2 / vc-os-macos 1; macOS-gated tests run only on macOS). Earlier "24"/"22, 20 in vc-core" figures were stale.
- [x] `cargo clippy --workspace --all-targets` — 0 warnings (as of last recorded run)
- [x] PBT-02 (roundtrip stable) proptest in **`vc-core/src/migrate/mod.rs`**; PBT-03 (parser robust) proptest in **`vc-sessions/src/lib.rs`** (NOT vc-core — see `known-deviations.md#D5`). Both inline, no `tests/` dir.
- [x] Build & Test instruction docs generated (build-and-test/)

### After all units:
- [x] Build and Test — finalized (workspace builds, tests pass — see corrected count above, clippy clean, Tauri app runs)

### 🟡 OPERATIONS PHASE
- [ ] Operations — PLACEHOLDER

### 🔧 Post-Construction Fixes
- [x] **Windows freeze fix (2026-09-08)** — running-apps list was empty and the window went "not responding" then closed. Cause: `WinWindowEnumerator` used `tasklist /v` (hangs for minutes when any window is unresponsive) from a SYNCHRONOUS Tauri command polled every 1s on the UI thread. Fixed by (a) enumerating via PowerShell `Get-Process`/`MainWindowTitle` (no window messaging → no hang, ~1s) in `crates/vc-os-windows/src/lib.rs`, and (b) making OS-touching commands `async` in `crates/vc-app/src/lib.rs` so they run off the main thread. Verified on real Windows: clippy 0 warnings, app stable, `Responding = True`. Detail in `audit.md`. Still open on Windows: browser-tab reading + app-icon extraction. *(Update 2026-09-08: app-icon extraction since implemented — `WinIconReader`, commit `6039456`; browser-tab reading still open.)*
- [x] **Windows duplicate-window fix (2026-09-08)** — double-clicking a running app opened a NEW window instead of focusing the one already open. Cause: `WinLauncher::open_app` went straight to PowerShell `Start-Process`, which always spawns a new instance; there was no "focus if already running" path (macOS `open` gets this for free, Windows does not) — violating FR-2.6 / §13.4. Fixed in `crates/vc-os-windows/src/lib.rs`: `open_app` now calls new `focus_existing_window(target)` first (finds a windowed process matching the target's ProcessName or exe Path, then restores + foregrounds it via Win32 `ShowWindowAsync`/`SetForegroundWindow` + `WScript.Shell.AppActivate`, all inline PowerShell — no native crate), and only falls back to `Start-Process` when no instance is running. Verified on real Windows: clippy 0 warnings; live test — running app → focused with no duplicate process (count before==after), bogus target → still launches. Detail in `audit.md`.
- [x] **Windows: user-launched apps only + app icons (2026-09-08, commit `6039456`)** — running-apps list now filters `Get-Process` on `MainWindowHandle != 0 && MainWindowTitle` (excludes background services / invisible helper windows that the old title-only heuristic leaked), de-duped by name. New `WinIconReader` extracts each exe's shell icon (PowerShell + C# `Add-Type`, shell32 `SHGetImageList` 256px jumbo → 48px → 32px fallback, transparent-margin trim) as a base64 PNG data URI; `vc-app` caches icons per bundle id. Resolves `known-deviations.md#B4`. (This feature had no prior audit/state entry — backfilled in `audit.md`.)
- [x] **In-app Claude prompt console via AWS Bedrock (2026-09-08, commit `a163d8d`)** — NEW feature not in original requirements/design: `crates/vc-app/src/claude.rs` (Bedrock runtime client) + 4 Tauri commands (`claude_status`, `set_claude_api_key`, `set_claude_model`, `send_claude_message`) + `AppSettings` fields (`claude_api_key`/`claude_model`/`claude_region`) + `reqwest` dep + frontend footer console UI. **Sends user prompts over HTTPS to Bedrock → conflicts with original NFR-S1 (local-only)**, now reconciled: NFR-S1 scoped to bundle/session DATA, console governed by new NFR-S3. Design doc: `construction/vc-app/claude-console/design.md`. Deviation: `known-deviations.md#F1`.

### 📝 Documentation Reconciliation (2026-09-08)
Read-only doc-vs-code audit (3 review agents) found the docs had drifted from the built code. Per user decision (code-as-truth + deviation record / formally document Bedrock + amend NFR / backfill audit):
- [x] `known-deviations.md` created — central record of design-vs-code gaps (A architecture, B adapters, C persistence, D domain, E frontend, F new feature) + prioritized backlog. **No code was modified.**
- [x] `construction/vc-app/claude-console/design.md` created — Bedrock console feature + command spec + security note.
- [x] `aidlc-state.md` refreshed — workspace state, current stage, test counts, command list, U4 enumeration mechanism.
- [x] `requirements.md` — NFR-S1 scoped + NFR-S3 added + SECURITY-01/12 mapping updated for the console.
- [x] `application-design/*` (components, services, unit-of-work, component-methods) — "구현 현황" banners added; UI-Automation claim corrected.
- [x] `audit.md` — retroactive entries backfilled for the Bedrock feature + this reconciliation.
- **Open backlog (code, not done here)**: see `known-deviations.md` — full conversation viewer, layout persistence, temp-file rollback, duplicate-identity invariant, Windows tabs/icons/tray, port-trait refactor.

### 🚧 In-Progress Feature — Running-resources per-window expansion UX (2026-09-08)
Re-align the running-apps left panel to the ORIGINAL per-window design (which the §13 clarifications + shipped code had collapsed to app-level) and add an explicit expand/collapse affordance. **Doc-reflection stage (this turn) — no code modified yet.**
- **Unit mapping**: U7 frontend (app-grouped list + per-window expand/collapse sub-list, distinguishable titles, click-window-to-activate, per-window green dot; preserve FR-3 DnD + bundling), U6 vc-app (per-app window list in enumeration command + per-window activate command), U4 vc-os-windows + U3 vc-os-macos (per-WINDOW enumerate + focus a specific window/HWND) — all **primary**; U1 vc-core (`RunningItem` + `matching/window.rs` L2 matcher — designed-but-unwired, now activated) — **supporting**. OUT of scope: U5 vc-sessions (different "session" concept — terminology kept distinct: window/tab vs session), U2 vc-store (live windows not persisted).
- **Docs updated**: `requirements.md` FR-2.8 + AC-20 + FR-2 reconciliation note; `REQUIREMENTS.ko.md` §13.1/§13.4 revised (app-level → app-grouped-with-window-expansion) + new §13.6; `known-deviations.md` B1 annotated + new section G (G1 per-window enumeration, G2 per-window activation, G3 expand/collapse UI) + backlog "진행중" row; NEW `construction/vc-os-windows/functional-design/window-enumeration.md`; reconciliation notes on `application-design/{services,components}.md`, `construction/vc-core/functional-design/domain-entities.md`, `construction/vc-os-macos/functional-design/accessibility-api.md`.
- **Implemented (2026-09-08)**:
  - U4 `vc-os-windows`: `LIST_APPS_SCRIPT` → `LIST_WINDOWS_SCRIPT` (one row per windowed process = per-window for process-per-window apps; `HWND\tName\tPath\tFocused\tTitle`, foreground via a tiny cached `Add-Type GetForegroundWindow`); `list_running_windows()` groups by name (icon once) keeping each window; `list_running_apps()` re-derived on top (dedup) so capture is unchanged; new `WinLauncher::focus_window(hwnd)` (validated decimal HWND via env var; `IsWindow`→GONE, `ShowWindowAsync`+`AppActivate(pid)`+`SetForegroundWindow`).
  - U3 `vc-os-macos`: `list_running_windows()` = `NSWorkspace` app list (no permission, no regression) overlaid with `System Events` per-window titles + frontmost flag; handle = `name\u{1f}title`; `MacLauncher::focus_window(handle)` activates app then best-effort `AXRaise` by title.
  - U6 `vc-app`: `RunningApp` now carries `windows: Vec<RunningWindow{handle,title,is_focused}>`; `list_running_apps` returns grouped windows via new `enumerate_running_windows()`; new `activate_window(handle)` command + `focus_window` dispatch; registered in `generate_handler!`.
  - U7 frontend: `types.ts` `RunningWindow` + `RunningApp.windows`; `api.ts` `activateWindow`; `App.tsx` `expandedApps` state + click-to-expand (>1 window) / click-to-activate (1) / activate-app (0 fallback) + per-window sub-rows with green `is_focused` dot; `styles.css` `.running-window`/`.win-dot`/`.win-title`/`.running-count` (indented; pad index only counts `.running-item`). DnD app-row unchanged.
- **Verified**: frontend `tsc`+build clean; `cargo build -p vc-app` clean; enumeration script emits per-window rows (2× `mspaint` grouped, WindowsTerminal `focused=1`); `focus_window` resolves HWND→PID (58772) + `IsWindow` GONE guard; live app screenshotted — `mspaint` group shows `▸ 2`, click expands to `▾ 2` + two `제목 없음 - 그림판` sub-rows with dots, pad numbering continues to `06` (sub-rows excluded). Single-window apps show no chevron.
- **CONFIRMED DEFECT (2026-09-08) → ✅ RESOLVED (2026-09-09, `known-deviations.md#G1` "✅ G1 해결")**: the shipped `Get-Process.MainWindowHandle` enumeration yielded **only one window per process**, collapsing every multi-window-per-process app. Fixed by re-implementing enumeration on Win32 `EnumWindows` (native Rust FFI). Verified live: msedge/chrome/KakaoTalk each now split into their real windows. macOS per-window still needs Accessibility; degrades to app-level otherwise (unchanged).
- [x] INCEPTION doc-reflection (requirements + design updated)
- [x] Construction / Code Generation (U4→U3→U6→U7) — **G1 enumeration NOW on `EnumWindows` native FFI; G2 activation + G3 expand/collapse UI done**
- [x] Build + test + real-OS verification (AC-20 on Windows; regression AC-2/3/4 — DnD app-row + capture dedup unchanged) — **multi-window-per-process (Edge/Chrome/KakaoTalk) now split correctly (2026-09-09)**

### ✅ DONE (2026-09-09): G1 window enumeration replaced with `EnumWindows` native FFI
**Outcome**: The last broken piece — enumeration listing one window per process — is fixed. `crates/vc-os-windows/src/lib.rs` `raw_windows()` now walks all top-level windows via Win32 `EnumWindows` through inline native Rust FFI (`#[link]` `extern "system"` to user32/dwmapi/kernel32 — no `windows`/`winapi` crate, no per-poll `csc` recompile). Only `raw_windows()` changed; `list_running_windows`/`list_running_apps`/vc-app/frontend contracts untouched (minimal blast radius). Details + evidence in `known-deviations.md#G1` ("✅ G1 해결").
- [x] **Reviewed yesterday's changes** (`git diff` since `bbc54e7`): UI (G3) + per-window activation (G2) confirmed complete; only enumeration needed the fix.
- [x] **Replaced enumeration**: `Get-Process.MainWindowHandle` → `EnumWindows`. Validated filter (`IsWindowVisible` + `GetWindowTextLengthW>0` + `GetWindow(GW_OWNER=4)==0` + `!(GWL_EXSTYLE & WS_EX_TOOLWINDOW 0x80)` + `!DWMWA_CLOAKED(14)`). HWND→PID `GetWindowThreadProcessId`; PID→exe path `QueryFullProcessImageNameW` (best-effort) + PID→name Toolhelp snapshot (guaranteed, preserves elevated-app names); foreground `GetForegroundWindow`. Grouped by name (icon once). `focus_window(hwnd)` kept as-is.
- [x] **Native Rust FFI** used (not inline C# `Add-Type`) — subprocess/compile-free per 1 s poll.
- [x] **Verified**: `cargo build -p vc-app` + `cargo clippy --workspace` 0 warnings; harness (`list_running_windows`) → Chrome=2, msedge=2, KakaoTalk=2 (main+chat, focused dot correct), WindowsTerminal=2, mspaint/Code/Obsidian=1 (no regression), explorer Program Manager excluded as tool window; live app screenshot → KakaoTalk/msedge `▾ 2` expand to per-window sub-rows with dots, pad numbering counts app rows only.
- [x] **Browser TABS remain OUT of scope** (`#B3`): windows now split; multiple tabs in one window still show as one (separate BrowserTabReader work).
- [x] Updated `known-deviations.md#G1` (resolved), `aidlc-state.md`, `audit.md`; deleted `diag_windows.ps1`.

### ✅ DONE (2026-09-09): Supplement Bolt — Windows browser TABS (live view + exact-tab activation)
**Trigger**: user request — detect a supported browser's currently-open tabs as individual selectable sessions and activate exactly the chosen tab (requirements #2/#3), while preserving multi-window enumeration (#1, already done via `EnumWindows`), bundle/status/restore compat (#4), no system/aux resources leaking (#5), and analyzing impact before working around conflicts (#6). Continuation of the per-window Bolt (U4→U6→U7), `#B3` live portion.
- **Impact analysis first (#6)**: verified #1 (multi-window) was ALREADY fixed by `EnumWindows` (commit `5d931ad`) — no rework of enumeration (which would be the arbitrary change the user warned against). The genuinely-open work was the `WinBrowserTabReader` tab stub (`#B3`). Empirically validated the platform limit (Chromium lazy-a11y → only foreground window's tabs readable; no per-tab URL) BEFORE coding; scope confirmed via the user's approved choice ("UIA, active-window tabs").
- **U4 `vc-os-windows`**: `WinBrowserTabReader::list_tabs(process)` (UIA enumerate tab strip → `(handle_token=`hwnd\u{1f}idx`, title, is_active)`; `TabItem`-only so no `+`/list/settings mis-detection, FR-9.6 → #5) + `activate_tab(handle)` (foreground window → `SelectionItemPattern.Select`, FR-4.2 → #3). Inline PowerShell `Add-Type UIAutomation*` (acceptable — on-demand, NOT on the 1s poll, FR-10.7). `clean_tab_title` strips Chromium memory-saver suffix. capture `read_tabs` still stub (URL needs DevTools). First unit tests in this crate (`tab_tests`, 5).
- **U6 `vc-app`**: `list_browser_tabs(name)` + `activate_tab(handle)` commands (async, `#[cfg]` dispatch — Windows real, else empty/unsupported), `RunningWindow` reused, registered in `generate_handler!` (now **20** commands).
- **U7 frontend**: `api.ts` `listBrowserTabs`/`activateTab`; `App.tsx` browser-name detection (`BROWSER_APPS`), browser groups always expandable, lazy tab fetch on expand (never on poll → #4/FR-10.7), tab rows click→`activateTab` (+refetch to move active dot), fallback to OS windows + hint row when unreadable (#1 preserved); `styles.css` `.running-window-hint`.
- **Verified (real Windows + Chrome)**: enumerate → 2 tabs, active flag correct, TSV↔parser match; activate idx 1 → `OK`, active flag moved → restored idx 0 (exact-tab, #3). `cargo build -p vc-app` + `clippy -p vc-os-windows -p vc-app --all-targets` 0 warnings; `cargo test -p vc-os-windows` 5/5; frontend `tsc --noEmit` clean. No regression — enumeration/capture/DnD/icons untouched (pure addition → #4). Temp probes `diag_tabs.ps1`/`diag_activate.ps1` deleted.
- **Docs**: `requirements.md` (FR-9/FR-10.12 reconciliation note — UIA scope + limits), `window-enumeration.md` §6, `known-deviations.md` §H + `#B3`/backlog updated, this state, `audit.md` appended.
- [x] Inception artifact update (requirements reconciliation) → [x] Construction (U4→U6→U7 code + design §6) → [x] tests added → [x] build/clippy/tsc/live verification, no regression.

### ✅ DONE (2026-09-09): Supplement Bolt 2 — live tabs as INDIVIDUAL bundle resources
**Trigger**: user request (Korean) — the app can already view a running browser's tab list and jump to a tab; extend so **each tab is registerable as an INDIVIDUAL resource** in a work bundle: (a) every tab individually selectable, (b) a selected tab addable as its own resource, (c) multiple tabs of the SAME window each registerable to the same bundle, (d) selecting a registered tab activates EXACTLY that tab (not the whole app), (e) the same tab must NOT be registered twice, (f) **only use info stably obtainable via the current UIA method — do NOT guess URLs/identifiers that cannot be obtained (FR-9.7)**, (g) no regression in general window registration / DnD / bundle management / restore. Continuation of the per-window + tabs Bolts (`#B3` → `#H2`).
- **Impact analysis first**: connected the live tab to the existing Resource model as a NEW focus-only `ResourceKind::BrowserTabLive` (distinct from URL-carrying capture `BrowserTab`), so registration/activation flow extends additively. vc-os-windows adapter UNCHANGED (reuses §6.2/6.3 `list_tabs`/`activate_tab`) — only domain + vc-app + frontend extended.
- **U1 `vc-core`**: `ResourceKind::BrowserTabLive` added (6→7 variants). `descriptor`=tab title, `hint`=`<browser>\u{1f}<hwnd>\u{1f}<idx>` (non-persistent activation token, serialized — no `#[serde(skip)]`), `reopen_info`=None. Compiler-forced exhaustive matches updated: `distinct_key` (dedup on full hint — same tab=dup, different idx=distinct), `plan_reopen` (→`FocusLinkedWindow`, no URL open), migrate PBT `prop_oneof!` (roundtrip covers 7 variants). New unit tests `test_distinct_live_tabs_by_hint`, `test_plan_reopen_live_tab_is_focus_only`.
- **U6 `vc-app`**: `add_tab_resource(bundle_id,title,browser,handle)` (composes hint, **dedups on full hint** FR-3.4, builds Resource) + `activate_tab_resource(hint,title)` commands. Helpers `split_tab_hint` (hint→`(browser, <hwnd>\u{1f}<idx>)`) + `activate_live_tab` (try stored token via `focus_tab`; if stale, re-enumerate that browser + **re-match by title**; else error — app-only foreground ≠ success, FR-4.2). `reopen_resource` arm `BrowserTabLive`→`activate_live_tab` (restore is focus-only). Registered in `generate_handler!` (now **22** commands). First vc-app unit tests (`tab_hint_compose_split_roundtrip`, `tab_hint_rejects_malformed`).
- **U7 frontend**: `types.ts` `"BrowserTabLive"`; `api.ts` `addTabResource`/`activateTabResource`; `App.tsx` — `kindLabel` "Tab", `isSavedTab`, `draggedTab` ref, poll yields while either drag active, tab rows `draggable` (window rows stay click-only), `onDropToBundle` handles tab drop (`addTabResource`) then app drop, saved tab resource activatable/double-clickable (`activateSavedTab`→`activateTabResource`) with no icon.
- **Verified (real Windows + Chrome)**: harness composed vc-app hint `chrome\u{1f}525722\u{1f}1` from 9 enumerated tabs, split back to exact token, activated idx 1 (active flag moved) → restored idx 0 — exact-tab only (FR-4.2); distinct idx = distinct hint = individually registerable + dedup-able. `cargo build/clippy -p vc-core -p vc-os-windows -p vc-app --all-targets` 0 warnings; `cargo test` vc-core 24/24, vc-os-windows 5/5, vc-app 2/2; frontend `tsc --noEmit` clean. Pure additive → existing app registration / DnD / bundle mgmt / restore unchanged. Harness deleted after verification.
- **Docs**: `requirements.md` (FR-9 보완 정합화 노트 — title-based registration, no URL), `domain-entities.md` (ResourceKind + ResourceIdentity row), `window-enumeration.md` §6.7 + AC 매핑 보완 2, `known-deviations.md` §H2 + `#B3`/§D count(7)/backlog updated, this state, `audit.md` appended.
- [x] Reviewed current implementation + Inception/Construction artifacts → [x] Inception artifact update (domain-entities, requirements) → [x] Construction (U1→U6→U7 code + design §6.7) → [x] tests added → [x] build/clippy/tsc/live verification (individual registration, dedup, exact-tab activation, regression), all green.
