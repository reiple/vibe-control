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
| A2 | 애플리케이션 서비스 **S1–S7** + `TauriCommandBridge` + `RefreshScheduler`로 유스케이스 오케스트레이션(포트 DI) | 서비스/브리지/스케줄러 struct 부재. 기능이 `AppState` + **18개**(`activate_window` 추가 반영, 2026-09-08 재검증) `#[tauri::command]` 핸들러 + 자유 함수로 평면화, 어댑터는 `#[cfg(target_os=…)]` 블록에서 **구체 타입 직접 호출** | [문서반영]+[코드백로그] | `vc-app/src/lib.rs:23`(AppState), `:683-701`(invoke_handler), `:542-668`(cfg dispatch) |
| A3 | 프론트엔드는 코어 이벤트를 **구독하는 얇은 뷰**(`status_delta`/`activation_report` emit) | 백엔드 `emit` 없음, 프론트 `listen()` 없음. 프론트가 1초 `setInterval`로 **폴링** | [문서반영]+[코드백로그] | `frontend/src/App.tsx:262` |
| A4 | 도메인 코어(`IdentityMatcher`/`RestorePlanner`/`StatusEvaluator`)를 서비스가 조합 | vc-app이 이들을 **import조차 하지 않음**; 매칭/복원계획/노이즈판정이 오케스트레이션에 연결되지 않음 | [코드백로그] | `vc-app/src/lib.rs:13`(import 목록) |

**해석**: 코드는 헥사고날의 "포트/어댑터"를 트레이트-DI가 아닌 **컴파일타임 `cfg` 분기 + 구체 타입**으로 실현했다. MVP로서는 동작하지만, 도구/OS 확장성·테스트 모킹·이벤트 기반 UI라는 원 설계의 이점은 아직 미실현. 설계 문서에는 "구현 현황" 배너로 현재 구조를 명시하고, 원 설계는 향후 리팩터링 목표로 남긴다.

---

## B. 플랫폼 어댑터 이탈

| # | 원 설계 | 실제 코드 | 유형 | 근거 |
|---|---|---|---|---|
| B1 | Windows 창 열거 = **UI Automation, 최상위 창** | PowerShell `Get-Process`에서 `MainWindowHandle != 0 && MainWindowTitle` 필터로 **사용자가 띄운 가시 창 앱만** 열거(이름 기준 dedup). UI Automation은 구현된 적 없음(설계→`tasklist /v`→`Get-Process` 이력). ⚠ 이 **프로세스당 대표 창 + 이름 dedup**은 FR-2.2/2.4(앱 그룹 안에 창 나열)를 앱 단위로 축소하는 원인 — **G1(진행 중 기능)에서 창 단위 열거로 대체 예정** | [문서반영]→[코드백로그(G1)] | `vc-os-windows/src/lib.rs:57-124` |
| B2 | macOS 창 열거 = **AXUIElement / accessibility 크레이트** | `osascript`/JXA + `NSWorkspace` 셸아웃(네이티브 접근성 의존성 없음) | [문서반영] | `vc-os-macos/src/lib.rs:47-118` |
| B3 | `WinBrowserTabReader`가 Edge/Chrome 탭 수집 | **대부분 해소(2026-09-09)** — 라이브 좌측 패널용 `list_tabs`(제목+활성여부)·`activate_tab`(정확한 탭 전환)을 **UI Automation**으로 구현(§H). 이어 라이브 탭을 **작업 묶음에 개별 등록**(제목 기반 포커스 전용 `ResourceKind::BrowserTabLive`, URL 미저장 — §H2)까지 구현. 이후 Edge **중첩 Tab 구조·지연 접근성 트리**를 진단·처리(Descendants 수집 + 제한 재시도 + 명시적 포그라운드 폴백)하고 **일반 앱 개별 창 등록**(`ResourceKind::WindowRef` 배선)을 추가(§H4). 잔여: capture용 `read_tabs`(제목+**URL**)만 — UIA가 URL 미제공(FR-9.7 추측 금지)이라 빈 스텁, DevTools 프로토콜 필요 | [문서반영(라이브+등록)]+[코드백로그(capture URL)] | `vc-os-windows/src/lib.rs`(`WinBrowserTabReader`), `vc-app/src/lib.rs`(`add_tab_resource`/`activate_live_tab`) |
| B4 | `WinIconProvider`(Windows 아이콘 제공) | ✅ **구현됨** (2026-09-08 커밋 `6039456`) — `WinIconReader`가 PowerShell + C# `Add-Type`(shell32 `SHGetImageList` jumbo 256px → 48px → 32px 폴백, 투명 여백 트림)로 exe 아이콘을 base64 PNG data URI로 추출. vc-app이 `extract_app_icon`에서 호출 + 캐시. **이탈 해소** | [문서반영] | `vc-os-windows/src/lib.rs:127-259`, `vc-app/src/lib.rs:365-372` |
| B5 | Windows 트레이/전역 단축키 훅(FR-10.13, US-10.4) | **부재** | [코드백로그] | vc-os-windows 크레이트 전반 |
| B6 | `MacPermissionChecker`(P6, AC-13) | **부재** — PermissionChecker struct 없음 | [코드백로그] | vc-os-macos 크레이트 전반 |
| B7 | 어댑터 이름 `*WindowActivator` / `*IconProvider` | 활성화는 `*Launcher`(macOS/Windows), 아이콘은 `MacIconReader`/`WinIconReader`로 명명. `open_app`은 실행 전 이미 떠 있는 창 포커스 시도(FR-2.6, 커밋 `1abc6ed`) | [문서반영] | `vc-os-macos/src/lib.rs:174,309`, `vc-os-windows/src/lib.rs:127,278,315` |

**참고**: B5/B6은 여전히 미구현. B3은 **대부분 해소** — 라이브 탭 표시·활성화(§H) + 작업 묶음 개별 등록(§H2)은 2026-09-09 구현, capture용 URL 수집만 잔존. B4(Windows 아이콘)는 2026-09-08 git pull로 들어온 커밋 `6039456`에서 구현되어 이탈 해소됨. Windows 앱 활성화 중복창 문제도 커밋 `1abc6ed`에서 "실행 중이면 기존 창 포커스" 경로 추가로 해결됨(FR-2.6).

---

## C. 영속성(vc-store) 이탈

| # | 원 설계 | 실제 코드 | 유형 | 근거 |
|---|---|---|---|---|
| C1 | 저장 실패 시 임시 파일 **정리/롤백** | ✅ **해결(2026-09-09)** — `save()`/`save_settings()`가 공유 `atomic_write(path, bytes)` 헬퍼 사용. 쓰기·rename 실패 시 `.tmp`를 `fs::remove_file`로 정리 후 `Err` 반환(누수 없음). 단위 테스트 `test_save_cleans_temp_on_failure`/`test_save_leaves_no_temp_file`로 검증 | [문서반영] | `vc-store/src/lib.rs`(`atomic_write`) |
| C2 | 단일 JSON 저장소가 설정 포함 **원자적 교체** | ✅ **해결(2026-09-09)** — `settings.json`도 `atomic_write`(temp→rename)로 전환. `bundles.json`과 동일한 원자성. 단위 테스트 `test_settings_atomic_roundtrip`로 검증(별도 파일 구조는 유지 — C3 참조) | [문서반영] | `vc-store/src/lib.rs`(`save_settings`→`atomic_write`) |
| C3 | `BundleStore`가 `StoreState`에 대한 `load()/save()` | 실제 트레이트는 `Vec<WorkBundle>` 대상 + `load_settings`/`save_settings` 추가. `StoreState` 타입 없음. 버전 태깅·마이그레이션은 vc-store가 아니라 `vc-core/migrate`에 존재 | [문서반영] | `vc-store/src/lib.rs:5-10`, `vc-core/src/migrate/mod.rs:6,44` |

---

## D. 도메인(vc-core) 이탈

| # | 원 설계 | 실제 코드 | 유형 | 근거 |
|---|---|---|---|---|
| D1 | 묶음 내 **중복 식별 금지** 불변식 강제(AC-3, BR-2). `WorkBundle::add_resource -> Result<()>`가 거부, `IdentityMatcher::is_duplicate` 구현 | `add_resource`가 `()` 반환, 검사 없이 `push`(주석: "IdentityMatcher 책임"). `is_duplicate`는 **미구현**. 등록 중복 방지는 vc-app 커맨드 내 인라인으로만 존재 | [코드백로그] | `bundle.rs:82-85`, (`is_duplicate` grep 0건), `vc-app/src/lib.rs:429` |
| D2 | `AppSettings { panel_width, card_height, window_rect: Option<Rect>, card_columns_hint }` | `window_rect`가 `window_x/y/width/height`로 평탄화, `card_columns_hint` **삭제**, Claude 3필드(`claude_api_key/model/region`) 추가 | [문서반영] | `vc-core/src/models/settings.rs:5-26` |
| D3 | 시그니처: `matches -> bool`(component-methods), `evaluate(saved,running,perm,sessions) -> Vec<(ResourceId,ResourceStatus)>`, `load_and_migrate -> StoreState`, `reorder(order: Vec<ResourceId>) -> Result<()>` | `matches -> MatchResult`, `evaluate(&[Resource],&[String],bool) -> Vec<StatusSnapshot>`, `load_and_migrate -> Vec<WorkBundle>`, `reorder(&mut self)`(인자 없이 순번 재부여). `RunningItem`/`PermissionState`/`SessionSnapshot`은 vc-core에 없거나 축소 | [문서반영] | `matching/window.rs:14`, `evaluate/mod.rs:38`, `migrate/mod.rs:14`, `bundle.rs:88` |
| D4 | 파일 레이아웃: `normalize/{url,path,app_id}.rs`, `matching/{signature,distinct}.rs`, `restore/plan.rs`, `evaluate/{status,session}.rs`, `migrate/{rules,transforms}.rs`, `tests/*.rs`, `benches/*.rs` | 각 영역이 단일 `mod.rs`(+`matching/window.rs`)로 통합. `tests/`·`benches/` 디렉터리 없음 | [문서반영] | `vc-core/src/` 트리 |
| D5 | PBT-02(라운드트립)·PBT-03(파서 견고성)이 vc-core `tests/pbt_*.rs` | 둘 다 **인라인 `#[cfg(test)] mod pbt`**. PBT-02(`prop_roundtrip_stable`)와 PBT-03(`prop_parser_robust`)은 **`vc-core/migrate/mod.rs`에 함께** 있고, `vc-sessions/lib.rs`에도 별도의 PBT-03 계열 2건(`prop_parser_robust`, `prop_lines_robust`)이 있다. ⚠ **2026-09-08 정정**: 종전 기술 "PBT-03는 vc-core 아님"은 **부정확**했다(`migrate/mod.rs:143`에 존재) | [문서반영] | `migrate/mod.rs:131-146`, `vc-sessions/src/lib.rs:456-474` |
| D6 | — | 미사용 `sha2` 의존성(매칭은 `std::DefaultHasher` 사용) | [코드백로그] | `vc-core/Cargo.toml`, `matching/mod.rs:27` |

**일치 확인(이탈 아님)**: `ResourceStatus`(4개), `SessionCompletion`(Waiting/NotWaiting/Unknown) enum은 설계와 **정확히 일치**. `ResourceKind`는 원 설계 6종에서 **7종으로 가법 확장**(2026-09-09 `BrowserTabLive` 추가 — 라이브 탭의 포커스 전용 등록, §H2). 기존 6종은 무변경이므로 이탈이 아니라 **후속 요구사항 대응 확장**.

---

## E. 프론트엔드/UX 이탈

| # | 원 설계 | 실제 코드 | 유형 | 근거 |
|---|---|---|---|---|
| E1 | 세션 카드에서 **전체 대화 스크롤 열람**(FR-12.3, AC-17) | 인라인 대화 패널 제거됨. "View"는 터미널 포커스로 대체, 카드엔 완료 칩+마지막 질문 1줄만. `SessionSnapshot.conversation`은 fetch되나 미렌더 | [코드백로그] | `App.tsx:144-191,353`, `audit.md`(대화 패널 제거 기록) |
| E2 | 레이아웃 설정 재실행 후 유지(FR-8.3/8.5/8.10, AC-14) | 저장 스키마·CSS 리사이즈는 있으나 어떤 커맨드도 레이아웃 설정을 **읽거나 쓰지 않음**(SettingsService 미배선). 실제로 읽고/쓰는 설정은 Claude 3필드뿐 | [코드백로그] | `vc-app/src/lib.rs`(get/update_settings 커맨드 없음) |
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
- **범위 밖(후속)**: 브라우저 **탭**(한 창 안 여러 탭)은 위 별개 문제 B(`#B3`) — `EnumWindows`로도 안 보임. → **§H에서 UI Automation으로 보완 구현(2026-09-09).**

---

## H. 보완 Bolt — Windows 브라우저 탭 라이브 표시·활성화 (2026-09-09) — `#B3` 부분 해소

G1(창 단위)에 이어 사용자 요청("지원 브라우저의 열린 탭을 개별 세션으로 확인·선택; 선택 시 정확히 그 탭 활성화")에 따라, Chrome/Edge/Brave/Whale의 탭을 **관리형 UI Automation**으로 열거·활성화하는 경로를 추가했다. `#B3`(BrowserTabReader 스텁)의 **라이브 표시·활성화 부분을 해소**한다.

- **어댑터(U4)** `crates/vc-os-windows/src/lib.rs` — `WinBrowserTabReader`에 신규:
  - `list_tabs(process) -> Vec<(handle_token, title, is_active)>`: 인라인 PowerShell `Add-Type UIAutomationClient/Types` → 대상 프로세스 창의 `ControlType.Tab` 스트립에서 `TabItem` 방출(TSV `<hwnd>\t<idx>\t<sel>\t<name>`). `TabItem`만 취해 `+`/탭목록/설정 오인 방지(FR-9.6). handle_token = `<hwnd>\u{1f}<idx>`(10진 HWND와 구분). `clean_tab_title`이 메모리 세이버 접미사 절단.
  - `activate_tab(handle_token) -> Result<()>`: 창 최전면화(트리 강제 생성) 후 `SelectionItemPattern.Select`(폴백 `Invoke`). `GONE`/`NOSTRIP`/`BADIDX`/`NOPATTERN` 에러 매핑(FR-4.2 — 창만 최전면화는 성공 아님).
  - capture용 `read_tabs`는 **여전히 스텁**: UIA는 활성 탭 URL만 보이고 배경 탭 URL 미제공, FR-9.7 추측 금지 → URL 필요한 영속 등록(FR-9.2/9.3)은 DevTools 프로토콜 별개 작업으로 잔존.
  - **인라인 `Add-Type` 수용 근거**: 탭 읽기는 **그룹 펼침 온디맨드**로만 호출 → 1초 폴링 hot-path 아님(창 열거는 FFI 필수였던 것과 대비, FR-10.7). `windows`/`winapi` 크레이트 미도입 유지.
- **vc-app(U6)**: 신규 커맨드 `list_browser_tabs(name)`·`activate_tab(handle)`(async, `#[cfg]` 분기 — Windows만 실동작), `RunningWindow` 재사용, `generate_handler!` 등록.
- **프론트(U7)**: 브라우저 이름 그룹은 항상 펼침 가능; 펼칠 때 `listBrowserTabs` 지연 호출(폴링 미포함), 탭 행 클릭→`activateTab`(+재fetch로 활성 점 갱신), 읽기 불가 시 OS 창으로 폴백 + 안내 문구. DnD/capture/아이콘 무변경.
- **한계(수용·문서화)**: ① 개별 탭 URL 없음(제목만) → 라이브 패널은 표시·활성화 전용. ② Chromium 지연 접근성 → 최전면/관여 창의 탭만 안정 노출, 백그라운드 전용 창은 창 목록 폴백(요구사항 #1 창 단위는 유지). 활성화는 대상 창을 먼저 최전면화하므로 백그라운드 창 탭도 전환됨.
- **실측 검증(2026-09-09, 실 Windows+Chrome)**: 탭 2개 열거·활성 탭 플래그 정확, TSV↔Rust 파서 일치; 인덱스 1 전환 `OK` 후 활성 플래그 이동 확인→인덱스 0 복원 확인(정확히 지정 탭만). `cargo build -p vc-app`·`clippy -p vc-os-windows -p vc-app --all-targets` 0 경고, `cargo test -p vc-os-windows` 5/5(신규 `tab_tests`), 프론트 `tsc --noEmit` 무오류. 회귀 없음(순수 추가). 진단 산출물 `diag_tabs.ps1`/`diag_activate.ps1`은 검증 후 삭제.
- **설계 문서**: `construction/vc-os-windows/functional-design/window-enumeration.md` §6.

### H2. 라이브 탭의 작업 묶음 개별 등록 (보완 Bolt 2, 2026-09-09) — `#B3` 추가 해소

§H가 "표시·활성화 전용"으로 남긴 라이브 탭을, 사용자 요청("실행 브라우저의 각 탭을 작업 묶음에 개별 리소스로 등록; 같은 창의 여러 탭 각각 등록; 등록 탭 선택 시 정확히 그 탭 활성화; 같은 탭 중복 금지")에 따라 **작업 묶음에 영속 등록**하도록 보완한다. **URL을 안정적으로 얻을 수 없으므로(FR-9.7 추측 금지) 제목 기반 포커스 전용**으로 구현 — vc-os-windows 어댑터는 **무변경**(§6.2/6.3의 `list_tabs`/`activate_tab` 재사용), 도메인·vc-app·프론트만 가법 확장한다.

- **도메인(vc-core)**: 신규 `ResourceKind::BrowserTabLive`(원 6종→7종, §D "일치 확인"). `descriptor`=탭 제목, `hint`=`<browser>\u{1f}<hwnd>\u{1f}<idx>`(비영속 활성화 토큰, FR-11.4), `reopen_info`=없음(URL 미저장). 컴파일러 강제 exhaustive-match 지점 갱신: `distinct_key`(hint 전체로 구분 — 같은 탭=중복, 다른 인덱스=별개), `plan_reopen`(→`FocusLinkedWindow`, URL 미개방), migrate PBT `prop_oneof!`(라운드트립 대상 7종).
- **vc-app(U6)**: 신규 커맨드 `add_tab_resource(bundle_id,title,browser,handle)`(hint 조합 후 **hint 전체로 중복 방지** FR-3.4 — 중복 검사는 D1/A4대로 vc-app 인라인)·`activate_tab_resource(hint,title)`. 헬퍼 `split_tab_hint`(hint→`(browser, <hwnd>\u{1f}<idx>)`)+`activate_live_tab`: 저장 토큰으로 `activate_tab` 시도 → 낡았으면 그 브라우저 재열거해 **제목으로 재매칭**, 실패 시 에러(FR-4.2 — 앱만 최전면화는 성공 아님). `reopen_resource`에도 `BrowserTabLive`→`activate_live_tab` 분기(복원도 포커스 전용). `generate_handler!`는 이제 **22개** 커맨드.
- **프론트(U7)**: 탭 행을 **draggable**로(창 행은 기존대로 클릭 전용), 드롭 시 `addTabResource` 등록. 저장된 탭 리소스는 아이콘 없이 배지 "Tab", 더블클릭 시 `activateTabResource`로 정확히 그 탭 재포커스. 앱 드래그(`draggedApp`)와 탭 드래그(`draggedTab`)는 별도 ref로 구분, 폴링은 두 드래그 중 어느 것이든 진행 중이면 양보(드래그 취소 방지).
- **테스트**: vc-core `distinct_key`(hint 구분: 동일=중복/다른 인덱스=별개)·`plan_reopen`(BrowserTabLive→FocusLinkedWindow) 신규 단위 테스트. vc-app 최초 단위 테스트(`split_tab_hint` 조합↔분해 라운드트립, malformed 거부). 라운드트립 PBT-02는 신규 종류 포함해 통과.
- **실측 검증(2026-09-09, 실 Windows + Chrome)**: 임시 하니스(`list_tabs`→vc-app 방식 hint 조합→분해→`activate_tab`→재열거)로 Chrome 9탭 열거, 조합 hint `chrome\u{1f}525722\u{1f}1` 분해가 원 토큰과 일치, 인덱스 1로 정확 전환(활성 플래그 이동) 후 원탭 복원 — **정확히 지정 탭만 활성화(FR-4.2)** 확인. `cargo build/clippy -p vc-core -p vc-os-windows -p vc-app --all-targets` 0 경고, `cargo test` vc-core 24/24·vc-os-windows 5/5·vc-app 2/2, 프론트 `tsc --noEmit` 무오류. 순수 가법 추가라 기존 앱 등록/DnD/복원 무변경. 하니스는 검증 후 삭제.
- **설계 문서**: `construction/vc-os-windows/functional-design/window-enumeration.md` §6.7, `construction/vc-core/functional-design/domain-entities.md`(ResourceKind·ResourceIdentity), `inception/requirements/requirements.md`(FR-9 보완 정합화 노트).
- **잔여**: capture용 `read_tabs`(제목+**URL**)만 — DevTools 프로토콜 별개 작업. 라이브 탭의 등록/복원은 본 §H2에서 URL 없이 해소.

### H3. 저장된 탭 활성화 정확도 보완 + 빈 제목 표시 개선 (보완 Bolt H3, 2026-09-09)

**문제 1 — 저장 탭 활성화 시 잘못된 탭 선택 (버그 수정)**

§H2에서 구현한 `activate_live_tab`(vc-app)이 저장된 토큰으로 `focus_tab`을 직접 호출했고, 내부 PowerShell 스크립트는 위치 인덱스로만 `SelectionItemPattern.Select`를 수행했다. 탭을 추가·닫기·순서 변경하면 저장된 `<hwnd>\u{1f}<idx>`의 `idx`가 다른 탭을 가리켜도 스크립트는 `OK`를 반환해 **잘못된 탭을 활성화하면서 에러가 발생하지 않는** 상황이 발생했다 — FR-4.2("앱만 최전면화는 성공 아님")의 정신 위반.

- **수정(vc-app/src/lib.rs `activate_live_tab`)**: `focus_tab` 호출 전에 `enumerate_browser_tab_sessions`로 live tab 목록을 먼저 가져와, 저장된 핸들(`handle`)이 기대 제목(`title`)을 가진 탭을 실제로 가리키는지 확인. 일치 시 빠른 경로로 `focus_tab`; 불일치 시 제목 기반 검색으로 직행.
- **잔여 한계(수용)**: 같은 창에 제목이 동일한 탭이 여러 개이면 첫 번째 매칭을 선택 — URL 없이는 구분 불가(FR-9.7).
- **테스트**: `cargo test` vc-core 24/24·vc-os-windows 5/5·vc-app 2/2. 탭 순서 변경 후 saved tab 활성화 정확도는 수동 검증 필요(이 환경에서 실행 불가 — 미완료로 기록).

**문제 2 — Edge 탭 빈 제목 표시 개선**

vc-os-windows `list_tabs`에서 UIA `Name`이 빈 문자열인 탭을 `(제목 없음)`으로 표시했다. 사용자가 몇 번째 탭인지 알 수 없고, 실제 제목 없는 탭과 읽기 실패를 구분하기 어려웠다.

**확인된 읽기 실패 원인(추정)**: Chromium 지연 접근성 트리 — 배경 창은 350ms warm-up 후에도 UIA `TabItem.Name`이 빈 문자열로 반환될 수 있음. Edge의 sleeping tab 등 브라우저 특정 상태도 기여 가능(live 실행 없어 단정 불가).

- **수정(vc-os-windows/src/lib.rs)**: `(제목 없음)` → `(탭 N번 — 제목 없음)` (N=1기준 현재 위치). 사용자가 어느 탭인지 위치로 파악 가능하고, "진짜 빈 제목"이 아닌 읽기 실패임을 암시.
- **부수 효과(의도된)**: 저장 후 제목 기반 fallback 활성화 시 위치 번호가 달라지면 매칭 실패 → 잘못된 탭 활성화 방지(FR-4.2 일치).
- **한계**: "진짜 빈 제목" vs "읽기 실패" 구분 불가(UIA는 둘 다 빈 문자열). 탭 warm-up 시간(350ms) 증가는 live 검증 없이 적용 보류.

### H4. Edge 중첩 Tab 수집·접근성 트리 준비·일반 앱 개별 창 등록 (보완 Bolt H4, 2026-09-09)

**진단 근거(2단계 진단 스크립트 실측, 읽기 전용).** `diag_edge_tabs.ps1`/`diag_edge_tabs2.ps1`로 §H3 "문제 2"의 추정을 실측 규명했다. Edge에서 **두 결함이 겹친다** — 이 둘은 실측 확정:
1. **중첩 Tab 구조**: 탭 스트립이 바깥 `ControlType.Tab`('탭 표시줄', `TabItem`을 Children=0·Descendants=3으로 보유) 안에 안쪽 무명 `ControlType.Tab`(직계 `TabItem` Children=3)이 든 2중 구조. 기존 `FindFirst(Descendants,Tab)`가 **바깥**을 잡고 `FindAll(Children,TabItem)`→0이라 수집 실패.
2. **지연 접근성 트리**: 백그라운드 Edge 창은 스트립은 있으나 `TabItem`이 아직 생성 안 됨(Children=0·Descendants=0). 포그라운드 전환 시 생성·안정.

일반화하지 않은 가정(미확정): "고정 대기를 늘리면 백그라운드 창도 항상 읽힌다"는 검증 안 됨 → 고정 대기 확대 대신 제한 재시도 + 명시적 포그라운드 폴백으로 설계. Chrome은 실측 재확인 못 함 → 설계로만 회귀 방지(Descendants는 직계 자식도 포함하므로 Chrome 직계 `TabItem`도 그대로 수집).

- **어댑터(U4) `crates/vc-os-windows/src/lib.rs`**:
  - `LIST_TABS_SCRIPT`/`ACTIVATE_TAB_SCRIPT` 모두 탭 스트립 하위 탐색을 `Children`→**`Descendants`**로(중첩 Tab 대응). 범위를 **찾은 탭 스트립 하위로 한정** → 페이지 내부 ARIA `TabItem`은 스트립의 자손이 아니므로 오수집 안 됨(FR-9.6). 두 스크립트가 **동일 탐색·순서**(FindFirst Tab → FindAll Descendants TabItem)를 써서 수집 인덱스와 활성화 인덱스가 정합.
  - 고정 350ms 단발 → **제한 재시도**(warm-up 후 창별 최대 6×150ms 폴링, `TabItem`>0이면 조기 종료). 실패 시 기존대로 빈 결과(창 목록 폴백).
  - `list_tabs(process, bring_to_front)` 파라미터 추가. `VC_TAB_FG='1'`일 때만 `VcFg` P/Invoke(ShowWindowAsync+SetForegroundWindow)로 각 대상 창을 먼저 포그라운드 → 지연 트리 강제 생성. 그룹 펼치기는 `false`(포커스 미탈취, FR-10.7 폴링 미포함 유지), 명시적 "다시 읽기"/저장 탭 활성화만 `true`.
- **vc-app(U6)**:
  - `enumerate_browser_tab_sessions(name, bring_to_front)`·`list_browser_tabs(name, reveal)`에 `reveal` 전달. `activate_live_tab`은 저장 탭 활성화 시 `enumerate_browser_tab_sessions(browser, true)`로 재열거(명시 의도이므로 포그라운드)—§H3의 제목 재검증은 그대로 유지(순서 변경 후 오활성화 방지).
  - **개별 창 등록(신규, `ResourceKind::WindowRef` 배선)**: 커맨드 `add_window_resource(bundle_id,title,app,handle)`(→ `WindowRef`: descriptor=제목, hint=HWND, reopen_info=app; **제목으로 중복 방지** — vc-core `distinct_key(WindowRef)=descriptor`와 정합, HWND는 OS가 재활용하므로 식별키 제외)·`activate_window_resource(hint,title,app)`. 헬퍼 `activate_live_window`: `enumerate_running_windows()`로 재열거해 저장 HWND가 같은 창을 가리키면 포커스, 낡았으면 (app,제목) 재매칭, 없으면 에러(FR-4.2). `reopen_resource`의 `WindowRef`도 이 경로로 분기(기존 `open_app` 공유 해제) — **닫힌 창은 재열지 않음**(범위 외, 에러로 표면화). `generate_handler!`는 이제 **24개** 커맨드.
- **프론트(U7)**:
  - 창 하위 행을 **draggable**로(일반 앱 창 + 브라우저 OS-창 폴백 행 모두) → 드롭 시 `addWindowResource`. `draggedWin` ref 추가, 폴링은 세 드래그(app/tab/win) 중 어느 것이든 진행 중이면 양보.
  - 탭 읽기 실패 폴백 문구를 사유 안내로("백그라운드 창이라 탭을 읽지 못했습니다 — 창 목록을 표시합니다") + **"⟳ 앞으로 가져와 다시 읽기"** 버튼(→ `fetchTabs(name, reveal=true)`). 목록 펼치기만으로는 창을 포그라운드로 가져오지 않음.
  - 저장 `WindowRef` 리소스는 배지 "Window"(앱/창/탭 구분 유지), 더블클릭 시 `activateWindowResource`로 그 창 복귀. 앱 전체 등록(`add_app_resource`) 유지.
- **정적 검증(2026-09-09, 실행 완료)**: `cargo clippy --workspace --all-targets` 0 경고, `cargo test --workspace` 전 크레이트 통과(vc-core 24/24·vc-os-windows 5/5·vc-app 2/2·vc-sessions 8/8·vc-store 2/2), 프론트 `npx tsc --noEmit` 무오류·`vite build` 성공. 2단계 진단 스크립트는 실 Windows+Edge에서 실행해 중첩 Tab·지연 트리를 실측 확정.
- **런타임 검증(미완료 — 이 세션에서 직접 실행 못 함, 검증 완료로 처리하지 않음)**: ① Edge 백그라운드/포그라운드 탭 수집, ② 여러 Edge 창, ③ 탭 순서 변경 후 저장 탭 정확 활성화, ④ Chrome 회귀(직계 `TabItem` 수집 유지), ⑤ 카카오톡 개별 창 등록·복귀. 진단은 실측했으나 앱을 통한 위 UI 플로우 실행은 미수행. 사용자 실환경 확인 필요.
- **설계 문서**: `construction/vc-os-windows/functional-design/window-enumeration.md` §6.9.

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
## H. 2026-09-08 정합화 재실행에서 신규 검출

출처: `inception/reverse-engineering/drift-analysis.md`(발견 45건). 직전 정합화(2026-09-08 오전)가 다루지 않았던 항목만 여기 모은다.

### H-1 ~ H-4 코드 이탈

| # | 내용 | 유형 | 근거 | 처리 |
|---|---|---|---|---|
| **H1** | **부팅 스플래시** — 전체 화면 오버레이가 최초 로드(묶음·Claude 상태·첫 실행앱 스캔)까지 입력 차단, 최소 650ms·하드캡 8초·페이드 후 언마운트. 요구사항·스토리·설계 **어디에도 없던 신규 기능** | [문서반영] — **정식 승격 완료** | 커밋 `7ab755a`; `App.tsx:133-162,234-323,520`; `styles.css`; `index.html:7-14` | ✅ `requirements.md` **FR-13 + AC-21** 신설, `stories.md` **EPIC-13/US-13.1** 신설(사용자 결정 Q2=A) |
| **H2** | **스플래시 고DPI 렌더 결함** — 고DPI Windows에서 WebView2가 초기 프레임을 모니터 배율로 래스터화한 뒤 창 DPI에 동기화 → 스플래시 카드가 잠시 확대되어 보임. 커밋 메시지가 결함으로 명시했으나 **문서에 미등록이었음** | [코드백로그] — **수정 보류** | 커밋 `7ab755a` "Known caveat"; `index.html:7-14` 주석 | 등록만 하고 수정 보류. 로드 완료 후 화면은 항상 정상. 후보 수정: Tauri 창 nudge(생성 직후 1px 리사이즈)로 DPI 동기화 강제 |
| **H3** | **죽은 공개 API** — G1 재구현 이후 `WinWindowEnumerator::list_running()`이 어디서도 호출되지 않음(macOS 동명 함수는 탭 리더가 사용하므로 대칭이 깨짐) | [코드백로그] | `vc-os-windows/src/lib.rs:307`; 호출처 grep 0건 | 제거 또는 `#[allow(dead_code)]` + 사유 주석 |
| **H4** | **`reqwest` 의존성 주석 오류** — "Claude (Anthropic Messages API) client"라 적혀 있으나 실제 대상은 **Bedrock**. 직전 정합화가 후속 과제로 지목했으나 미수정 | [코드백로그] | `vc-app/Cargo.toml` | 주석 1줄 정정 |

### H-5 확장 규칙(Security Full / PBT Partial) 미이행 — ✅ **승인된 면제 (Approved Waiver, 2026-09-08)**

`aidlc-state.md`의 Extension Configuration은 **Security Baseline = Full(전 규칙 차단)**, **PBT = Partial(PBT-02/03/07/08/09 차단)** 로 승인되어 있다. 아래 8건은 그 차단 규칙의 미이행이며, 직전 정합화에서 **전혀 다루어지지 않았다**.

| # | 승인된 규칙 | 현행 코드 | 근거 |
|---|---|---|---|
| H5-a | **SECURITY-13** — serde `deny_unknown_fields`, 중첩 5단계·문자열 1MB 제한 | `deny_unknown_fields` **0건**, 깊이·크기 제한 없음 | `vc-core/src/models/*`, `migrate/mod.rs:14-41` |
| H5-b | **SECURITY-04** — Tauri WebView에 제한적 CSP 적용 | `"csp": null` | `tauri.conf.json:25` |
| H5-c | **PBT-02/03** — 임의 입력 **1000회 이상** | `ProptestConfig` 미설정 → proptest 기본 **256회** | `migrate/mod.rs:131`, `vc-sessions:457` |
| H5-d | **PBT-08** — 시드 로깅 + CI 통합 | CI 자체 부재(`.github/` 없음), 시드 파일 미커밋 | 리포 전역 |
| H5-e | NFR 성능 측정(<1ms/<100ms/<200ms, criterion) | criterion·`benches/` 부재 → **성능 목표 미검증** | `crates/*/Cargo.toml` |
| H5-f | 커버리지 ≥90% | 커버리지 도구 부재. vc-app(795 LOC)·vc-os-windows(782 LOC)·frontend(957 LOC) **테스트 0개** | 정적 카운트 |
| H5-g | **PBT-09** — 프론트 `fast-check` | 미도입(프론트 테스트 러너 자체 없음) | `frontend/package.json` |
| H5-h | **SECURITY-10** — 의존성 취약점 스캔 | lock 파일 커밋은 ✅. 스캔 설정·실행 없음. 미사용 `sha2` 잔존 | `vc-core/Cargo.toml:13` |

#### 면제 결정 (Waiver Record)

**결정**: 사용자 확정(2026-09-08) — *"Q3의 답과 같이 D-50~D-57 전부 면제로 결정"*. Q1(옵션 B, 이행)과의 모순은 **Q3 우선**으로 해소되었다(`reconciliation-clarification-questions.md`).
**유형**: [수용] — 개인 프로젝트/PoC 성격에 맞춘 확장 모드 하향. **코드 무수정.**
**적용 범위**: 아래 8건에 **한정**한다. 다른 SECURITY 규칙(01/03/05/09/11/12/15)은 **여전히 차단 제약으로 유효**하다.

| # | 면제 대상 | 수용하는 리스크 | 재검토 트리거 (면제 해제 조건) |
|---|---|---|---|
| H5-a | SECURITY-13 강화(`deny_unknown_fields`, 깊이·크기 상한) | 손상·조작된 `bundles.json`/`settings.json`이 알 수 없는 필드나 과대 중첩을 담아도 거부되지 않는다. **완화 요인**: 저장 파일은 사용자 본인 소유의 OS config 디렉터리에만 존재하고, 미래 버전 거부와 손상 JSON 오류 처리는 이미 동작한다 | 저장 파일이 사용자 밖에서 오거나(가져오기/동기화/공유) 다중 사용자 환경이 되면 즉시 해제 |
| H5-b | SECURITY-04 CSP (`csp: null`) | WebView에 콘텐츠 보안 정책이 없다. **완화 요인**: 프론트가 렌더하는 값은 전부 로컬 출처(창 제목·앱 이름·세션 파일)이고 외부 CDN을 쓰지 않으며 자산이 로컬 번들이다. **잔여 리스크**: 창 제목·세션 내용은 결국 *외부에서 유입된 문자열*이며, Bedrock 응답도 렌더된다 | 원격 콘텐츠를 로드하거나, 사용자 입력 HTML을 렌더하거나, 배포용 서명 빌드를 만들 때 해제 |
| H5-c | PBT-02/03 실행 **1000회** | 기본 256회로 실행되어 희귀 반례 탐지 확률이 낮다. **완화 요인**: 테스트 자체는 존재하고 통과한다 | PBT 회귀가 실제로 발생하면 해제 |
| H5-d | PBT-08 시드 보존 + CI | 실패 재현이 수동이며, 회귀 방지 자동화가 없다 | CI를 도입하는 시점에 함께 해제 |
| H5-e | 성능 측정(criterion) | 창 열거<500ms 등 **성능 목표가 계속 미검증**으로 남는다. **완화 요인**: 실 OS에서 체감 검증은 반복적으로 수행됨(프리즈 사고 후 `async` 전환 등) | 폴링 지연·UI 프리즈가 재발하면 해제 |
| H5-f | 커버리지 ≥90% | vc-app(795 LOC)·vc-os-windows(782 LOC)·frontend(957 LOC)가 **테스트 0개**로 유지된다. **프로젝트 최대 리스크 지점** | 해당 단위를 리팩터링하거나 결함이 반복되면 해제 |
| H5-g | PBT-09 프론트 `fast-check` | 프론트 자동 테스트가 전혀 없다. **완화 요인**: `tsc`가 빌드에 포함되어 타입 수준 검증은 있다 | 프론트 테스트 러너 도입 시 함께 |
| H5-h | SECURITY-10 취약점 스캔 | 의존성 CVE를 자동 감지하지 못한다. **완화 요인**: `Cargo.lock`·`package-lock.json` 커밋됨(재현 가능 빌드) | 배포/공개 시점, 또는 CI 도입 시 해제 |

**확장 설정 조정**: `aidlc-state.md`의 Extension Configuration을 다음과 같이 조정한다 — Security Baseline은 **Full 유지 + 위 2건(SECURITY-04, SECURITY-13 강화 조항)만 명시적 예외**, PBT는 **Partial(차단) → Advisory(권고)** 로 하향. Security를 통째로 내리지 않는 이유는, 현재 **충족 중인** SECURITY-05/12/15 등이 함께 면제되어 앞으로의 검증에서 빠지는 것을 막기 위함이다.

> ℹ️ **정직한 부기**: H5-b(CSP)는 `tauri.conf.json` 한 줄 변경으로 끝나는 항목이라 비용 대비 효과가 가장 컸다. 사용자에게 그 점을 제시했고(작업량 "매우 작음"), 그럼에도 **전부 면제**로 확정되었다 — 위 재검토 트리거에 도달하면 가장 먼저 되살릴 항목으로 기록해 둔다.

---

## 백로그 (우선순위)

원 설계 의도가 유효하나 코드에 아직 반영되지 않은 항목(이번 정합화에서 **코드는 수정하지 않음**; 향후 반복 후보).

| 우선 | 항목 | 관련 이탈 | 관련 FR/AC |
|---|---|---|---|
| ✅완료 | 창 단위 모델 재정렬 **완료**(G1·G2·G3). 열거를 `EnumWindows` 네이티브 FFI로 교체 — Edge/Chrome/카톡 다중 창이 각각 표시됨(실측 검증, 2026-09-09) | **G1✔, G2✔, G3✔** | **FR-2.2/2.4/2.6/2.8, FR-4.1/4.2, AC-20** |
| ✅완료 | **묶음에서 리소스 제거** 배선 완료 — 코드 감사(2026-09-09)에서 이미 구현되어 있음 확인: `remove_resource` Tauri 커맨드(`vc-app/src/lib.rs:562`) + `removeResource` api 래퍼(`frontend/src/api.ts:115`) + `App.tsx:644` 호출. 종전 백로그 표기가 stale이었음 | ~~신규 `#D-33`~~ | FR-1.2 |
| P1 | 레이아웃 설정 영속화(get/update_settings 배선) | E2 | FR-8.10, AC-14 |
| P1 | 저장 리소스 상태 표시 배선(`evaluate` 호출 + 프론트 `status` 필드) | A4, 신규 `#D-30` | FR-7.1~7.3 |
| ✅완료 | **폴링 절약 구현 완료 (2026-09-08)** — 창이 숨겨짐/최소화 시 폴링 중단, 리사이즈 후 400ms 유예, 복귀 시 즉시 갱신, 진행 중 폴링과 중첩 방지. `frontend/src/App.tsx` | ~~`#D-37`~~ | FR-7.5, **FR-7.6**, NFR-Pf3 |
| P2 | 항목 편집(표시명·재실행 주소) | 신규 `#D-31` | FR-6.1/6.2(연기) |
| P2 | 묶음 이름 변경 | 신규 `#D-32` | FR-1.1(연기) |
| P2 | 사용자 문서(저장 위치·권한·사용법 + Claude 콘솔 외부 전송 고지) | 신규 `#D-36` | NFR-U1, DoD |
| P3 | ~~세션 전체 대화 뷰어 복원~~ → **요구사항 v1.1에서 축소**(터미널 포커스로 대체). 되돌릴 경우에만 착수 | E1 | FR-12.3(개정), AC-17(개정) |
| P3 | 스플래시 고DPI 렌더 보정(Tauri 창 nudge) | **H2** | FR-13(알려진 제약) |
| P4 | 죽은 API `WinWindowEnumerator::list_running` 정리 / `reqwest` 주석 정정 | **H3, H4** | — |
| ✅완료 | 저장 실패 시 `.tmp` 정리 + `settings.json` 원자적 쓰기 **완료(2026-09-09)** — 공유 `atomic_write` 헬퍼(temp→rename, 실패 시 `.tmp` 정리). `cargo test -p vc-store` 5/5 통과·clippy 0 경고 | C1✔, C2✔ | FR-11.5/11.6, SECURITY-15 |
| P2 | 중복 식별 금지 불변식 도메인화(`add_resource`/`is_duplicate`) | D1 | FR-3.4, AC-3 |
| 부분완료 | Windows 브라우저 탭 — 라이브 표시·활성화(§H) + **작업 묶음 개별 등록·정확 활성화·중복 방지 완료**(§H2, 제목 기반 포커스 전용). 잔여: capture용 URL 수집(DevTools 필요) | B3 | FR-9.6/10.12, FR-3.4/3.5/4.1/4.2, AC-20 완료 / FR-9.1·9.3, AC-9 잔여 |
| P3 | Windows 트레이/전역 단축키 | B5 | FR-10.13 |
| P3 | macOS 권한 체커/안내 상태 재확인 | B6 | FR-10.2, AC-13 |
| P3 | 포트 트레이트화 + 서비스/이벤트 리팩터링(헥사고날 복원) | A1–A4 | 아키텍처 |
| P4 | 미사용 `sha2` 의존성 제거 | D6 | — |

---

## 관련 문서
- 원 설계: `inception/application-design/components.md`, `services.md`, `component-methods.md`, `unit-of-work.md`
- 신규 기능: `construction/vc-app/claude-console/design.md`
- 창 단위 재정렬(G): `construction/vc-os-windows/functional-design/window-enumeration.md`, `inception/requirements/requirements.md`(FR-2.8/AC-20), `REQUIREMENTS.ko.md`(§13.1/§13.4/§13.6)
- 상태/이력: `aidlc-state.md`, `audit.md`
