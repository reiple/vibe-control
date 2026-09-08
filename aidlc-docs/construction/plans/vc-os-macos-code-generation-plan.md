# 코드 생성 계획 — U3 vc-os-macos (as-built)
> ⚠ **소급 생성 (2026-09-08, Q5=B)** — 이 계획은 사전 계획이 아니라 **as-built 역작성**이다. `[x]`는 현행 코드(커밋 `d1e0f2f`)에서 실제로 확인된 항목, `[ ]`는 계획 대비 **미이행으로 남은 항목**이다.

### Step 1 구조
- [x] `crates/vc-os-macos/` + `Cargo.toml`(vc-core만)
- [x] 전 코드 `#[cfg(target_os = "macos")]` 게이트

### Step 2 셸아웃 기반
- [x] `run_osascript(script)` / `run_jxa(script, args)` — OS 상호작용 단일 통로

### Step 3 열거
- [x] `MacWindowEnumerator::list_running()` — 앱 이름 목록
- [x] `::list_running_apps()` — `(name, bundle_id)`
- [x] `::list_running_windows()` — `NSWorkspace` 앱 목록 + System Events 창 제목/frontmost 오버레이, 핸들 = `name\u{1f}title`
- [ ] 원 설계의 **AXUIElement 네이티브 접근성** — 미채택(셸아웃) → `#B2`

### Step 4 활성화
- [x] `MacLauncher::open_app / open_path / open_url`
- [x] `::focus_window(handle)` — 앱 활성화 후 제목 기준 `AXRaise` best-effort
- [x] `::run_in_terminal(cmd)` / `::activate_terminal(hint)`
- [x] `shell_quote`(호출측 U6) 기반 인자 인용 — SECURITY-05

### Step 5 브라우저·아이콘
- [x] `MacBrowserTabReader::read_tabs()` — Safari/Chrome
- [x] `MacIconReader::icon_data_uri(target)`

### Step 6 미이행
- [ ] `MacPermissionChecker`(P6) — FR-10.2 / AC-13 → `#B6`, `#D-42`
- [ ] 호출 타임아웃 상한 / 결과 캐시
- [ ] 테스트 확충(현재 1개)
