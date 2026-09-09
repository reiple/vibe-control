# 코드 생성 계획 — U4 vc-os-windows (as-built)
> ⚠ **소급 생성 (2026-09-08, Q5=B)** — 이 계획은 사전 계획이 아니라 **as-built 역작성**이다. `[x]`는 현행 코드(커밋 `d1e0f2f`)에서 실제로 확인된 항목, `[ ]`는 계획 대비 **미이행으로 남은 항목**이다.

### Step 1 구조
- [x] `crates/vc-os-windows/` + `Cargo.toml`(vc-core만 — `windows`/`winapi` 크레이트 **의도적 미도입**)
- [x] 전 코드 `#[cfg(target_os = "windows")]` 게이트 (macOS 빌드에 미방출)

### Step 2 네이티브 FFI
- [x] `winffi` 모듈 — `#[link(name="user32"/"dwmapi"/"kernel32")] extern "system"`
- [x] 심볼: `EnumWindows`, `IsWindowVisible`, `GetWindowTextW/LengthW`, `GetWindow`, `GetWindowLongW`, `GetWindowThreadProcessId`, `GetForegroundWindow`, `DwmGetWindowAttribute`, `OpenProcess`/`CloseHandle`/`QueryFullProcessImageNameW`, `CreateToolhelp32Snapshot`/`Process32FirstW/NextW`

### Step 3 열거 (열거 방식 3차 개정)
- [x] ~~`tasklist /v`~~(행 발생) → ~~`Get-Process.MainWindowHandle`~~(프로세스당 창 1개) → **`EnumWindows`**
- [x] `enum_windows_cb` 필터: visible ∧ 제목 있음 ∧ owner 없음 ∧ ¬toolwindow ∧ ¬cloaked
- [x] `raw_windows()` — HWND→PID(`GetWindowThreadProcessId`), PID→이름(Toolhelp, **보장**), PID→경로(`QueryFullProcessImageNameW`, best-effort + 폴백)
- [x] `list_running_windows()` — 이름 기준 그룹핑(아이콘 1회)
- [x] `list_running_apps()` — 위에서 dedup 재유도(캡처 경로 무변경)
- [ ] `list_running()` — **호출처 없음(죽은 API)** → `#H3`

### Step 4 활성화
- [x] `WinLauncher::open_app` — `focus_existing_window` 우선, 없으면 `Start-Process` (FR-2.6 중복창 방지)
- [x] `::focus_window(hwnd)` — 전자릿수 숫자 검증 → **환경변수 전달**(주입 방지) → `IsWindow` 가드 → `ShowWindowAsync`+`AppActivate(pid)`+`SetForegroundWindow`
- [x] `::open_path / open_url / run_in_terminal / activate_terminal`
- [x] `CREATE_NO_WINDOW`로 콘솔 플래시 억제

### Step 5 아이콘
- [x] `WinIconReader::icon_data_uri` — PowerShell + C# `Add-Type`, shell32 `SHGetImageList` 256→48→32 폴백 + 투명 여백 트림 → base64 PNG

### Step 6 미이행
- [ ] `WinBrowserTabReader::read_tabs()` — **스텁(빈 벡터)** → `#B3`, `#D-40` (FR-10.12, AC-7/8)
- [ ] 트레이/전역 단축키 → `#B5` (FR-10.13 — **v1.1에서 범위 제외로 개정**)
- [ ] **테스트 0개 / 782 LOC** → ⏸ `#H5-f`
