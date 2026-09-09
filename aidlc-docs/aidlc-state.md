# AI-DLC State Tracking

## Project Information
- **Project Type**: Greenfield (built out; now in brownfield doc-reconciliation)
- **Start Date**: 2026-09-07T07:32:51Z
- **Current Stage**: CONSTRUCTION built + post-construction fixes/features landed → Documentation Reconciliation FINALIZED (2026-09-08) → **NEW FEATURE in INCEPTION doc-reflection (2026-09-08): per-window expansion UX** — realign running-apps panel to the already-designed per-window model (app-grouped list, click-to-expand window/tab sub-list, exact-window activation) + new expand/collapse affordance. Units: U7/U6/U4/U3 primary, U1 supporting; U5/U2 out of scope. Requirements/design docs updated (FR-2.8, AC-20, REQUIREMENTS.ko §13.1/13.4/13.6, known-deviations G1–G3, new vc-os-windows functional-design). **G1/G2/G3 all implemented + verified on real Windows (G1 enumeration replaced with `EnumWindows` native FFI, 2026-09-09) — feature complete; browser-tab splitting (#B3) remains the only follow-up.** Operations still placeholder.
- **Project Name**: vibe-control (작업 맥락 전환 데스크톱 앱)

## Workspace State
- **Existing Code**: Yes — full Rust Cargo workspace + React/Vite frontend implemented
- **Programming Languages**: Rust (workspace crates) + TypeScript/React (frontend)
- **Build System**: Cargo workspace + Vite (Tauri v2)
- **Project Structure**: `crates/{vc-core,vc-store,vc-os-macos,vc-os-windows,vc-sessions,vc-app}` + `frontend/` + `aidlc-docs/`
- **Reverse Engineering Needed**: No (docs authored during construction; reconciled 2026-09-08)
- **Workspace Root**: 개발 머신마다 다름 — 리포 루트 기준으로 기술한다(고정 절대경로를 적지 않는다).
  - 관측된 체크아웃: `C:\Users\gayeon\Documents\coding\vibe-control` (Windows, 실행 검증용) · `/home/trsprs/workspace/nott/vibe-control` (WSL, 2026-09-08 정합화 재실행용 — `cargo` 미설치라 빌드/테스트 불가)

## Code Location Rules
- **Application Code**: Workspace root (NEVER in aidlc-docs/)
- **Documentation**: aidlc-docs/ only
- **Structure patterns**: See code-generation.md Critical Rules

## Extension Configuration
| Extension | Enabled | Mode | Decided At |
|---|---|---|---|
| Security Baseline | Yes | **Full — 명시적 예외 2건** (SECURITY-04 CSP, SECURITY-13의 `deny_unknown_fields`/깊이·크기 상한 조항). **그 외 전 규칙은 차단 제약 유지** | Requirements Analysis → **개정 2026-09-08 (waiver)** |
| Resiliency Baseline | No | — (skipped, rules not loaded) | Requirements Analysis |
| Property-Based Testing | Yes | **Advisory (권고 — 차단 아님)**. 종전 Partial(PBT-02/03/07/08/09 차단)에서 하향. PBT-02/03/07 테스트는 **계속 유지·통과**하되 실행횟수·시드/CI·프론트 프레임워크는 강제하지 않는다 | Requirements Analysis → **하향 2026-09-08 (waiver)** |

> **면제 근거·수용 리스크·재검토 트리거**: `known-deviations.md#H-5`(Waiver Record). 사용자 확정 2026-09-08. 이 조정은 **D-50~D-57 8건에 한정**되며, 다른 규칙의 미준수는 여전히 차단 사유다.

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
- [x] Tauri commands (actual: **18** — 2026-09-08 재검증, `activate_window` 포함. 종전 '17' 표기는 정정됨 `#D-60`) (registered in `vc-app/src/lib.rs` invoke_handler): get_bundles, save_bundles, capture_current, get_session_snapshot, restore_bundle, resume_coding_session, activate_coding_session, list_running_apps, activate_app, **activate_window**, get_app_icon, create_bundle, delete_bundle, add_app_resource, claude_status, set_claude_api_key, set_claude_model, send_claude_message. (`capture_current`/`save_bundles` exist but the UI no longer calls them — `known-deviations.md#E3`)
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

### ✅ DONE (2026-09-09): P1 백로그 — C1/C2 원자적 쓰기 완료 + FR-1.2 상태 정정
**트리거**: "AI-DLC 워크플로우를 확인하고 계속 진행". 워크플로우는 CONSTRUCTION 완료·G1/G2/G3 검증 완료 상태 → 남은 P1 백로그를 실제 코드와 대조.
**드리프트 발견**: 백로그가 stale이었음 — **FR-1.2(묶음에서 리소스 제거)는 이미 완전 배선**(커맨드 `vc-app/src/lib.rs:562` + `api.ts:115` + `App.tsx:644`). 문서 미기재 커맨드(`list_app_children`/`activate_child`/`add_child_resource`/`claude_usage`)도 존재. 실제 미해결 P1: C1, C2, A4(FR-7 evaluate 미배선), B3(브라우저 탭 스텁).
**구현(C1+C2)**: `crates/vc-store/src/lib.rs`에 공유 `atomic_write(path, bytes)` 헬퍼 도입 — temp 쓰기→원자적 rename, **쓰기/rename 실패 시 `.tmp`를 `fs::remove_file`로 정리 후 `Err`**(누수 없음). `save()`·`save_settings()` 둘 다 이를 사용 → `settings.json`도 원자적. 단위 테스트 3종 신설.
**검증**: `cargo test -p vc-store` **5/5 통과**, `cargo clippy -p vc-store --all-targets` **0 경고**, `cargo build --workspace` clean. (실 툴체인 존재: cargo 1.98, node 24, frontend node_modules 설치됨.)
**문서**: `known-deviations.md`(C1/C2 해결 표기, FR-1.2·C1/C2 백로그 행 완료 처리), `audit.md` 로그.
**남은 P1 후보**: A4/FR-7 저장 리소스 상태 표시 배선(`evaluate` 호출), E2/FR-8.10 레이아웃 설정 영속화(get/update_settings), B3 Windows 브라우저 탭.

### ✅ DONE (2026-09-09): G1 window enumeration replaced with `EnumWindows` native FFI
**Outcome**: The last broken piece — enumeration listing one window per process — is fixed. `crates/vc-os-windows/src/lib.rs` `raw_windows()` now walks all top-level windows via Win32 `EnumWindows` through inline native Rust FFI (`#[link]` `extern "system"` to user32/dwmapi/kernel32 — no `windows`/`winapi` crate, no per-poll `csc` recompile). Only `raw_windows()` changed; `list_running_windows`/`list_running_apps`/vc-app/frontend contracts untouched (minimal blast radius). Details + evidence in `known-deviations.md#G1` ("✅ G1 해결").
- [x] **Reviewed yesterday's changes** (`git diff` since `bbc54e7`): UI (G3) + per-window activation (G2) confirmed complete; only enumeration needed the fix.
- [x] **Replaced enumeration**: `Get-Process.MainWindowHandle` → `EnumWindows`. Validated filter (`IsWindowVisible` + `GetWindowTextLengthW>0` + `GetWindow(GW_OWNER=4)==0` + `!(GWL_EXSTYLE & WS_EX_TOOLWINDOW 0x80)` + `!DWMWA_CLOAKED(14)`). HWND→PID `GetWindowThreadProcessId`; PID→exe path `QueryFullProcessImageNameW` (best-effort) + PID→name Toolhelp snapshot (guaranteed, preserves elevated-app names); foreground `GetForegroundWindow`. Grouped by name (icon once). `focus_window(hwnd)` kept as-is.
- [x] **Native Rust FFI** used (not inline C# `Add-Type`) — subprocess/compile-free per 1 s poll.
- [x] **Verified**: `cargo build -p vc-app` + `cargo clippy --workspace` 0 warnings; harness (`list_running_windows`) → Chrome=2, msedge=2, KakaoTalk=2 (main+chat, focused dot correct), WindowsTerminal=2, mspaint/Code/Obsidian=1 (no regression), explorer Program Manager excluded as tool window; live app screenshot → KakaoTalk/msedge `▾ 2` expand to per-window sub-rows with dots, pad numbering counts app rows only.
- [x] **Browser TABS remain OUT of scope** (`#B3`): windows now split; multiple tabs in one window still show as one (separate BrowserTabReader work).
- [x] Updated `known-deviations.md#G1` (resolved), `aidlc-state.md`, `audit.md`; deleted `diag_windows.ps1`.

### ✅ DONE (2026-09-08): 개발 실행 도구 이탈 수정 — `tauri dev`가 프론트+Rust 동시 실행
**Outcome**: 문서의 개발 실행 절차(`cargo tauri dev`)가 그대로 실패하던 문제를 해결. 근본 원인은 `tauri.conf.json`의 `beforeDevCommand` 상대 경로(`../../frontend`)가 실제 실행 cwd(`crates/`)와 어긋나 vite 서버가 안 뜬 것. 상세·검증은 `known-deviations.md#H1`.
- [x] `beforeDevCommand` `../../frontend` → `../frontend` 수정 (`frontendDist`/`devUrl` 무변경 → 프로덕션 임베드 빌드 무영향).
- [x] Tauri 프리빌트 CLI(`@tauri-apps/cli`)를 `frontend` devDependency로 설치(문서의 `cargo install tauri-cli`는 미검증이라 프리빌트를 정본화).
- [x] **검증**: 오버라이드 없이 `tauri dev` → vite `:1420` LISTENING + `vibe-control.exe` 동시 기동 실측(Windows). 핫리로드 정상.
- [x] `build-instructions.md` 개발 실행 절차 갱신 + `known-deviations.md#H` 이탈 기록 + `audit.md` 로그.
- [ ] (미검증) cargo 플러그인 `cargo tauri dev`는 cwd가 달라 `../frontend`와 어긋날 여지 — 도입 시 재확인.
## Reverse Engineering Status
- [x] Reverse Engineering — Completed on 2026-09-08T22:15:00Z (re-run, 사용자 명시 요청)
- **Artifacts Location**: `aidlc-docs/inception/reverse-engineering/`
- **Analyzed Commit**: `d1e0f2f` (main, clean tree)
- **Artifacts**: business-overview.md · architecture.md · code-structure.md · api-documentation.md · component-inventory.md · technology-stack.md · dependencies.md · code-quality-assessment.md · reverse-engineering-timestamp.md · **drift-analysis.md** · reconciliation-questions.md
- **Analysis constraint**: 본 세션 환경(WSL/Linux)에 `cargo` 미설치 → 빌드/테스트/clippy **재실행하지 못함**. 실행 검증 수치는 이전 실 OS 기록의 인용.
- **Drift 결과**: 45건 (신규 27 / 기존 유지 18) — 의도된 신규기능 3 · 구현결함 5 · 아키텍처위반 7 · 요구사항위반 13 · NFR위반 8 · 문서전용 9
- **Status**: ⏸ **사용자 승인 대기** — 범위 결정 전까지 코드·기준선 문서 **무수정**. 결정 질문: `inception/reverse-engineering/reconciliation-questions.md`


---

## 🔁 정합화 재실행 #2 (2026-09-08) — Reverse Engineering 재실행 + 드리프트 해소

트리거: 사용자 요청("vibe coding 이후 코드가 승인 산출물과 어긋났을 수 있으니 RE를 다시 돌리고 드리프트를 분류·영향분석하라").
방침: **기준선 = 승인 산출물**, 문서를 코드에 맞춰 자동 개정하지 않음, 코드 무수정, 영향분석 승인 후 진행.

### 날짜 표기 정정 (`#D-64`)
종전 상태/감사 로그에 `2026-09-09` 항목이 다수 있었으나 **모든 커밋 날짜는 `2026-09-08`** 이다(`git log --date=short`). 해당 표기는 작업 세션 구분용 라벨이며 실제 날짜가 아니다 — 이후 기록은 커밋 날짜를 기준으로 한다.

### 결과
- **드리프트 45건** (신규 27 / 기존 유지 18): 의도된 신규기능 3 · 구현결함 5 · 아키텍처위반 7 · 요구사항위반 13 · NFR위반 8 · 문서전용 9
- 산출물: `inception/reverse-engineering/` (RE 9종 + `drift-analysis.md` + 질문지 2종)

### 사용자 결정
| 질문 | 답 | 적용 |
|---|---|---|
| Q1 범위 | A — 옵션 B | ⏸ Q3와 충돌 → 해소 대기 |
| Q2 스플래시 | A — 정식 요구사항 승격 | ✅ 완료 |
| Q3 확장 규칙 8건 | C — 전부 면제 | ⏸ **Q1과 모순** → `reconciliation-clarification-questions.md` |
| Q4 미충족 요구사항 13건 | B — 정식 개정 | ✅ 완료 (requirements v1.1) |
| Q5 누락 CONSTRUCTION 산출물 | B — 소급 생성 | ✅ 완료 |

### 완료된 작업 (코드 무수정)
- [x] **Reverse Engineering 재실행** — `inception/reverse-engineering/` 9개 산출물 (커밋 `d1e0f2f` 기준)
- [x] **드리프트/영향 분석** — `drift-analysis.md`: 발견 45건 전량 + 근거 `file:line` + 요구사항 커버리지 매트릭스 + 단계별 영향 집계 + 범위 옵션 A~D
- [x] **requirements.md → v1.1** — 개정 이력 신설. **FR-13(부팅) + AC-21 신설**. FR-1.1/1.2·3.2·3.3·6.x·7.1~7.3·8.10·10.2·10.12·10.13·12.3 및 AC-7/8/13/14/17을 축소·연기·범위제외로 **정식 개정**(각 항목에 되돌릴 근거 명기)
- [x] **stories.md** — **EPIC-13 / US-13.1**(부팅 스플래시, 시나리오 7개) 신설. 연기·범위제외 시나리오에 배지 표기(삭제하지 않음). JS-1에 미충족 표기
- [x] **known-deviations.md** — **섹션 H 신설**: H1 스플래시(승격 완료) · H2 고DPI 결함(등록·수정 보류) · H3 죽은 API · H4 주석 오류 · **H-5 확장 규칙 8건(결정 대기)**. A2 커맨드 수 17→18 정정, D5 부정확 기술 정정, 백로그 전면 재정렬
- [x] **CONSTRUCTION 산출물 소급 생성 (Q5=B)** — U5 `vc-sessions/{functional-design,nfr-requirements,nfr-design}`, U6 `vc-app/{functional-design,nfr-requirements,nfr-design}`, U7 `frontend/{functional-design,nfr-requirements,nfr-design}`, U3·U4 `nfr-requirements`+`nfr-design`, 코드 생성 계획 6종(U2~U7). 전부 **as-built** 명시
- [x] **vc-core 코드 생성 계획 체크박스 정합** (`#D-68`) — 29개 항목 실측 반영, 미이행 8개만 `[ ]`로 잔존
- [x] **claude-console/design.md 줄 번호 정정** (`#D-61`) — `activate_window` 삽입으로 밀린 4개 커맨드 + 2개 struct
- [x] **build-and-test-summary.md** — 확장 준수표를 요구 vs 실제로 재작성, 테스트 33개/통합 테스트 구조적 불가 명시, **스플래시(AC-21)·창 펼침(AC-20) E2E 체크리스트 신설**, 완료 기준 실태 반영
- [x] **상태 문서 정정** — 워크스페이스 경로(`#D-63`), 커맨드 수(`#D-60`), 날짜 표기(`#D-64`)

### 모순 해소 (2026-09-08 확정)
- [x] **Q1/Q3 모순 해소 — Q3 우선 확정**: 사용자 결정 *"D-50~D-57 전부 면제"*. 확장 규칙 8건은 **승인된 면제(waiver)** 로 기록되고 **코드 수정 없음**
- [x] **확장 설정 하향**: Security Baseline = Full + 명시적 예외 2건 / PBT = Partial(차단) → Advisory(권고). 상세 근거·재검토 트리거는 `known-deviations.md#H-5`
- [x] CQ2(설정 하향 방식)는 미응답 → AI 판단으로 "Security는 좁은 예외, PBT만 하향" 채택. 근거는 질문지에 기록

### 최종 상태
- **드리프트 45건 처리 결과**: 정식 승격 1(스플래시 FR-13/AC-21) · 요구사항 정식 개정 13 · **승인된 면제 8** · 신규 이탈 등록 4(H1~H4) · 문서 정정 9 · 기존 백로그 유지 10
- **미해결 잔여**: 없음 — 모든 발견이 *해소 / 개정 / 면제 / 등록된 백로그* 중 하나로 귀결됨
- **코드 변경**: **1건** — `frontend/src/App.tsx` 폴링 절약(D-37, FR-7.5·7.6/NFR-Pf3). `crates/`는 무수정
  - 검증: `npx tsc --noEmit` 통과 · `npx vite build` 성공(159.29 kB). ⚠ 실 OS 실행 확인은 이 환경(WSL, cargo 없음)에서 불가 — **Windows/macOS에서 E2E 체크리스트 수행 필요**(`build-and-test-summary.md`에 항목 추가됨)
- **다음 반복 후보**(백로그 P1): 묶음에서 리소스 제거 · 레이아웃 설정 영속화 · 저장 리소스 상태 표시 배선 · `.tmp` 정리 + `settings.json` 원자적 쓰기
- **면제 항목 재검토 트리거**: 저장 파일 외부 유입 → SECURITY-13 / 원격 콘텐츠·배포 서명 빌드 → CSP / CI 도입 → PBT-08·SECURITY-10 (`known-deviations.md#H-5`)

### ✅ 정합화 재실행 #2 — **완료 (2026-09-08)**

---

### ✅ DONE (2026-09-09): 백로그 소진 (E1·E2·D6 — feature/backlog-layout-conversation-store)
창 단위 재정렬(G) 이후 남은 백로그 상위 항목을 구현·검증하고 main에 병합했다. **C1·C2(vc-store 원자적 쓰기)는 병합 시점에 main이 이미 반영(PR #7)**되어 있어 중복분은 폐기하고 main 버전을 채택 — 이 브랜치의 실기여는 E2·E1·D6.
- [x] **E2 — 레이아웃 영속화**: `LayoutSettings` DTO(패널폭/카드높이/창 rect, Claude 자격증명 제외) + `get_layout`/`save_panel_layout` 커맨드(`generate_handler!` 등록). 창 위치/크기는 백엔드 `on_window_event`가 Moved/Resized 시 메모리에 stash, CloseRequested/Destroyed 시 1회 디스크 flush(폴링 디스크쓰기 회피). 부팅 시 `restore_window_rect`가 저장 rect를 창에 적용. 프론트: 부팅 시 사이드바 폭 적용(ref 명령형, CSS `resize`와 충돌 방지) + `ResizeObserver` 디바운스(500ms) 저장. **실측 검증(병합 전 브랜치)**: 창을 (150,120)/920×640으로 이동 후 종료 → settings.json에 rect 기록; 재기동 → outer rect 정확히 복원.
- [x] **E1 — 세션 전체 대화 뷰어**: 요구사항 v1.1에서 터미널 포커스로 축소됐던 인앱 뷰어를 재도입. 세션 카드 "Log" 버튼 → `ConversationModal`이 `get_session_snapshot`으로 `SessionSnapshot.conversation`(백엔드가 이미 최대 60턴/턴당 4000자 반환)을 스크롤 모달로 렌더(역할별 말풍선, 최신 턴 스크롤). 기존 "View"(터미널 포커스)/"Resume"과 병행. 읽기 전용.
- [x] **D6 — 미사용 `sha2` 제거**: 소스 실사용 0건 확인 후 `vc-core/Cargo.toml`에서 삭제(매칭은 `std::DefaultHasher`).
- [x] **main 병합**: `origin/main`(랜딩 페이지·인트로 애니메이션·usage-meter/context-activation·vc-store C1/C2 선반영)을 브랜치로 병합, vc-app/lib.rs·frontend(App.tsx/api.ts) 충돌을 양 기능 보존으로 해소(레이아웃 커맨드 + `add_child_resource`/accessibility 공존), vc-store는 main 버전 채택, Cargo.lock 재생성.
- **환경 주의**: 이 머신에 별개 저장소 `D:\workspace\nott\vibe-control`의 vite dev 서버가 :1420에서 실행 중이라, 로컬 `cargo build` exe(WebView2)가 `devUrl`(:1420)로 연결해 그쪽 프론트엔드를 표시한다. 프론트 검증은 빌드 통과 + 방출 번들 문자열 확인으로 수행(사용자 dev 서버 미종료). 정식 `tauri build`는 임베드 자산 사용 → 무관.
- **남은 백로그**: P1(저장 리소스 상태 표시), P2(D1 중복식별 불변식, B3 브라우저 탭, 항목 편집, 묶음 이름 변경, 사용자 문서), P3(B5, B6, A1–A4, 스플래시 고DPI).
