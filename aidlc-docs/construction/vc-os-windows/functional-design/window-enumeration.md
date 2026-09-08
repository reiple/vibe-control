# 창 단위 열거 + 창 지정 활성화 설계 — U4 vc-os-windows (+ U3/U6/U7 계약)

단계: CONSTRUCTION — Functional Design (per-unit, U4)
착수: 2026-09-08 · 관련: `known-deviations.md#G1`/`#G2`/`#G3`, 요구 FR-2.2/2.4/2.6/2.8, FR-4.1/4.2, AC-20, `REQUIREMENTS.ko.md §13.1/§13.4/§13.6`

> **배경**: 원 설계(포트 P1 `WindowEnumerator`, P2 `WindowActivator`, U1 `RunningItem`+`matching/window.rs` L2 매처)는 **창 단위**를 규정했으나, 출하 코드는 Windows `Get-Process`(프로세스당 대표 창, 이름 dedup)와 앱 대상 `open_app`으로 **앱 단위**에 머물렀다. 본 문서는 창 단위 열거 + 창 지정 활성화로의 재정렬 설계를 정의한다. **용어**: 여기서 다루는 "창/탭(window/tab)"은 vc-sessions의 코딩 "세션"과 무관하다.

---

## 1. 데이터 계약 (어댑터 → vc-app → 프론트)

U1 `RunningItem`(이미 정의됨, `vc-core/src/matching/window.rs`)을 어댑터 방출 형태로 삼는다:

```
RunningItem { app_id, role, title, native_handle, is_focused, group_key, kind_hint }
```

- **app_id / group_key**: 앱 그룹핑 키(프로세스명 또는 exe 경로 정규화). 아이콘·그룹 헤더는 group_key당 1회(FR-2.2).
- **native_handle**: 비영속 힌트 — Windows는 **HWND**(문자열화), macOS는 창 식별자. `ResourceIdentity.hint`와 동일 성격(영구 식별자 아님, FR-11.4).
- **title**: 사용자에게 보이는 구분 가능한 제목(창 제목 / 탭 제목·주소).
- **is_focused**: 현재 최전면 창 여부 → 그룹 내 녹색 점(FR-2.4).

vc-app(U6)은 이를 앱별로 그룹핑하여 프론트에 반환한다. 제안 형태(구현 시 확정):

```
RunningAppGroup { app_id, display_name, bundle_id, windows: Vec<RunningWindow> }
RunningWindow  { handle: String, title: String, is_focused: bool }
```

- 창이 0개인 앱은 목록에서 제외(FR-2.7). 창이 1개여도 동일 구조(`windows.len()==1`) — 프론트가 특수 분기하지 않음(FR-2.8 일관성).
- 종료된 창은 다음 폴링(FR-7.4)에서 재열거로 자연 제거.

---

## 2. Windows 창 열거 (U4)

### 2.0 현재 상태 (2026-09-08) — 1차 구현은 결함으로 확정, 재구현 필요

1차로 출하된 `LIST_WINDOWS_SCRIPT`는 여전히 `Get-Process | Where MainWindowHandle -ne 0`(프로세스당 **대표 창 1개**)에 기반한다. 이는 **창 단위가 아니라 프로세스-당-대표-창**이며, 한 프로세스가 여러 최상위 창을 호스팅하는 앱(카카오톡 메인+채팅, Edge/Chrome 다중 창)을 창 1개로 축소한다. 그림판은 인스턴스=프로세스라서 우연히 동작했을 뿐. **실사용 재현 결함**으로 확정 — 상세 진단·증거는 `known-deviations.md#G1`("G1 확정 진단"). 아래 2.1이 정식 재구현 설계다.

> **✅ 구현 완료 (2026-09-09)**: 아래 2.1 설계대로 `crates/vc-os-windows/src/lib.rs` `raw_windows()`를 네이티브 Rust FFI(`#[link]` `extern "system"` → user32/dwmapi/kernel32) 기반 `EnumWindows`로 교체했다. 인라인 C# 미사용(폴링당 컴파일 0), `windows`/`winapi` 크레이트 미도입. `list_running_windows`/`list_running_apps` 계약 무변경. 실측: chrome=2·msedge=2·KakaoTalk=2(메인+채팅)·WindowsTerminal=2 창, mspaint/Code/Obsidian=1(회귀 없음), Program Manager는 `tool`로 제외. 상세는 `known-deviations.md#G1` "✅ G1 해결".

### 2.1 정식 설계 — `EnumWindows` 기반 창 단위 열거 (구현 완료)

- **방식**: Win32 `EnumWindows`로 **모든 최상위 창**을 순회하고, 각 HWND에 다음 필터(진단 `diag_windows.ps1`로 실증)를 적용:
  - `IsWindowVisible(h)` 참
  - `GetWindowTextLength(h) > 0` (제목 있음)
  - `GetWindow(h, GW_OWNER=4) == 0` (owned 창 아님 = 최상위)
  - `GetWindowLongW(h, GWL_EXSTYLE=-20) & WS_EX_TOOLWINDOW(0x80) == 0` (툴윈도우 아님)
  - `DwmGetWindowAttribute(h, DWMWA_CLOAKED=14) == 0` (클로킹 안 됨 — 가상 데스크톱/UWP 유령 창 제외)
  - HWND→PID = `GetWindowThreadProcessId`; PID→exe 경로 = `QueryFullProcessImageNameW` (그룹/아이콘 키). 최전면 여부 = `GetForegroundWindow()`와 HWND 비교.
  - 이 필터셋은 실측에서 `explorer`의 Program Manager를 `tool`로 정확히 제외하고, Edge/Chrome의 실제 창 2개를 모두 잡음(`known-deviations.md#G1` 증거표 참조).
- **구현 권장 = 네이티브 Rust FFI** (인라인 C# `Add-Type` 아님): 1초 폴링에서 fresh powershell.exe마다 csc 재컴파일(~150-400ms)은 부적절. `user32`/`dwmapi`를 직접 링크(`EnumWindows`, `GetWindowTextW`, `GetWindowThreadProcessId`, `IsWindowVisible`, `GetWindow`, `GetWindowLongW`, `GetForegroundWindow`, `DwmGetWindowAttribute`, `QueryFullProcessImageNameW`)하면 서브프로세스·컴파일 없이 즉시 열거. (대안: 캐시된 `Add-Type` 세션을 유지하는 장수 헬퍼 프로세스 — 더 복잡, 비권장.) 기존 `focus_window`/`WinIconReader`의 인라인 C# 패턴과 달리 열거는 매초 호출되므로 FFI가 정답.
- **그룹핑**: 결과를 PID→exe 경로로 그룹핑 → `app_id`/아이콘 그룹 키. 아이콘·헤더는 그룹당 1회(FR-2.2). `list_running_apps()`(capture용)는 그룹 대표만 dedup해 하위호환.
- **보안(NFR-S2 / SECURITY-05)**: 열거는 입력 인자 없음(신뢰 경계 문제 없음). 활성화(`focus_window`)의 HWND는 계속 `$env:VC_HWND`로 전달, 명령줄 연결 금지.
- **성능(NFR-Pf2/Pf3)**: 열거는 창 메시지 전송 없이 속성 읽기만(과거 `tasklist /v` hang 회피). 네이티브 FFI면 폴링당 서브프로세스 0개. 처리 창 수 상한 고려.

---

## 3. Windows 창 지정 활성화 (U4)

현행 `open_app(target)`(앱 대상, 아무 창 포커스)에 더해 **HWND 지정 포커스** 경로를 추가한다.

- **신규**: `focus_window(hwnd: &str) -> bool` — 주어진 HWND에 대해 `ShowWindowAsync(hwnd, SW_RESTORE=9)` + `SetForegroundWindow` + `WScript.Shell.AppActivate`(PID 폴백), 기존 `focus_existing_window`의 검증된 포커스 시퀀스를 특정 HWND에 적용. HWND는 `$env:VC_HWND`로 전달.
- **FR-4.2 준수**: 창 지정 활성화가 대상 HWND를 최전면화하지 못하면 성공으로 보고하지 않는다(앱만 활성화 ≠ 성공). HWND가 이미 사라졌으면 실패 반환 → U6가 재열거/재실행 판단.
- **기존 경로 유지**: 창 목록이 비어 있는(=실행 중 아님) 앱을 다시 여는 경우는 기존 `open_app`→`start_process` 폴백 사용(FR-4.3, §13.4 개정).

---

## 4. macOS 대응 (U3 — 별도 상세는 `../vc-os-macos/functional-design/accessibility-api.md`)

- 열거: `NSWorkspace` 앱 목록 + 앱별 AX 창(`AXWindows`) 또는 `CGWindowListCopyWindowInfo`로 창 단위 스냅샷(제목·창 번호). 접근성 권한 필요(AC-13).
- 활성화: 특정 창 raise — 앱 활성화 후 AX `AXRaise`(가능 시) 또는 창 번호 기반 포커스. 기존 `activate_terminal(match_hint)`(코딩 세션 창 포커스)와 동일 계열의 창 지정 로직 재사용.

---

## 5. vc-app(U6) / 프론트(U7) 계약

- **U6**: `list_running_apps`(또는 신규 커맨드)가 `Vec<RunningAppGroup>` 반환하도록 확장. 신규 `activate_window(handle)` async 커맨드(창 지정). 기존 `activate_app(target)`은 하위호환 유지(창 목록 빈 앱 재실행용). OS 분기는 기존 `#[cfg(target_os=…)]` 패턴.
- **U7**: 앱 그룹 렌더 + 클릭 시 `windows` 펼침/접기 상태 토글(FR-2.8, §13.6). 각 창 행에 제목 + 활성 창 녹색 점(FR-2.4). 창 행 클릭 → `activateWindow(handle)`. `windows.len()==1`도 동일 렌더(펼치면 1행). **드래그 앤 드롭 보존**: 앱 그룹 헤더의 기존 draggable(FR-3.1) + 개별 창 드래그(FR-3.5) 동작 유지, 펼침 토글이 dragStart와 충돌하지 않도록 클릭 vs 드래그 구분.

---

## AC 매핑
- **AC-20**(신규): 다중 창 앱 펼침 → 특정 창 선택 → 정확한 창 최전면; 단일 창 앱 동일 상호작용; 닫힌 창 갱신 후 제거.
- 회귀 방지: **AC-2**(정확한 창/탭 최전면), **AC-3/AC-4**(드래그 1회 등록·같은 앱 다른 창 개별 등록), **AC-7**(닫힌/보조 창 제외).

---

## 6. Windows 브라우저 탭 열거 + 탭 지정 활성화 (U4 보완 Bolt, 2026-09-09)

§5까지의 **창 단위**(G1–G3)에 이어, 지원 브라우저(Chrome/Edge/Brave/Whale)의 **탭 단위** 확인·선택을 보완한다. 탭은 OS 최상위 창이 아니라 브라우저 내부 UI이므로 `EnumWindows`로는 안 보이고, **UI Automation**으로 창의 탭 스트립을 읽어야 한다(FR-10.12). 이 Bolt는 §5 창 단위 모델과 **연속**이며, 창 열거/활성화·capture·DnD·아이콘은 무변경(순수 추가).

### 6.1 접근 방식 — 관리형 UI Automation, 온디맨드
- **왜 UIA인가**: 브라우저 탭 제목/선택 상태/선택 동작은 `System.Windows.Automation`(UIAutomationClient + UIAutomationTypes, .NET Framework 동봉)으로 표준적으로 접근된다. `ControlType.Tab`(탭 스트립) 컨테이너 → `ControlType.TabItem`(탭들). 활성 탭 = `SelectionItemPattern.IsSelected`, 탭 전환 = `SelectionItemPattern.Select`(폴백 `InvokePattern.Invoke`). `TabItem`만 취하므로 `+`/탭 목록/설정 등은 자연 제외(FR-9.6).
- **왜 인라인 PowerShell `Add-Type`가 허용되는가**: 창 열거(§2)는 1초 폴링이라 FFI가 필수였지만, **탭 읽기는 그룹 펼침 시 온디맨드**로만 호출된다(FR-10.7). 따라서 `WinIconReader`와 동일하게 fresh `powershell.exe` + `Add-Type -AssemblyName UIAutomationClient/…` 패턴이 수용 가능(폴링 hot-path 아님). `windows`/`winapi` 크레이트 미도입 원칙 유지.
- **보안(NFR-S2/SECURITY-05)**: 모든 신뢰 밖 값(대상 프로세스명, HWND, 탭 인덱스)은 `$env:VC_PROC`/`$env:VC_HWND`/`$env:VC_TABIDX`로 전달, 명령줄 연결 금지. Rust 측에서 프로세스명=식별자, HWND/인덱스=전(全)자릿수 검증.

### 6.2 열거 — `WinBrowserTabReader::list_tabs(process) -> Vec<(handle_token, title, is_active)>`
- 모든 최상위 `Window`를 순회하며 대상 프로세스명(`chrome`/`msedge`/…)의 창에서 `Tab` 스트립을 찾고 `TabItem`들을 방출. 워밍업(먼저 `FindFirst(Tab)`) + 350ms 대기로 Chromium 지연 트리 생성을 유도.
- 출력: `<hwnd>\t<index>\t<selected 0|1>\t<name>` TSV → Rust가 파싱. **handle_token = `<hwnd>\u{1f}<index>`** (단위 구분자 `\u{1f}`) — 10진 HWND(창 활성화용)와 구분되고, macOS `name\u{1f}title` 관례와 대칭.
- **제목 정리**: Chromium 메모리 세이버 주석(`… - 메모리 사용량 - 348MB` / `… - Memory usage - 120 MB`)을 `clean_tab_title`이 제거(숫자 없는 짧은 라벨 세그먼트만 안전하게 절단 — 실제 " - " 포함 제목은 보존). 빈 제목은 `(제목 없음)`.
- **한계(수용)**: URL 미제공(FR-9.1의 주소 부분은 라이브 탭 불가; FR-9.7에 따라 추측 금지) → 표시·활성화 전용. Chromium 지연 접근성으로 **최전면/관여 창의 탭만** 안정적으로 읽힘 → 백그라운드 전용 창은 빈 결과. 실패·빈 결과는 `Ok(vec![])`로 degrade(caller가 OS 창 목록으로 폴백).

### 6.3 활성화 — `WinBrowserTabReader::activate_tab(handle_token) -> Result<()>`
- 토큰을 `hwnd`/`index`로 분해·검증 → 대상 창을 **먼저 최전면화**(`ShowWindowAsync(SW_RESTORE)` + `SetForegroundWindow`; 이는 Chromium이 그 창의 접근성 트리를 (재)생성하게 하여 이전 백그라운드 창의 탭 스트립도 읽히게 함) → `Tab` 스트립의 `index`번째 `TabItem`을 `Select`(폴백 `Invoke`).
- **FR-4.2 준수**: 창만 최전면화하고 탭 전환 실패면 성공으로 보고하지 않음. `GONE`(창 사라짐)/`NOSTRIP`(스트립 못 읽음)/`BADIDX`(탭 사라짐)/`NOPATTERN`을 각각 에러로 매핑 → U6가 드롭 후 재열거.

### 6.4 vc-app(U6) / 프론트(U7) 계약
- **U6**: 신규 `list_browser_tabs(name) -> Vec<RunningWindow>`(온디맨드), `activate_tab(handle)` async 커맨드. `RunningWindow{handle,title,is_focused}` **재사용**(탭=창과 동형: `is_focused`=활성 탭). `#[cfg(target_os=…)]` 분기 — Windows만 실동작, 그 외 빈 벡터/미지원 에러. `generate_handler!` 등록.
- **U7**: 브라우저 이름(`chrome/msedge/brave/whale`) 그룹은 **항상 펼침 가능**. 펼칠 때 `listBrowserTabs` **지연 호출**(폴링에는 안 태움), 탭이 있으면 탭 행 렌더(클릭→`activateTab`, 이후 재-fetch로 활성 점 갱신), 없으면 OS 창 행으로 폴백 + 안내 문구. 앱 그룹 헤더 DnD·capture 무변경.

### 6.5 테스트
- `vc-os-windows`에 최초 단위 테스트 추가(`tab_tests`): `is_mem_token`(크기 토큰 판별), `clean_tab_title`(메모리 주석 절단 vs 실제 하이픈 제목 보존, ko/en), 토큰 라운드트립(`<hwnd>\u{1f}<index>` ↔ split). UIA 자체는 라이브 브라우저 필요 → 실측 검증으로 대체(§아래).

### 6.6 실측 검증 (2026-09-09, 실 Windows + Chrome)
- 열거: 활성 Chrome 창에서 탭 2개 방출, 활성 탭 `selected=1` 정확, TSV가 Rust 파서와 일치, 메모리 세이버 접미사 존재(정리 로직 적용 대상) 확인.
- 활성화: 인덱스 1로 전환 → `OK`, 재열거 시 활성 플래그가 인덱스 1로 이동 확인 → 인덱스 0으로 복원까지 확인(정확히 지정 탭만 전환 = FR-4.2).
- 회귀: `cargo build -p vc-app`·`clippy -p vc-os-windows -p vc-app --all-targets` 0 경고, `cargo test -p vc-os-windows` 5/5 통과, 프론트 `tsc --noEmit` 무오류. 창 열거(§2)·capture·DnD 코드 무변경(순수 추가).

## AC 매핑(보완)
- **AC-20 확장**: 브라우저 그룹 펼침 → 탭 목록 확인 → 특정 탭 선택 → 정확한 탭 최전면(활성 점 이동). 읽기 불가 창은 OS 창으로 폴백.
- 회귀 방지: **AC-7**(탭 목록/설정 등 비-탭 UI 오인 금지 = `TabItem`만), **AC-2**(정확한 창/탭 최전면), FR-10.7(폴링 미지연).

## 미해결/후속
- 탭 **영속 등록/복원**(작업 묶음에 탭 저장, FR-9.2/9.3·AC-8/9)은 URL이 필요 → capture용 `read_tabs`(DevTools 프로토콜) 별개 작업으로 잔존. 라이브 패널은 표시·활성화 전용.
- macOS 라이브 탭 세션(현재 앱 창 폴백) — AX 기반 탭 열거는 후속.
- U1 `matching/window.rs` L2 매처(`app_id|role|title`)는 저장 리소스↔실행 창 재추적(FR-4.4)에 사용 가능하나 본 기능 범위는 실행 목록 표시·활성화에 한정.
