# AI-DLC State Tracking

## Project Information
- **Project Type**: Greenfield (built out; now in brownfield doc-reconciliation)
- **Start Date**: 2026-09-07T07:32:51Z
- **Current Stage**: CONSTRUCTION built + post-construction fixes/features landed → **Documentation Reconciliation FINALIZED (2026-09-08)**: design/state docs realigned to actual code; design-vs-code gaps recorded in `known-deviations.md`; **no code modified**. Reconciliation closed per user decision (finalize docs, no further code work). Operations still placeholder.
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
