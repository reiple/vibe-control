# AI-DLC State Tracking

## Project Information
- **Project Type**: Greenfield
- **Start Date**: 2026-09-07T07:32:51Z
- **Current Stage**: CONSTRUCTION COMPLETE - all 7 units built, tested (24 tests pass, PBT-02/03 pass, clippy 0 warnings), Tauri app compiles and launches → ready for Operations (placeholder)
- **Project Name**: vibe-control (작업 맥락 전환 데스크톱 앱)

## Workspace State
- **Existing Code**: No
- **Programming Languages**: None yet
- **Build System**: None yet
- **Project Structure**: Empty (requirements document only)
- **Reverse Engineering Needed**: No
- **Workspace Root**: /Users/ezitsu/code/ddthon/vibe-control

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
- [x] Functional Design (UI Automation)
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
- [x] Tauri commands: get_bundles, save_bundles, capture_current, get_session_snapshot
- [x] `npm run build` succeeds; `cargo build -p vc-app` succeeds
- [x] App binary launches (window created, no crash)

### Build & Test verification (U1–U6)
- [x] Root workspace Cargo.toml created; missing crate manifests added
- [x] `cargo build --workspace` succeeds
- [x] `cargo test --workspace` — 22 tests pass (20 in vc-core incl. PBT-02/03)
- [x] `cargo clippy --workspace --all-targets` — 0 warnings
- [x] PBT-02 (roundtrip stable) + PBT-03 (parser robust) proptest added to vc-core
- [x] Build & Test instruction docs generated (build-and-test/)

### After all units:
- [x] Build and Test — finalized (workspace builds, 24 tests pass, clippy clean, Tauri app runs)

### 🟡 OPERATIONS PHASE
- [ ] Operations — PLACEHOLDER

### 🔧 Post-Construction Fixes
- [x] **Windows freeze fix (2026-09-08)** — running-apps list was empty and the window went "not responding" then closed. Cause: `WinWindowEnumerator` used `tasklist /v` (hangs for minutes when any window is unresponsive) from a SYNCHRONOUS Tauri command polled every 1s on the UI thread. Fixed by (a) enumerating via PowerShell `Get-Process`/`MainWindowTitle` (no window messaging → no hang, ~1s) in `crates/vc-os-windows/src/lib.rs`, and (b) making OS-touching commands `async` in `crates/vc-app/src/lib.rs` so they run off the main thread. Verified on real Windows: clippy 0 warnings, app stable, `Responding = True`. Detail in `audit.md`. Still open on Windows: browser-tab reading + app-icon extraction.
- [x] **Windows duplicate-window fix (2026-09-08)** — double-clicking a running app opened a NEW window instead of focusing the one already open. Cause: `WinLauncher::open_app` went straight to PowerShell `Start-Process`, which always spawns a new instance; there was no "focus if already running" path (macOS `open` gets this for free, Windows does not) — violating FR-2.6 / §13.4. Fixed in `crates/vc-os-windows/src/lib.rs`: `open_app` now calls new `focus_existing_window(target)` first (finds a windowed process matching the target's ProcessName or exe Path, then restores + foregrounds it via Win32 `ShowWindowAsync`/`SetForegroundWindow` + `WScript.Shell.AppActivate`, all inline PowerShell — no native crate), and only falls back to `Start-Process` when no instance is running. Verified on real Windows: clippy 0 warnings; live test — running app → focused with no duplicate process (count before==after), bogus target → still launches. Detail in `audit.md`.
