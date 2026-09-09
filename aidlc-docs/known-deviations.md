# 알려진 이탈 (Known Deviations) — 원 설계 대비 실제 구현

작성: 2026-09-08 · 문서 정합화(Documentation Reconciliation) 단계
정합화 기준: **현행 코드 기준 + 이탈 기록** (설계/상태 문서는 실제 구현대로 기술하되, 원 헥사고날 설계 의도와의 차이를 본 문서에 보존)

> 이 문서는 INCEPTION Application Design(`inception/application-design/*`)이 기술한 **원 설계**와 CONSTRUCTION 결과인 **실제 코드** 사이의 확인된 차이를 한곳에 모은다. 각 이탈은 유형으로 분류한다:
> - **[문서반영]** — 코드가 사실상 최종 형태이므로 설계/상태 문서를 코드에 맞게 갱신함(또는 본 문서로 기록). 되돌릴 계획 없음.
> - **[코드백로그]** — 원 설계 의도가 여전히 유효하며, 향후 코드로 구현/보완할 후보. (이번 작업에서 코드는 수정하지 않음)
> - **[수용]** — 원 설계와 다르지만 의도적으로 수용한 결정.

검증 근거: 2026-09-08 읽기 전용 코드 리뷰(vc-core / 어댑터 / vc-app·frontend 3영역 대조). 이후 같은 날 git pull(커밋 `6039456` Windows 사용자 앱만 표시+아이콘, `1abc6ed` 활성화/열거 신뢰성)로 들어온 변경분을 재확인하여 B1/B4/B7 반영.

---

## A. 아키텍처 수준 이탈

| # | 원 설계 | 실제 코드 | 유형 | 근거(file:line) |
|---|---|---|---|---|
| A1 | vc-core가 포트 트레이트 **P1–P8**를 모두 정의(도메인이 능력을 선언, 어댑터가 구현) | vc-core에 `trait` 정의 **0개**. P4는 `vc-sessions`, P5는 `vc-store`에 트레이트로 존재. P1/P3는 트레이트 없이 OS별 구체 struct만. P2/P6/P7/P8은 트레이트로 존재하지 않음 | [문서반영]+[코드백로그] | `vc-core/src/*`(trait 없음), `vc-sessions/src/lib.rs:17`, `vc-store/src/lib.rs:5` |
| A2 | 애플리케이션 서비스 **S1–S7** + `TauriCommandBridge` + `RefreshScheduler`로 유스케이스 오케스트레이션(포트 DI) | 서비스/브리지/스케줄러 struct 부재. 기능이 `AppState` + 17개 `#[tauri::command]` 핸들러 + 자유 함수로 평면화, 어댑터는 `#[cfg(target_os=…)]` 블록에서 **구체 타입 직접 호출** | [문서반영]+[코드백로그] | `vc-app/src/lib.rs:23`(AppState), `:683-701`(invoke_handler), `:542-668`(cfg dispatch) |
| A3 | 프론트엔드는 코어 이벤트를 **구독하는 얇은 뷰**(`status_delta`/`activation_report` emit) | 백엔드 `emit` 없음, 프론트 `listen()` 없음. 프론트가 1초 `setInterval`로 **폴링** | [문서반영]+[코드백로그] | `frontend/src/App.tsx:262` |
| A4 | 도메인 코어(`IdentityMatcher`/`RestorePlanner`/`StatusEvaluator`)를 서비스가 조합 | vc-app이 이들을 **import조차 하지 않음**; 매칭/복원계획/노이즈판정이 오케스트레이션에 연결되지 않음 | [코드백로그] | `vc-app/src/lib.rs:13`(import 목록) |

**해석**: 코드는 헥사고날의 "포트/어댑터"를 트레이트-DI가 아닌 **컴파일타임 `cfg` 분기 + 구체 타입**으로 실현했다. MVP로서는 동작하지만, 도구/OS 확장성·테스트 모킹·이벤트 기반 UI라는 원 설계의 이점은 아직 미실현. 설계 문서에는 "구현 현황" 배너로 현재 구조를 명시하고, 원 설계는 향후 리팩터링 목표로 남긴다.

---

## B. 플랫폼 어댑터 이탈

| # | 원 설계 | 실제 코드 | 유형 | 근거 |
|---|---|---|---|---|
| B1 | Windows 창 열거 = **UI Automation, 최상위 창** | PowerShell `Get-Process`에서 `MainWindowHandle != 0 && MainWindowTitle` 필터로 **사용자가 띄운 가시 창 앱만** 열거(이름 기준 dedup). UI Automation은 구현된 적 없음(설계→`tasklist /v`→`Get-Process` 이력). ⚠ 이 **프로세스당 대표 창 + 이름 dedup**은 FR-2.2/2.4(앱 그룹 안에 창 나열)를 앱 단위로 축소하는 원인 — **G1(진행 중 기능)에서 창 단위 열거로 대체 예정** | [문서반영]→[코드백로그(G1)] | `vc-os-windows/src/lib.rs:57-124` |
| B2 | macOS 창 열거 = **AXUIElement / accessibility 크레이트** | `osascript`/JXA + `NSWorkspace` 셸아웃(네이티브 접근성 의존성 없음) | [문서반영] | `vc-os-macos/src/lib.rs:47-118` |
| B3 | `WinBrowserTabReader`가 Edge/Chrome 탭 수집 | **스텁** — 빈 벡터 반환("Not yet implemented on Windows") | [코드백로그] | `vc-os-windows/src/lib.rs:264-275` |
| B4 | `WinIconProvider`(Windows 아이콘 제공) | ✅ **구현됨** (2026-09-08 커밋 `6039456`) — `WinIconReader`가 PowerShell + C# `Add-Type`(shell32 `SHGetImageList` jumbo 256px → 48px → 32px 폴백, 투명 여백 트림)로 exe 아이콘을 base64 PNG data URI로 추출. vc-app이 `extract_app_icon`에서 호출 + 캐시. **이탈 해소** | [문서반영] | `vc-os-windows/src/lib.rs:127-259`, `vc-app/src/lib.rs:365-372` |
| B5 | Windows 트레이/전역 단축키 훅(FR-10.13, US-10.4) | **부재** | [코드백로그] | vc-os-windows 크레이트 전반 |
| B6 | `MacPermissionChecker`(P6, AC-13) | **부재** — PermissionChecker struct 없음 | [코드백로그] | vc-os-macos 크레이트 전반 |
| B7 | 어댑터 이름 `*WindowActivator` / `*IconProvider` | 활성화는 `*Launcher`(macOS/Windows), 아이콘은 `MacIconReader`/`WinIconReader`로 명명. `open_app`은 실행 전 이미 떠 있는 창 포커스 시도(FR-2.6, 커밋 `1abc6ed`) | [문서반영] | `vc-os-macos/src/lib.rs:174,309`, `vc-os-windows/src/lib.rs:127,278,315` |

**참고**: B3/B5/B6은 여전히 미구현. B4(Windows 아이콘)는 2026-09-08 git pull로 들어온 커밋 `6039456`에서 구현되어 이탈 해소됨. Windows 앱 활성화 중복창 문제도 커밋 `1abc6ed`에서 "실행 중이면 기존 창 포커스" 경로 추가로 해결됨(FR-2.6).

---

## C. 영속성(vc-store) 이탈

| # | 원 설계 | 실제 코드 | 유형 | 근거 |
|---|---|---|---|---|
| C1 | 저장 실패 시 임시 파일 **정리/롤백** | ✅ **해결(2026-09-09)** — 공용 `atomic_write` 헬퍼가 쓰기/rename 실패 시 `.tmp`를 `remove_file`로 정리(누수 없음). `save`가 이 헬퍼 사용 | [문서반영] | `vc-store/src/lib.rs` `atomic_write` |
| C2 | 단일 JSON 저장소가 설정 포함 **원자적 교체** | ✅ **해결(2026-09-09)** — `settings.json`도 `atomic_write`(temp→rename)로 통일. `bundles.json`과 동일한 원자성 + tmp 정리. (설정은 여전히 별도 파일이나 각 파일 쓰기는 원자적) | [문서반영] | `vc-store/src/lib.rs` `save_settings` |
| C3 | `BundleStore`가 `StoreState`에 대한 `load()/save()` | 실제 트레이트는 `Vec<WorkBundle>` 대상 + `load_settings`/`save_settings` 추가. `StoreState` 타입 없음. 버전 태깅·마이그레이션은 vc-store가 아니라 `vc-core/migrate`에 존재 | [문서반영] | `vc-store/src/lib.rs:5-10`, `vc-core/src/migrate/mod.rs:6,44` |

---

## D. 도메인(vc-core) 이탈

| # | 원 설계 | 실제 코드 | 유형 | 근거 |
|---|---|---|---|---|
| D1 | 묶음 내 **중복 식별 금지** 불변식 강제(AC-3, BR-2). `WorkBundle::add_resource -> Result<()>`가 거부, `IdentityMatcher::is_duplicate` 구현 | `add_resource`가 `()` 반환, 검사 없이 `push`(주석: "IdentityMatcher 책임"). `is_duplicate`는 **미구현**. 등록 중복 방지는 vc-app 커맨드 내 인라인으로만 존재 | [코드백로그] | `bundle.rs:82-85`, (`is_duplicate` grep 0건), `vc-app/src/lib.rs:429` |
| D2 | `AppSettings { panel_width, card_height, window_rect: Option<Rect>, card_columns_hint }` | `window_rect`가 `window_x/y/width/height`로 평탄화, `card_columns_hint` **삭제**, Claude 3필드(`claude_api_key/model/region`) 추가 | [문서반영] | `vc-core/src/models/settings.rs:5-26` |
| D3 | 시그니처: `matches -> bool`(component-methods), `evaluate(saved,running,perm,sessions) -> Vec<(ResourceId,ResourceStatus)>`, `load_and_migrate -> StoreState`, `reorder(order: Vec<ResourceId>) -> Result<()>` | `matches -> MatchResult`, `evaluate(&[Resource],&[String],bool) -> Vec<StatusSnapshot>`, `load_and_migrate -> Vec<WorkBundle>`, `reorder(&mut self)`(인자 없이 순번 재부여). `RunningItem`/`PermissionState`/`SessionSnapshot`은 vc-core에 없거나 축소 | [문서반영] | `matching/window.rs:14`, `evaluate/mod.rs:38`, `migrate/mod.rs:14`, `bundle.rs:88` |
| D4 | 파일 레이아웃: `normalize/{url,path,app_id}.rs`, `matching/{signature,distinct}.rs`, `restore/plan.rs`, `evaluate/{status,session}.rs`, `migrate/{rules,transforms}.rs`, `tests/*.rs`, `benches/*.rs` | 각 영역이 단일 `mod.rs`(+`matching/window.rs`)로 통합. `tests/`·`benches/` 디렉터리 없음 | [문서반영] | `vc-core/src/` 트리 |
| D5 | PBT-02(라운드트립)·PBT-03(파서 견고성)이 vc-core `tests/pbt_*.rs` | PBT-02는 `vc-core/migrate/mod.rs`(인라인), **PBT-03는 `vc-sessions/lib.rs:461`**(vc-core 아님) | [문서반영] | `migrate/mod.rs:131-146`, `vc-sessions/src/lib.rs:461-474` |
| D6 | — | ✅ **해결(2026-09-09)** — 미사용 `sha2` 의존성 제거(매칭은 `std::DefaultHasher` 사용, 소스 실사용 0건 확인 후 `vc-core/Cargo.toml`에서 삭제) | [문서반영] | `vc-core/Cargo.toml` |

**일치 확인(이탈 아님)**: `ResourceKind`(6개), `ResourceStatus`(4개), `SessionCompletion`(Waiting/NotWaiting/Unknown) enum은 설계와 **정확히 일치**.

---

## E. 프론트엔드/UX 이탈

| # | 원 설계 | 실제 코드 | 유형 | 근거 |
|---|---|---|---|---|
| E1 | 세션 카드에서 **전체 대화 스크롤 열람**(FR-12.3, AC-17) | ✅ **해결(2026-09-09)** — 세션 카드에 "Log" 버튼 추가 → `ConversationModal`이 `get_session_snapshot`으로 `conversation`(최대 60턴)을 fetch해 스크롤 가능한 모달로 렌더(역할별 말풍선, 최신 턴으로 스크롤). 읽기 전용, 스냅샷이 이미 노출하는 내용만 표시 | [문서반영] | `App.tsx` `ConversationModal` |
| E2 | 레이아웃 설정 재실행 후 유지(FR-8.3/8.5/8.10, AC-14) | ✅ **해결(2026-09-09)** — `get_layout`/`save_panel_layout` 커맨드 배선 + 백엔드 `on_window_event`(Moved/Resized→메모리 stash, CloseRequested/Destroyed→디스크 flush) + 부팅 시 `restore_window_rect`로 창 위치/크기 복원. 프론트: 부팅 시 사이드바 폭 적용 + `ResizeObserver` 디바운스 저장. 창 위치/크기 저장·복원 실측 검증 | [문서반영] | `vc-app/src/lib.rs`(`get_layout`/`save_panel_layout`/`restore_window_rect`/`on_window_event`), `App.tsx` |
| E3 | "캡처" 모델(capture_current) | 프론트는 그룹 생성 + 드래그 모델 사용. `capture_current`/`save_bundles`는 백엔드·`api.ts`에 존재하나 `App.tsx`가 호출 안 함(고아) | [문서반영] | `api.ts:19,24`, `App.tsx`(미호출) |

**일치 확인**: 프론트↔백엔드 커맨드 정합성은 깨끗(프론트가 호출하는 모든 커맨드가 백엔드에 존재; 없는 커맨드 호출 없음).

---

## F. 원 설계에 없는 신규 기능

| # | 내용 | 유형 | 상세 |
|---|---|---|---|
| F1 | **앱 내 Claude 프롬프트 콘솔 (AWS Bedrock 경유)** — `claude.rs` + 4개 커맨드 + `AppSettings` 3필드 + `reqwest` 의존성 + 프론트 콘솔 UI | [수용] | 별도 설계 문서 `construction/vc-app/claude-console/design.md` 신설. **NFR-S1과 충돌**(로컬 전용 vs 외부 HTTPS 송신) → `requirements.md` NFR-S1 개정 + SECURITY 매핑에 보안 노트 추가. 유형은 **[수용]으로 확정**(기능이 의도적으로 도입·동작 중이며 NFR-S3로 규율). 단, 향후 "로컬 전용"이 강제 요구사항으로 격상되면 옵트인 토글 / 기본 비활성으로 완화 재검토 여지 있음. |

---

## G. 진행 중 기능 — 창 단위 모델 재정렬 (2026-09-08 착수)

원 설계는 **창 단위 열거 + 정확한 창 활성화**를 규정했으나(FR-2.2/2.4/2.6, FR-4.1/4.2, 포트 P1/P2, U1 `RunningItem`+`matching/window.rs` L2 매처), `REQUIREMENTS.ko.md §13.1/§13.4`의 사후 명확화와 출하 코드가 이를 **앱 단위**로 축소했다. 사용자 요청(실행 앱 패널 창/탭 펼치기·선택 UX)에 따라 원 창 단위 설계로 **재정렬**한다. 이 항목은 [코드백로그]가 아니라 **[진행 중]** — 코드 수정 예정.

| # | 원 설계(유효) | 현행 코드(축소) | 재정렬 방향 | 근거(file:line) |
|---|---|---|---|---|
| G1 | 앱당 **여러 창**을 각각 열거(제목·핸들·포커스여부); FR-2.2 앱 아이콘 1회+창 제목 나열, FR-2.4 그룹 내 활성 창 녹색점 | `RunningApp { name, bundle_id }` — 창 목록 없음. Windows `visible_window_apps()`가 프로세스당 대표 창 + **이름 dedup**. macOS `list_running_apps()`도 앱 단위 | 어댑터가 **창 단위 스냅샷**(U1 `RunningItem` 형태) 방출 → U6가 앱별 그룹 + 창 리스트로 커맨드 반환 → U7이 펼침/접기 UI. Win: `EnumWindows`/HWND+제목, mac: AX/CGWindowList 창 | `vc-app/src/lib.rs`(`RunningApp`), `vc-os-windows/src/lib.rs:57-124`, `vc-os-macos/src/lib.rs:73-118`, `vc-core/src/matching/window.rs:5-11` |
| G2 | FR-2.6/FR-4.1 **특정 창/탭** 최전면; FR-4.2 앱만 활성화는 성공 아님 | `open_app(target)`이 앱 대상 — `focus_existing_window`가 매칭되는 **아무 창**이나 포커스(창 지정 불가) | 창(HWND/네이티브 핸들) 지정 활성화 커맨드 신설; U6에 per-window activate. Win: 특정 HWND `SetForegroundWindow`, mac: 특정 창 raise | `vc-os-windows/src/lib.rs:278-340`, `vc-app/src/lib.rs`(`activate_app`) |
| G3 | (신규) 앱 클릭 시 창/탭 목록 펼침/접기 — 원 설계·§13에 없던 명시적 어포던스 | 없음(클릭 즉시 활성화) | U7 프론트에 expand/collapse 상태 + 창 하위목록 렌더 (FR-2.8, §13.6, AC-20) | `frontend/src/App.tsx:436-459` |

**용어 주의**: 본 기능의 "창/탭"은 U5 vc-sessions의 코딩 에이전트 **"세션"**과 다른 개념이다. 문서·코드에서 실행 창은 **window/tab**, 코딩 세션은 **session**으로 구분한다.

**미영향(보존 확인 대상)**: FR-3.1 드래그 앤 드롭, Resource/Workspace 묶음(U2 vc-store 영속 스키마), Claude 콘솔(F1)은 이 기능으로 변경되지 않는다.

### G1 확정 진단 (2026-09-08 추가) — 출하 코드가 프로세스당 창 1개만 노출

**증상(사용자 보고)**: 그림판(mspaint) 외에는 창 나누기/창 띄우기가 반영되지 않음. 카카오톡(메인+채팅 창), Edge(창을 나눠도, 한 창에 탭이 여러 개여도) 모두 창 목록에 1개만 표시.

**근본 원인(진단으로 확정)**: G1의 1차 구현은 창 단위 열거를 표방했으나 실제로는 PowerShell `Get-Process | Where MainWindowHandle -ne 0` 방식이다. `MainWindowHandle`은 **프로세스당 대표 창 딱 1개**만 가리킨다. 따라서 **한 프로세스가 여러 최상위 창을 호스팅하는 앱**(카카오톡 = 메인+각 채팅 창, Edge/Chrome = 여러 브라우저 창)은 전부 창 1개로 축소된다. 그림판이 "되는 것처럼 보인" 이유는 그림판은 인스턴스마다 **별도 프로세스**라서 창 2개 = 프로세스 2개였기 때문(=예외 케이스). 즉 1차 구현은 "프로세스-당-창" 앱에서만 우연히 동작했다.

**진단 증거**(`diag_windows.ps1` — `Get-Process.MainWindowHandle` count vs `EnumWindows`(top·non-tool·non-cloaked) count, 실측 2026-09-08):

| 앱 | 프로세스 PID | MainWindowHandle(현행) | EnumWindows(실제 최상위 창) | 갭 |
|---|---|---|---|---|
| msedge | 35552 | 1 | **2** ("AI-DLC 1Day Workshop", "ChatGPT") | -1 |
| chrome | 52688 | 1 | **2** ("해커톤 공부 내용 정리", "GitHub - reiple/vibe-control") | -1 |
| KakaoTalk | 36648 | 1 | 1 (현재 메인만 열림) | 0* |
| explorer(Program Manager) | 51444 | 0 | 0 (`tool`로 정확히 제외됨) | 0 |

*KakaoTalk은 진단 시점에 메인 창만 열려 있어 1개. 채팅 창을 팝아웃하면 같은 PID 아래 최상위 창이 늘어 동일하게 축소된다(원인 동일).

**결론**: 본 항목은 **"연기 가능한 단순화"가 아니라 실사용에서 재현되는 결함**이다. `EnumWindows` 기반 재구현이 **필수**. (1차 구현 시 "1초 폴링을 C# 컴파일러에서 떼어 두려고" 미룬 것은 오판 — 대다수 실제 앱이 프로세스-당-여러-창).

**검증된 수정 방향(내일 바로 착수 가능)**:
1. **열거를 `EnumWindows`로 교체** — 모든 최상위 창을 순회. HWND별 필터(진단으로 검증됨):
   - `IsWindowVisible(h)` 참
   - `GetWindowTextLength(h) > 0` (제목 있음)
   - `GetWindow(h, GW_OWNER=4) == 0` (owned 창 아님 = 최상위)
   - `GetWindowLong(h, GWL_EXSTYLE=-20) & WS_EX_TOOLWINDOW(0x80) == 0` (툴윈도우 아님)
   - `DwmGetWindowAttribute(h, DWMWA_CLOAKED=14) == 0` (클로킹 안 됨 — UWP/가상데스크톱 유령 창 제외)
   - HWND→PID는 `GetWindowThreadProcessId`, PID→exe 경로는 `QueryFullProcessImageNameW`(그룹/아이콘 키)
2. **성능** — 인라인 PowerShell `Add-Type`(C#)는 fresh powershell.exe마다 csc 재컴파일(~150-400ms). 1초 폴링에서 매번 컴파일은 부적절 → **네이티브 Rust FFI(user32/dwmapi 직접 호출)** 권장: `EnumWindows`, `GetWindowTextW`, `GetWindowThreadProcessId`, `IsWindowVisible`, `GetWindow`, `GetWindowLongW`, `GetForegroundWindow`, `DwmGetWindowAttribute`, `QueryFullProcessImageNameW`. 서브프로세스·컴파일 없음. (대안: 캐시된 `Add-Type` 세션을 유지하는 장수 헬퍼 프로세스 — 더 복잡.)
3. **활성화(G2)** — 이미 구현된 `focus_window(hwnd)`는 HWND 지정이라 그대로 유효. 열거가 진짜 HWND들을 주면 다중 창이 각각 활성화된다.
4. **회귀 방지** — `list_running_apps()`(capture용 dedup)와 DnD·아이콘 그룹핑은 그대로. `EnumWindows` 결과를 PID/exe로 그룹핑해 아이콘 1회 규칙 유지.

**진단 산출물**: `diag_windows.ps1`(루트, 임시). 위 필터 휴리스틱이 실증된 스크립트였음 — 재구현 완료 후 **삭제됨(2026-09-09)**.

**별개 문제 B(브라우저 탭)**: "한 창에 여러 탭"은 `EnumWindows`로도 안 보인다 — 탭은 OS 창이 아니라 브라우저 내부 UI. Edge/Chrome 탭 열거는 창 열거와 **범위가 다르며** `known-deviations.md#B3`(BrowserTabReader 스텁, DevTools 프로토콜/UI Automation 필요)의 별도 작업이다. 이번 `EnumWindows` 수정으로 **브라우저 창은 개별 표시되지만 탭은 여전히 창 1개로 묶여** 보인다 — 탭 분리는 후속.

### ✅ G1 해결 (2026-09-09) — `EnumWindows` 네이티브 FFI로 교체 완료

`crates/vc-os-windows/src/lib.rs`의 열거를 `Get-Process.MainWindowHandle`(프로세스당 창 1개) → **Win32 `EnumWindows` 네이티브 Rust FFI**로 재구현했다. 인라인 PowerShell `Add-Type`(매 폴링 csc 재컴파일 ~150-400ms) 대신 `#[link(name=…)]` `extern "system"` 블록으로 user32/dwmapi/kernel32를 직접 링크 — 서브프로세스·컴파일 0(1초 폴링 적합). `windows`/`winapi` 크레이트 미도입(표면이 작고 macOS 빌드에는 `#[cfg(target_os="windows")]`로 미방출).

- **구현**: `winffi` 모듈(FFI 선언) + `enum_windows_cb`(위 검증 필터 그대로: `IsWindowVisible` + `GetWindowTextLengthW>0` + `GetWindow(GW_OWNER)==0` + `!(GWL_EXSTYLE & WS_EX_TOOLWINDOW)` + `!DWMWA_CLOAKED`) + `raw_windows`(창별 행 수집 → PID로 exe 경로/이름 해석). PID→이름은 Toolhelp 스냅샷으로 **보장**(권한 부족 프로세스도 이름 유지), PID→전체경로는 `QueryFullProcessImageNameW` best-effort(실패 시 `name.exe` 폴백 — 기존 동작 보존). HWND는 `(hwnd as usize)` 10진 문자열로 `focus_window`의 전(全)자릿수 가드와 round-trip.
- **계약 무변경**: `raw_windows`만 교체 — `list_running_windows`(이름 dedup 그룹핑, 아이콘 1회)·`list_running_apps`(capture용)·vc-app·프론트 계약 그대로. **최소 blast radius**.
- **실측 검증(2026-09-09, 실 Windows)**: `cargo build -p vc-app` + `clippy --workspace` 0 경고. 전용 하니스(`list_running_windows` 직접 호출) 결과 — **chrome=2창, msedge=2창, KakaoTalk=2창(메인+채팅, 포커스 창 녹색점 정확), WindowsTerminal=2**; mspaint/Code/Obsidian=1(회귀 없음); explorer Program Manager는 `tool`로 정확히 제외. 실 앱 UI 스크린샷 — KakaoTalk·msedge `▾ 2` 펼침 + 창별 하위행·점, 패딩 번호는 앱 행만 카운트. capture dedup·DnD·아이콘 그룹핑 무변경.
- **범위 밖(후속)**: 브라우저 **탭**(한 창 안 여러 탭)은 위 별개 문제 B(`#B3`) — `EnumWindows`로도 안 보임.

---

## H. 빌드/실행 도구 이탈 — `tauri dev` 개발 실행이 문서대로 동작하지 않음

`build-instructions.md`의 개발 실행 절차(`cd crates/vc-app && cargo tauri dev`)가 **그대로는 실패**했다. 사용자 요청("실행 시 프론트엔드와 Rust 앱을 동시에 실행")을 처리하던 중 확인됨.

| # | 문서/설정 | 실제 동작 | 유형 | 근거 |
|---|---|---|---|---|
| H1 | `tauri.conf.json`의 `beforeDevCommand: "npm --prefix ../../frontend run dev"` — `crates/vc-app`에서 실행 시 vite 개발 서버 기동 | 설치된 npm 프리빌트 CLI(`@tauri-apps/cli`)로 `tauri dev` 실행 시, tauri가 `beforeDevCommand`를 **`crates/` 디렉터리(cwd)** 에서 돌려 `../../frontend`가 `D:\claude\frontend`로 잘못 해석 → `npm ENOENT` → vite 미기동 → dev 실패 | [문서반영] (설정 수정 + 문서 갱신) | `crates/vc-app/tauri.conf.json:9`, 실측 오류 로그 2026-09-08 |

**근본 원인**: `beforeDevCommand`의 상대 경로(`../../frontend`)가, 이 워크스페이스에서 npm 프리빌트 CLI가 명령을 실행하는 실제 작업 디렉터리(`crates/`)와 두 레벨 어긋난다. `crates/`에서 `frontend`로 가려면 `../frontend`(한 레벨)여야 한다.

**적용한 수정(2026-09-08, 실측 검증)**:
1. `tauri.conf.json`의 `beforeDevCommand`를 `npm --prefix ../../frontend run dev` → **`npm --prefix ../frontend run dev`** 로 수정. (`frontendDist`/`devUrl`/`beforeBuildCommand` 무변경 → `cargo build -p vc-app` 프로덕션 임베드 경로 **무영향**.)
2. Tauri CLI를 `frontend`에 devDependency로 설치(`@tauri-apps/cli`, 프리빌트). 문서의 `cargo install tauri-cli`(소스 컴파일)는 미설치·미검증이라 프리빌트 경로를 정본으로 문서화.
3. `build-instructions.md` 개발 실행 절차를 검증된 명령(`crates/vc-app`에서 `../../frontend/node_modules/.bin/tauri dev`)으로 갱신. CLI가 `frontend/node_modules`에 있어 `crates/vc-app`에서 `npx tauri`로는 못 찾는 점도 명시.

**검증**: 수정 후 오버라이드 없이 `tauri dev` 단독 실행 → vite `:1420` LISTENING + `vibe-control.exe` 기동 동시 확인(실측 2026-09-08, Windows). 앱 창의 WebView가 `:1420`을 로드, 프론트/Rust 핫리로드 정상.

**범위 밖(미검증)**: cargo 플러그인 `cargo tauri dev`는 이 머신에 미설치라 검증하지 않음 — 해당 도구는 `beforeDevCommand`를 다른 cwd(`crates/vc-app`)에서 돌릴 수 있어 `../frontend`와 어긋날 여지가 있다. cargo 플러그인 도입 시 재확인 필요.

---

## 백로그 (우선순위)

원 설계 의도가 유효하나 코드에 아직 반영되지 않은 항목(이번 정합화에서 **코드는 수정하지 않음**; 향후 반복 후보).

| 우선 | 항목 | 관련 이탈 | 관련 FR/AC |
|---|---|---|---|
| ✅완료 | 창 단위 모델 재정렬 **완료**(G1·G2·G3). 열거를 `EnumWindows` 네이티브 FFI로 교체 — Edge/Chrome/카톡 다중 창이 각각 표시됨(실측 검증, 2026-09-09) | **G1✔, G2✔, G3✔** | **FR-2.2/2.4/2.6/2.8, FR-4.1/4.2, AC-20** |
| ✅완료 | 세션 전체 대화 뷰어 복원 — `ConversationModal`이 fetch된 `SessionSnapshot.conversation`(최대 60턴) 렌더, 세션 카드에 "Log" 버튼 추가(2026-09-09) | **E1 해결** | FR-12.3, AC-17 |
| ✅완료 | 레이아웃 설정 영속화 — `get_layout`/`save_panel_layout` 커맨드 + 백엔드 창 이벤트(이동/리사이즈 시 메모리 갱신, 종료 시 flush) + 부팅 시 창 위치/크기 복원 + 사이드바 폭 저장(2026-09-09) | **E2 해결** | FR-8.10, AC-14 |
| ✅완료 | 저장 실패 시 `.tmp` 정리 + `settings.json` 원자적 쓰기 — 공용 `atomic_write` 헬퍼(temp→rename, 실패 시 temp 제거)로 `save`/`save_settings` 통일(2026-09-09) | **C1·C2 해결** | FR-11.5/11.6, SECURITY-15 |
| P2 | 중복 식별 금지 불변식 도메인화(`add_resource`/`is_duplicate`) | D1 | FR-3.4, AC-3 |
| P2 | Windows 브라우저 탭 읽기(Edge/Chrome) | B3 | FR-10.12, AC-7/8 |
| P3 | Windows 트레이/전역 단축키 | B5 | FR-10.13 |
| P3 | macOS 권한 체커/안내 상태 재확인 | B6 | FR-10.2, AC-13 |
| P3 | 포트 트레이트화 + 서비스/이벤트 리팩터링(헥사고날 복원) | A1–A4 | 아키텍처 |
| ✅완료 | 미사용 `sha2` 의존성 제거 — `vc-core/Cargo.toml`에서 삭제(매칭은 `std::DefaultHasher` 사용, 실사용 0건)(2026-09-09) | **D6 해결** | — |

> **환경 주의(2026-09-09)**: 로컬 `cargo build` 산출 exe(debug·release 모두)는 포트 1420에 dev 서버가 떠 있으면 그 dev 서버를 로드한다(WebView2가 `devUrl`로 연결). 이 머신에는 **별개 저장소 `D:\workspace\nott\vibe-control`의 vite dev 서버가 :1420에서 실행 중**이라, 로컬 exe가 그쪽 프론트엔드(사용량 배너 등)를 표시했다. 이 저장소의 프론트엔드 변경은 tsc·vite 빌드 통과 + 방출 번들에 신규 기능 문자열 포함으로 검증(라이브 창 스크린샷은 :1420 점유로 미실시 — 사용자 dev 서버 미종료). 정식 배포(`tauri build`)는 임베드 자산을 쓰므로 무관.

---

## 관련 문서
- 원 설계: `inception/application-design/components.md`, `services.md`, `component-methods.md`, `unit-of-work.md`
- 신규 기능: `construction/vc-app/claude-console/design.md`
- 창 단위 재정렬(G): `construction/vc-os-windows/functional-design/window-enumeration.md`, `inception/requirements/requirements.md`(FR-2.8/AC-20), `REQUIREMENTS.ko.md`(§13.1/§13.4/§13.6)
- 상태/이력: `aidlc-state.md`, `audit.md`
